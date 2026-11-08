//! The daemon's end of the connection to the `gestos-input` service.

use crate::input_protocol::Request;
use crate::key_steps::KeyStep;
use crate::modifiers::Modifiers;
use crate::keys::{name_to_keycode, parse_key_combo_on};
use crate::uinput::trigger_button_code;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const HELD_REPLY_TIMEOUT: Duration = Duration::from_millis(200);
// How long each typed key is held, and the pause before the next one. Apps miss keys that come
// faster than they read them.
const TYPED_KEY_HOLD: Duration = Duration::from_millis(8);
const TYPED_KEY_GAP: Duration = Duration::from_millis(8);

#[derive(Clone)]
pub struct InputClient {
    writer: Arc<Mutex<UnixStream>>,
    held: Arc<(Mutex<Option<Modifiers>>, Condvar)>,
}

impl InputClient {
    /// Connects, returning the client and the stream to read the service's events from.
    pub fn connect(path: &str) -> std::io::Result<(Self, UnixStream)> {
        let stream = UnixStream::connect(path)?;
        let reader = stream.try_clone()?;
        Ok((Self::new(stream), reader))
    }

    fn new(writer: UnixStream) -> Self {
        InputClient {
            writer: Arc::new(Mutex::new(writer)),
            held: Arc::default(),
        }
    }

    /// A lost service is noticed by whoever reads its events, so writes needn't report it.
    pub fn send(&self, request: &Request) {
        let mut writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writer.write_all(request.encode().as_bytes());
    }

    fn key(&self, code: u16, pressed: bool) {
        self.send(&Request::Key { code, pressed });
    }

    /// Passes on the service's answer to `held`.
    pub fn answer_held(&self, modifiers: Modifiers) {
        let (slot, arrived) = &*self.held;
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(modifiers);
        arrived.notify_all();
    }

    /// Which modifiers are down. Without an answer in time, none are, so nothing waits forever.
    pub fn held(&self) -> Modifiers {
        let (slot, arrived) = &*self.held;
        *slot.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.send(&Request::QueryHeld);
        let deadline = Instant::now() + HELD_REPLY_TIMEOUT;
        let mut answer = slot.lock().unwrap_or_else(|e| e.into_inner());
        while answer.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            answer = arrived
                .wait_timeout(answer, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        answer.unwrap_or_default()
    }

    pub fn click(&self, button: i32) {
        let code = trigger_button_code(button).unwrap_or(button as u16);
        self.key(code, true);
        std::thread::sleep(Duration::from_millis(50));
        self.key(code, false);
    }

    pub fn keypress_string(&self, keys: &str) {
        let layout = crate::keymap::current_layout_keys();
        let codes = match parse_key_combo_on(keys, layout.as_deref()) {
            Ok(codes) => codes,
            Err(unknown) => {
                eprintln!("gestos: Ignoring shortcut '{}': unknown key '{}'", keys, unknown);
                return;
            }
        };

        // Press all keys in sequence
        for &code in &codes {
            self.key(code, true);
            std::thread::sleep(Duration::from_millis(10));
        }

        std::thread::sleep(Duration::from_millis(50));

        // Release in reverse order
        for &code in codes.iter().rev() {
            self.key(code, false);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Types text with the keys the layout has for each character, Shift and AltGr included, and
    /// its dead keys for accents. A character the layout can't type is skipped, so the rest still
    /// comes out.
    pub fn type_text(&self, text: &str) {
        let layout = crate::keymap::current_layout_keys();
        for c in text.chars() {
            let Some(keys) = layout
                .as_deref()
                .and_then(|layout| layout.keys_for_char(c))
                .or_else(|| us_char_key(c).map(|key| vec![key]))
            else {
                eprintln!("gestos: Can't type '{}' on this keyboard layout", c);
                continue;
            };
            for (key, shift, altgr) in keys {
                self.type_key(key, shift, altgr);
            }
        }
    }

    fn type_key(&self, key: u16, shift: bool, altgr: bool) {
        let mut codes = Vec::new();
        if shift {
            codes.push(evdev::KeyCode::KEY_LEFTSHIFT.0);
        }
        if altgr {
            codes.push(evdev::KeyCode::KEY_RIGHTALT.0);
        }
        codes.push(key);
        for &code in &codes {
            self.key(code, true);
        }
        std::thread::sleep(TYPED_KEY_HOLD);
        for &code in codes.iter().rev() {
            self.key(code, false);
        }
        std::thread::sleep(TYPED_KEY_GAP);
    }

    /// Runs a custom action's steps in order. `open_app` opens an app or brings it to the front,
    /// which only the desktop can do, and says whether it then has focus. If it doesn't, the steps
    /// after it would go to some other window, so they're left out.
    pub fn run_steps(&self, steps: &[KeyStep], open_app: &dyn Fn(&str) -> bool) {
        for step in steps {
            match step {
                KeyStep::Press(keys) => self.keypress_string(keys),
                KeyStep::Type(text) => self.type_text(text),
                KeyStep::Wait(millis) => {
                    std::thread::sleep(Duration::from_millis(u64::from(*millis)));
                }
                KeyStep::Open(app) => {
                    if !open_app(app) {
                        return;
                    }
                }
            }
        }
    }
}

/// Where a character is on a US layout, for when the layout in use can't be read.
fn us_char_key(c: char) -> Option<(u16, bool, bool)> {
    let name = match c {
        ' ' => "space".to_string(),
        '\n' => "Return".to_string(),
        '\t' => "Tab".to_string(),
        c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase().to_string(),
        _ => return None,
    };
    Some((name_to_keycode(&name)?, c.is_ascii_uppercase(), false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_protocol::LineBuffer;
    use std::io::Read;

    fn client() -> (InputClient, UnixStream) {
        let (ours, theirs) = UnixStream::pair().unwrap();
        (InputClient::new(ours), theirs)
    }

    fn received(stream: &mut UnixStream, count: usize) -> Vec<Request> {
        let mut buffer = LineBuffer::default();
        let mut requests = Vec::new();
        let mut bytes = [0u8; 256];
        while requests.len() < count {
            let length = stream.read(&mut bytes).unwrap();
            buffer.push(&bytes[..length]);
            while let Some(line) = buffer.next_line() {
                requests.push(Request::decode(&line).unwrap());
            }
        }
        requests
    }

    #[test]
    fn a_click_presses_and_releases_the_button() {
        let (client, mut service) = client();
        client.click(3);
        assert_eq!(
            received(&mut service, 2),
            [
                Request::Key { code: 0x111, pressed: true },
                Request::Key { code: 0x111, pressed: false }
            ]
        );
    }

    #[test]
    fn a_shortcut_is_released_in_reverse_order() {
        let (client, mut service) = client();
        client.keypress_string("Control_L+F1");
        let requests = received(&mut service, 4);
        let Request::Key { code: first, pressed: true } = requests[0] else {
            panic!("{requests:?}");
        };
        assert_eq!(requests[3], Request::Key { code: first, pressed: false });
    }

    #[test]
    fn steps_press_type_and_wait_in_order() {
        let (client, mut service) = client();
        let started = Instant::now();
        client.run_steps(
            &[
                KeyStep::Wait(30),
                KeyStep::Type("A".to_string()),
                KeyStep::Press("F1".to_string()),
            ],
            &|_| true,
        );
        assert!(started.elapsed() >= Duration::from_millis(30));
        let requests = received(&mut service, 6);
        let Request::Key { code: shift, pressed: true } = requests[0] else {
            panic!("{requests:?}");
        };
        let Request::Key { code: a, pressed: true } = requests[1] else {
            panic!("{requests:?}");
        };
        assert_eq!(requests[2], Request::Key { code: a, pressed: false });
        assert_eq!(requests[3], Request::Key { code: shift, pressed: false });
        assert_eq!(requests[4], Request::Key { code: evdev::KeyCode::KEY_F1.0, pressed: true });
    }

    #[test]
    fn steps_after_an_app_that_doesnt_come_up_are_left_out() {
        let (client, mut service) = client();
        let opened = std::cell::RefCell::new(Vec::new());
        client.run_steps(
            &[
                KeyStep::Open("org.example.Missing".to_string()),
                KeyStep::Press("F1".to_string()),
            ],
            &|app| {
                opened.borrow_mut().push(app.to_string());
                false
            },
        );
        assert_eq!(*opened.borrow(), ["org.example.Missing"]);
        client.send(&Request::QueryHeld);
        assert_eq!(received(&mut service, 1), [Request::QueryHeld]);
    }

    #[test]
    fn a_character_with_no_key_is_skipped() {
        assert_eq!(us_char_key('é'), None);
        assert_eq!(us_char_key('Q'), Some((evdev::KeyCode::KEY_Q.0, true, false)));
    }

    #[test]
    fn an_unknown_key_sends_nothing() {
        let (client, mut service) = client();
        client.keypress_string("Control_L+NoSuchKey");
        client.send(&Request::QueryHeld);
        assert_eq!(received(&mut service, 1), [Request::QueryHeld]);
    }

    #[test]
    fn held_gets_the_services_answer() {
        let (client, mut service) = client();
        let answering = client.clone();
        let answerer = std::thread::spawn(move || {
            assert_eq!(received(&mut service, 1), [Request::QueryHeld]);
            answering.answer_held(Modifiers::parse("ctrl"));
        });
        assert_eq!(client.held(), Modifiers::parse("ctrl"));
        answerer.join().unwrap();
    }

    #[test]
    fn no_answer_counts_as_nothing_held() {
        let (client, _service) = client();
        assert!(client.held().is_empty());
    }
}
