//! Finding out which modifiers are held, for gestures that need them. Keyboards are only opened
//! if a gesture uses a modifier, and their key events are never read: the kernel is only asked
//! which keys that can be modifiers are down at the moment the gesture button is pressed.

use crate::modifiers::HeldKeys;

const KEY_LEFTCTRL: usize = 29;
const KEY_A: usize = 30;
const INOTIFY_HEADER_SIZE: usize = 16;

/// Whether a device's `capabilities/key` text, hex words with the lowest last, has all these keys.
fn has_keys(capabilities: &str, keys: &[usize]) -> bool {
    let words: Vec<u64> = capabilities
        .split_whitespace()
        .rev()
        .filter_map(|word| u64::from_str_radix(word, 16).ok())
        .collect();
    keys.iter()
        .all(|key| words.get(key / 64).is_some_and(|word| word & (1 << (key % 64)) != 0))
}

/// Whether an input node like `event5` is a keyboard on real hardware. Virtual devices don't
/// count, which excludes the one this program creates, whose key range looks like a keyboard.
pub fn is_physical_keyboard(node: &str) -> bool {
    let device = std::path::PathBuf::from("/sys/class/input").join(node).join("device");
    let is_virtual = std::fs::canonicalize(&device)
        .map(|path| path.to_string_lossy().contains("/virtual/"))
        .unwrap_or(true);
    !is_virtual
        && std::fs::read_to_string(device.join("capabilities/key"))
            .is_ok_and(|capabilities| has_keys(&capabilities, &[KEY_LEFTCTRL, KEY_A]))
}

/// The `event*` names in a buffer of inotify events.
pub fn event_nodes(buffer: &[u8]) -> Vec<String> {
    let mut nodes = Vec::new();
    let mut offset = 0;
    while offset + INOTIFY_HEADER_SIZE <= buffer.len() {
        let length_bytes = &buffer[offset + 12..offset + INOTIFY_HEADER_SIZE];
        let length = u32::from_ne_bytes(length_bytes.try_into().unwrap()) as usize;
        let start = offset + INOTIFY_HEADER_SIZE;
        let Some(name) = buffer.get(start..start + length) else {
            break;
        };
        let name = String::from_utf8_lossy(name);
        let name = name.trim_end_matches('\0');
        if name.starts_with("event") {
            nodes.push(name.to_string());
        }
        offset = start + length;
    }
    nodes
}

#[cfg(target_os = "linux")]
pub struct Keyboards {
    devices: Vec<evdev::Device>,
}

#[cfg(target_os = "linux")]
impl Keyboards {
    pub fn open() -> Self {
        let mut devices = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/dev/input") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("event") && is_physical_keyboard(&name) {
                    if let Ok(device) = evdev::Device::open(entry.path()) {
                        devices.push(device);
                    }
                }
            }
        }
        Keyboards { devices }
    }

    pub fn count(&self) -> usize {
        self.devices.len()
    }

    pub fn held(&self) -> HeldKeys {
        HeldKeys::from_key_codes(
            self.devices
                .iter()
                .filter_map(|device| device.get_key_state().ok())
                .flat_map(|keys| keys.iter().map(|key| key.0).collect::<Vec<_>>()),
        )
    }
}

/// Apple's vendor IDs: for its keyboards over USB, and over Bluetooth.
const APPLE_VENDORS: [&str; 2] = ["05ac", "004c"];

/// Whether a list of input devices, as `/proc/bus/input/devices` has it, has a keyboard from Apple.
fn has_apple_keyboard(devices: &str) -> bool {
    devices.split("\n\n").any(|device| {
        let from_apple = device.lines().any(|line| {
            line.starts_with("I:")
                && APPLE_VENDORS.iter().any(|vendor| line.contains(&format!("Vendor={vendor}")))
        });
        let keyboard = device
            .lines()
            .any(|line| line.starts_with("H:") && line.split_whitespace().any(|word| word == "kbd"));
        from_apple && keyboard
    })
}

/// Whether one of the driver for Apple keyboards' options is on, as `1` or `Y`.
fn apple_option(name: &str) -> bool {
    std::fs::read_to_string(format!("/sys/module/hid_apple/parameters/{name}"))
        .is_ok_and(|value| matches!(value.trim(), "1" | "Y"))
}

/// What the modifier keys are called on the keyboards connected: a Mac's names if one is from
/// Apple, as the person using it reads them on its keys. Found once, when first asked.
pub fn modifier_names() -> crate::modifiers::ModifierNames {
    use crate::modifiers::ModifierNames;
    static NAMES: std::sync::OnceLock<ModifierNames> = std::sync::OnceLock::new();
    *NAMES.get_or_init(|| {
        let devices = std::fs::read_to_string("/proc/bus/input/devices").unwrap_or_default();
        if has_apple_keyboard(&devices) {
            ModifierNames::mac(apple_option("swap_opt_cmd"), apple_option("swap_ctrl_cmd"))
        } else {
            ModifierNames::PC
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES: &str = "I: Bus=0003 Vendor=046d Product=c52b Version=0111
N: Name=\"Logitech USB Receiver\"
H: Handlers=sysrq kbd event4

I: Bus=0003 Vendor=05ac Product=024f Version=0111
N: Name=\"Apple Inc. Magic Keyboard\"
H: Handlers=sysrq kbd leds event7

I: Bus=0005 Vendor=004c Product=0269 Version=0001
N: Name=\"Magic Mouse\"
H: Handlers=mouse2 event9
";

    #[test]
    fn a_keyboard_from_apple_is_found_and_its_mouse_is_not_a_keyboard() {
        assert!(has_apple_keyboard(DEVICES));
        let without = DEVICES.replace("Vendor=05ac", "Vendor=1234");
        assert!(!has_apple_keyboard(&without));
    }

    #[test]
    fn a_mac_keyboard_names_its_keys_as_they_are_set() {
        use crate::modifiers::ModifierNames;
        let names = ModifierNames::mac(false, false);
        assert_eq!((names.super_key, names.alt), ("\u{2318} Command", "\u{2325} Option"));
        let swapped = ModifierNames::mac(true, false);
        assert_eq!((swapped.super_key, swapped.alt), ("\u{2325} Option", "\u{2318} Command"));
        let swapped = ModifierNames::mac(false, true);
        assert_eq!((swapped.super_key, swapped.ctrl), ("\u{2303} Control", "\u{2318} Command"));
    }

    #[test]
    fn capabilities_are_read_from_the_lowest_word_last() {
        // Bits 29 and 30 are in the lowest word, which comes last.
        assert!(has_keys("ff 60000000", &[KEY_LEFTCTRL, KEY_A]));
        assert!(!has_keys("60000000 0", &[KEY_LEFTCTRL, KEY_A]));
        assert!(!has_keys("20000000", &[KEY_LEFTCTRL, KEY_A]));
        assert!(!has_keys("", &[KEY_A]));
    }

    fn inotify_event(name: &str, padded_length: usize) -> Vec<u8> {
        let mut event = vec![0; 12];
        event.extend((padded_length as u32).to_ne_bytes());
        let mut padded = name.as_bytes().to_vec();
        padded.resize(padded_length, 0);
        event.extend(padded);
        event
    }

    #[test]
    fn only_event_nodes_are_taken_from_inotify_events() {
        let mut buffer = inotify_event("event7", 16);
        buffer.extend(inotify_event("js0", 16));
        buffer.extend(inotify_event("event12", 16));
        assert_eq!(event_nodes(&buffer), ["event7", "event12"]);
    }

    #[test]
    fn a_truncated_inotify_buffer_is_not_a_problem() {
        let buffer = inotify_event("event7", 16);
        assert!(event_nodes(&buffer[..20]).is_empty());
        assert!(event_nodes(&[]).is_empty());
    }
}
