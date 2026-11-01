//! The daemon's end of the connection to the `gestos-input` service.

use crate::input_protocol::Request;
use crate::modifiers::Modifiers;
use crate::keys::parse_key_combo_on;
use crate::uinput::trigger_button_code;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const HELD_REPLY_TIMEOUT: Duration = Duration::from_millis(200);

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
