//! What `gestosd` and the `gestos-input` service say to each other, one line of text per message.
//! The service has the rights to read input devices, so this is all that anything talking to it
//! can do: no key events ever come out of it, only gesture paths and which modifiers are held.

use crate::modifiers::HeldKeys;

pub const SOCKET_PATH: &str = "/run/gestos/input.sock";
pub const MAX_LINE_LENGTH: usize = 4096;
/// How long after a gesture the service says which modifiers are held. It's for waiting until
/// they're released, and a little longer than the daemon waits for that.
pub const HELD_REPORT_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether systemd runs this system. Without it, as on Alpine, OpenRC starts the input service.
pub fn uses_systemd() -> bool {
    std::path::Path::new("/run/systemd/system").exists()
}

/// How to run a command as root here. Alpine has doas, not sudo.
pub fn as_root() -> &'static str {
    let doas_only = std::path::Path::new("/usr/bin/doas").exists()
        && !std::path::Path::new("/usr/bin/sudo").exists();
    if doas_only { "doas" } else { "sudo" }
}

/// The command that starts the input service, now and at every boot.
pub fn start_service_command() -> String {
    let root = as_root();
    if uses_systemd() {
        format!("{root} systemctl enable --now gestos-input.socket")
    } else {
        format!("{root} rc-update add gestos-input && {root} rc-service gestos-input start")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Open {
        button: i32,
        sensitivity: i32,
        keyboards: bool,
        device: String,
    },
    SetKeyboards(bool),
    /// Whether clicking the left button while the right one draws is reported, as `Chord`.
    SetChord(bool),
    Key { code: u16, pressed: bool },
    QueryHeld,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Ready,
    Started(f64, f64),
    Updated(f64, f64),
    Point(f64, f64),
    Ended,
    Finished(HeldKeys),
    /// The left button was clicked while the right one was held, which then draws nothing.
    Chord,
    Held(HeldKeys),
    Refused { permanent: bool, message: String },
}

fn flag(value: bool) -> u8 {
    value as u8
}

fn parse_flag(text: &str) -> Option<bool> {
    match text {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

impl Request {
    pub fn encode(&self) -> String {
        match self {
            Request::Open {
                button,
                sensitivity,
                keyboards,
                device,
            } => format!("open {button} {sensitivity} {} {device}\n", flag(*keyboards)),
            Request::SetKeyboards(on) => format!("keyboards {}\n", flag(*on)),
            Request::SetChord(on) => format!("chord {}\n", flag(*on)),
            Request::Key { code, pressed } => format!("key {code} {}\n", flag(*pressed)),
            Request::QueryHeld => "held\n".to_string(),
        }
    }

    pub fn decode(line: &str) -> Option<Self> {
        let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
        match command {
            "open" => {
                let mut fields = rest.splitn(4, ' ');
                Some(Request::Open {
                    button: fields.next()?.parse().ok()?,
                    sensitivity: fields.next()?.parse().ok()?,
                    keyboards: parse_flag(fields.next()?)?,
                    device: fields.next()?.to_string(),
                })
            }
            "keyboards" => Some(Request::SetKeyboards(parse_flag(rest)?)),
            "chord" => Some(Request::SetChord(parse_flag(rest)?)),
            "key" => {
                let (code, pressed) = rest.split_once(' ')?;
                Some(Request::Key {
                    code: code.parse().ok()?,
                    pressed: parse_flag(pressed)?,
                })
            }
            "held" if rest.is_empty() => Some(Request::QueryHeld),
            _ => None,
        }
    }
}

fn parse_point(text: &str) -> Option<(f64, f64)> {
    let (x, y) = text.split_once(' ')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

impl Event {
    pub fn encode(&self) -> String {
        match self {
            Event::Ready => "ready\n".to_string(),
            Event::Started(x, y) => format!("started {x} {y}\n"),
            Event::Updated(x, y) => format!("updated {x} {y}\n"),
            Event::Point(x, y) => format!("point {x} {y}\n"),
            Event::Ended => "ended\n".to_string(),
            Event::Finished(held) => format!("finished {}\n", held.encode()),
            Event::Chord => "chord\n".to_string(),
            Event::Held(held) => format!("held {}\n", held.encode()),
            Event::Refused { permanent, message } => {
                format!("refused {} {}\n", flag(*permanent), message.replace('\n', " "))
            }
        }
    }

    pub fn decode(line: &str) -> Option<Self> {
        let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
        match command {
            "ready" => Some(Event::Ready),
            "started" => parse_point(rest).map(|(x, y)| Event::Started(x, y)),
            "updated" => parse_point(rest).map(|(x, y)| Event::Updated(x, y)),
            "point" => parse_point(rest).map(|(x, y)| Event::Point(x, y)),
            "ended" => Some(Event::Ended),
            "finished" => Some(Event::Finished(HeldKeys::decode(rest))),
            "chord" if rest.is_empty() => Some(Event::Chord),
            "held" => Some(Event::Held(HeldKeys::decode(rest))),
            "refused" => {
                let (permanent, message) = rest.split_once(' ')?;
                Some(Event::Refused {
                    permanent: parse_flag(permanent)?,
                    message: message.to_string(),
                })
            }
            _ => None,
        }
    }
}

/// Collects bytes into lines. A line that's too long means the other side isn't speaking this
/// protocol, which is reported instead of using ever more memory.
#[derive(Default)]
pub struct LineBuffer {
    bytes: Vec<u8>,
}

impl LineBuffer {
    pub fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn next_line(&mut self) -> Option<String> {
        let end = self.bytes.iter().position(|byte| *byte == b'\n')?;
        let line = String::from_utf8_lossy(&self.bytes[..end]).into_owned();
        self.bytes.drain(..=end);
        Some(line)
    }

    pub fn overflowed(&self) -> bool {
        self.bytes.len() > MAX_LINE_LENGTH
    }
}

/// Who is using the seat, from the contents of `/run/systemd/seats/seat0`.
pub fn active_seat_uid(seat_file: &str) -> Option<u32> {
    seat_file
        .lines()
        .find_map(|line| line.strip_prefix("ACTIVE_UID="))
        .and_then(|uid| uid.trim().parse().ok())
}

/// Whether to say which modifiers are held. Answering at any time would let any program watch the
/// rhythm of someone's typing, so it needs a gesture that was just drawn with the real mouse.
pub fn may_report_held(last_gesture: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last_gesture.is_some_and(|at| now.saturating_duration_since(at) <= HELD_REPORT_WINDOW)
}

/// The keys a client may press: the keyboard range the virtual device has, and mouse buttons.
pub fn may_press_key(code: u16) -> bool {
    (1..255).contains(&code) || (0x110..=0x11f).contains(&code)
}

/// Whether a resolved path is an input event node, so that a client can't open anything else.
pub fn is_event_node(path: &std::path::Path) -> bool {
    path.parent() == Some(std::path::Path::new("/dev/input"))
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("event"))
            .is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn requests_survive_being_sent() {
        for request in [
            Request::Open {
                button: 3,
                sensitivity: 30,
                keyboards: true,
                device: "/dev/input/by-path/a mouse-event".to_string(),
            },
            Request::SetKeyboards(false),
            Request::SetChord(true),
            Request::Key { code: 29, pressed: true },
            Request::QueryHeld,
        ] {
            let line = request.encode();
            assert_eq!(Request::decode(line.trim_end_matches('\n')), Some(request));
        }
    }

    #[test]
    fn events_survive_being_sent() {
        let held = HeldKeys::from_key_codes([29, 125]);
        for event in [
            Event::Ready,
            Event::Started(1.5, -2.0),
            Event::Updated(3.0, 4.25),
            Event::Point(0.0, 100.0),
            Event::Ended,
            Event::Finished(held.clone()),
            Event::Finished(HeldKeys::default()),
            Event::Chord,
            Event::Held(held),
            Event::Held(HeldKeys::default()),
            Event::Refused {
                permanent: true,
                message: "no such button".to_string(),
            },
        ] {
            let line = event.encode();
            assert_eq!(Event::decode(line.trim_end_matches('\n')), Some(event));
        }
    }

    #[test]
    fn nonsense_is_not_understood() {
        for line in ["", "open", "open x 30 1 /dev/input/event1", "key 29", "key 99999 1", "key 1 2", "held now", "chord 2", "rm -rf"] {
            assert_eq!(Request::decode(line), None, "{line}");
        }
        for line in ["", "started 1", "point a b", "refused", "chord now"] {
            assert_eq!(Event::decode(line), None, "{line}");
        }
    }

    #[test]
    fn a_refusal_stays_on_one_line() {
        let event = Event::Refused {
            permanent: false,
            message: "two\nlines".to_string(),
        };
        assert_eq!(event.encode().matches('\n').count(), 1);
    }

    #[test]
    fn lines_can_arrive_in_pieces() {
        let mut buffer = LineBuffer::default();
        buffer.push(b"rea");
        assert_eq!(buffer.next_line(), None);
        buffer.push(b"dy\nended\nhe");
        assert_eq!(buffer.next_line().as_deref(), Some("ready"));
        assert_eq!(buffer.next_line().as_deref(), Some("ended"));
        assert_eq!(buffer.next_line(), None);
        assert!(!buffer.overflowed());
        buffer.push(&vec![b'x'; MAX_LINE_LENGTH + 1]);
        assert!(buffer.overflowed());
    }

    #[test]
    fn the_active_user_is_read_from_the_seat_file() {
        assert_eq!(active_seat_uid("IS_SEAT0=1\nACTIVE=c2\nACTIVE_UID=1000\nCAN_TTY=1\n"), Some(1000));
        assert_eq!(active_seat_uid("IS_SEAT0=1\nACTIVE_UID=\n"), None);
        assert_eq!(active_seat_uid(""), None);
    }

    #[test]
    fn held_modifiers_are_only_reported_just_after_a_gesture() {
        let gesture = std::time::Instant::now();
        assert!(!may_report_held(None, gesture));
        assert!(may_report_held(Some(gesture), gesture));
        assert!(may_report_held(Some(gesture), gesture + HELD_REPORT_WINDOW));
        let later = gesture + HELD_REPORT_WINDOW + std::time::Duration::from_millis(1);
        assert!(!may_report_held(Some(gesture), later));
    }

    #[test]
    fn only_keyboard_keys_and_mouse_buttons_can_be_pressed() {
        assert!(may_press_key(29));
        assert!(may_press_key(0x110));
        assert!(!may_press_key(0));
        assert!(!may_press_key(255));
        assert!(!may_press_key(0x140));
    }

    #[test]
    fn only_event_nodes_can_be_opened() {
        assert!(is_event_node(Path::new("/dev/input/event12")));
        for path in ["/dev/input/event", "/dev/input/mice", "/dev/input/event1x", "/dev/sda", "/dev/input/by-path/event1", "/etc/shadow"] {
            assert!(!is_event_node(Path::new(path)), "{path}");
        }
    }
}
