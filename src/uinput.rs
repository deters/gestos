#[cfg(target_os = "linux")]
use evdev::uinput::VirtualDevice;
#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};

/// Maps a trigger button argument to its evdev code. Only mouse and digitizer buttons are
/// accepted: this binary may run with `input` group rights, so it must not act as a way to
/// observe keyboard keys.
pub fn trigger_button_code(button: i32) -> Option<u16> {
    match button {
        1 => Some(0x110), // BTN_LEFT
        2 => Some(0x112), // BTN_MIDDLE
        3 => Some(0x111), // BTN_RIGHT
        8 => Some(0x113), // BTN_SIDE
        9 => Some(0x114), // BTN_EXTRA
        0x110..=0x11f | 0x140..=0x14f => Some(button as u16),
        _ => None,
    }
}

pub fn name_to_keycode(name: &str) -> Option<u16> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "a" => Some(30),
        "b" => Some(48),
        "c" => Some(46),
        "d" => Some(32),
        "e" => Some(18),
        "f" => Some(33),
        "g" => Some(34),
        "h" => Some(35),
        "i" => Some(23),
        "j" => Some(36),
        "k" => Some(37),
        "l" => Some(38),
        "m" => Some(50),
        "n" => Some(49),
        "o" => Some(24),
        "p" => Some(25),
        "q" => Some(16),
        "r" => Some(19),
        "s" => Some(31),
        "t" => Some(20),
        "u" => Some(22),
        "v" => Some(47),
        "w" => Some(17),
        "x" => Some(45),
        "y" => Some(21),
        "z" => Some(44),
        "0" => Some(11),
        "1" => Some(2),
        "2" => Some(3),
        "3" => Some(4),
        "4" => Some(5),
        "5" => Some(6),
        "6" => Some(7),
        "7" => Some(8),
        "8" => Some(9),
        "9" => Some(10),
        "return" | "enter" => Some(28),
        "escape" | "esc" => Some(1),
        "backspace" => Some(14),
        "tab" => Some(15),
        "space" => Some(57),
        "delete" => Some(111),
        "home" => Some(102),
        "end" => Some(107),
        "left" => Some(105),
        "up" => Some(103),
        "right" => Some(106),
        "down" => Some(108),
        "page_up" => Some(104),
        "page_down" => Some(109),
        "control_l" | "ctrl" | "control" => Some(29),
        "control_r" => Some(97),
        "shift_l" | "shift" => Some(42),
        "shift_r" => Some(54),
        "alt_l" | "alt" => Some(56),
        "alt_r" => Some(100),
        "super_l" | "super" | "win" | "meta" => Some(125),
        "super_r" => Some(126),
        "f1" => Some(59),
        "f2" => Some(60),
        "f3" => Some(61),
        "f4" => Some(62),
        "f5" => Some(63),
        "f6" => Some(64),
        "f7" => Some(65),
        "f8" => Some(66),
        "f9" => Some(67),
        "f10" => Some(68),
        "f11" => Some(87),
        "f12" => Some(88),
        "mute" | "audiomute" | "xf86audiomute" => Some(113),
        "volumedown" | "audiolowervolume" | "xf86audiolowervolume" => Some(114),
        "volumeup" | "audioraisevolume" | "xf86audioraisevolume" => Some(115),
        "playpause" | "audioplaypause" | "xf86audioplay" => Some(164),
        "play" | "audioplay" => Some(207),
        "pause" | "audiopause" => Some(119),
        "stop" | "audiostop" | "xf86audiostop" => Some(166),
        "next" | "audionext" | "xf86audionext" => Some(163),
        "prev" | "audioprev" | "xf86audioprev" => Some(165),
        "www" | "xf86www" => Some(150),
        "explorer" | "xf86explorer" | "homepage" => Some(172),
        "mail" | "xf86mail" => Some(155),
        "search" | "xf86search" => Some(217),
        "calc" | "xf86calculator" => Some(140),
        "print" | "printscreen" => Some(99),
        "insert" => Some(110),
        "prior" => Some(104),
        "minus" | "underscore" => Some(12),
        "equal" | "plus" => Some(13),
        "bracketleft" | "braceleft" => Some(26),
        "bracketright" | "braceright" => Some(27),
        "semicolon" | "colon" => Some(39),
        "apostrophe" | "quotedbl" => Some(40),
        "grave" | "asciitilde" => Some(41),
        "backslash" | "bar" => Some(43),
        "comma" | "less" => Some(51),
        "period" | "greater" => Some(52),
        "slash" | "question" => Some(53),
        "exclam" => Some(2),
        "at" => Some(3),
        "numbersign" => Some(4),
        "dollar" => Some(5),
        "percent" => Some(6),
        "asciicircum" => Some(7),
        "ampersand" => Some(8),
        "asterisk" => Some(9),
        "parenleft" => Some(10),
        "parenright" => Some(11),
        _ => None,
    }
}

/// Resolves a shortcut like `Control_L+Alt_L+t` to its key codes. A shortcut with an unknown
/// key is rejected as a whole, because pressing only the known part would be a different
/// shortcut. Keys are physical positions on a US layout, not the characters a layout produces.
pub fn parse_key_combo(keys: &str) -> Result<Vec<u16>, String> {
    keys.split(['+', ' '])
        .filter(|token| !token.is_empty())
        .map(|token| name_to_keycode(token).ok_or_else(|| token.to_string()))
        .collect()
}

/// Shared handle to the virtual device. The lock is held only for each write, so an action
/// that is sleeping between key events never delays pointer forwarding.
#[cfg(target_os = "linux")]
#[derive(Clone)]
pub struct UinputDevice {
    device: Arc<Mutex<VirtualDevice>>,
}

#[cfg(target_os = "linux")]
impl UinputDevice {
    pub fn init_from_device(source_dev: &evdev::Device) -> Result<Self, std::io::Error> {
        let mut keys = evdev::AttributeSet::<evdev::KeyCode>::new();
        // Enable standard keyboard keys so virtual device can type shortcuts
        for code in 1..255 {
            if (0x110..=0x11f).contains(&code) {
                continue; // Skip mouse button range to preserve source bindings
            }
            keys.insert(evdev::KeyCode(code));
        }

        let mut builder = VirtualDevice::builder()?
            .name(source_dev.name().unwrap_or("Virtual Gestos Forwarder"));

        // Add keyboard keys
        builder = builder.with_keys(&keys)?;

        // Copy source device mouse buttons
        if let Some(src_keys) = source_dev.supported_keys() {
            let mut mouse_keys = evdev::AttributeSet::<evdev::KeyCode>::new();
            for k in src_keys.iter() {
                if k.0 >= 0x110 && k.0 <= 0x11f {
                    mouse_keys.insert(k);
                }
            }
            builder = builder.with_keys(&mouse_keys)?;
        }

        // Copy relative/absolute axes
        if let Some(src_rel) = source_dev.supported_relative_axes() {
            builder = builder.with_relative_axes(src_rel)?;
        }
        if let Ok(abs_axes) = source_dev.get_absinfo() {
            for (code, info) in abs_axes {
                let abs_setup = evdev::UinputAbsSetup::new(code, info);
                builder = builder.with_absolute_axis(&abs_setup)?;
            }
        }

        let device = builder.build()?;
        Ok(UinputDevice {
            device: Arc::new(Mutex::new(device)),
        })
    }

    // `VirtualDevice::emit` terminates each batch with its own SYN_REPORT.
    fn emit(&self, events: &[evdev::InputEvent]) {
        let mut device = self.device.lock().unwrap_or_else(|e| e.into_inner());
        let _ = device.emit(events);
    }

    pub fn click(&self, button: i32) {
        let ev_button = match button {
            1 => evdev::KeyCode::BTN_LEFT,
            2 => evdev::KeyCode::BTN_MIDDLE,
            3 => evdev::KeyCode::BTN_RIGHT,
            8 => evdev::KeyCode::BTN_SIDE,
            9 => evdev::KeyCode::BTN_EXTRA,
            other => evdev::KeyCode(other as u16),
        };

        let press = evdev::InputEvent::new(evdev::EventType::KEY.0, ev_button.0, 1);
        let release = evdev::InputEvent::new(evdev::EventType::KEY.0, ev_button.0, 0);

        self.emit(&[press]);
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.emit(&[release]);
    }

    pub fn keypress_string(&self, keys: &str) {
        let codes = match parse_key_combo(keys) {
            Ok(codes) => codes,
            Err(unknown) => {
                eprintln!("gestos: Ignoring shortcut '{}': unknown key '{}'", keys, unknown);
                return;
            }
        };

        // Press all keys in sequence
        for &code in &codes {
            let press = evdev::InputEvent::new(evdev::EventType::KEY.0, code, 1);
            self.emit(&[press]);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        std::thread::sleep(std::time::Duration::from_millis(50));

        // Release in reverse order
        for &code in codes.iter().rev() {
            let release = evdev::InputEvent::new(evdev::EventType::KEY.0, code, 0);
            self.emit(&[release]);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Forwards one input frame, without its SYN_REPORT.
    pub fn forward_frame(&self, events: &[evdev::InputEvent]) {
        self.emit(events);
    }
}

#[cfg(not(target_os = "linux"))]
#[derive(Clone)]
pub struct UinputDevice;

#[cfg(not(target_os = "linux"))]
impl UinputDevice {
    pub fn click(&self, button: i32) {
        log::info!("uinput click stub: {}", button);
    }
    pub fn keypress_string(&self, keys: &str) {
        log::info!("uinput keypress string stub: {}", keys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_combo_resolves_in_order_ignoring_case() {
        assert_eq!(parse_key_combo("Control_L+Alt_L+t"), Ok(vec![29, 56, 20]));
        assert_eq!(parse_key_combo("control+C"), Ok(vec![29, 46]));
        assert_eq!(parse_key_combo("Shift+minus"), Ok(vec![42, 12]));
    }

    #[test]
    fn key_combo_with_an_unknown_key_is_rejected_whole() {
        assert_eq!(
            parse_key_combo("Control_L+NoSuchKey"),
            Err("NoSuchKey".to_string())
        );
    }

    #[test]
    fn every_keypress_in_the_shipped_config_resolves() {
        let path = std::env::temp_dir().join(format!(
            "gestos-keys-{}.yaml",
            std::process::id()
        ));
        std::fs::write(&path, include_str!("../gestos.yaml")).unwrap();
        let config = crate::config::Configuration::load_from_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        for gesture in &config.gestures {
            for action in &gesture.actions {
                if let crate::config::ActionType::Keypress(keys) = action {
                    assert!(parse_key_combo(keys).is_ok(), "{}: {}", gesture.name, keys);
                }
            }
        }
    }

    #[test]
    fn every_fallback_key_used_by_the_desktop_backends_resolves() {
        for keys in [
            "Super_L+Shift_L+Control_L+Alt_L+F12",
            "Alt_L+Print",
            "Shift_L+Print",
            "Control_L+Alt_L+Delete",
            "XF86AudioRaiseVolume",
            "XF86AudioLowerVolume",
            "XF86AudioMute",
            "XF86AudioPlay",
            "XF86AudioNext",
            "XF86AudioPrev",
            "XF86WWW",
            "XF86Explorer",
            "XF86Mail",
            "XF86Search",
            "XF86Calculator",
        ] {
            assert!(parse_key_combo(keys).is_ok(), "{}", keys);
        }
    }

    #[test]
    fn trigger_accepts_mouse_and_digitizer_buttons() {
        assert_eq!(trigger_button_code(1), Some(0x110));
        assert_eq!(trigger_button_code(3), Some(0x111));
        assert_eq!(trigger_button_code(9), Some(0x114));
        assert_eq!(trigger_button_code(0x11f), Some(0x11f));
        assert_eq!(trigger_button_code(0x14a), Some(0x14a));
    }

    #[test]
    fn trigger_rejects_keyboard_codes() {
        for code in [-1, 0, 4, 7, 30, 0x10f, 0x120, 0x13f, 0x150, 0x1_0110] {
            assert_eq!(trigger_button_code(code), None, "code {code}");
        }
    }
}
