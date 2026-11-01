#[cfg(target_os = "linux")]
use evdev::uinput::VirtualDevice;
#[cfg(target_os = "linux")]
use evdev::KeyCode;
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

#[cfg(target_os = "linux")]
pub fn name_to_keycode(name: &str) -> Option<u16> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "a" => Some(KeyCode::KEY_A.0),
        "b" => Some(KeyCode::KEY_B.0),
        "c" => Some(KeyCode::KEY_C.0),
        "d" => Some(KeyCode::KEY_D.0),
        "e" => Some(KeyCode::KEY_E.0),
        "f" => Some(KeyCode::KEY_F.0),
        "g" => Some(KeyCode::KEY_G.0),
        "h" => Some(KeyCode::KEY_H.0),
        "i" => Some(KeyCode::KEY_I.0),
        "j" => Some(KeyCode::KEY_J.0),
        "k" => Some(KeyCode::KEY_K.0),
        "l" => Some(KeyCode::KEY_L.0),
        "m" => Some(KeyCode::KEY_M.0),
        "n" => Some(KeyCode::KEY_N.0),
        "o" => Some(KeyCode::KEY_O.0),
        "p" => Some(KeyCode::KEY_P.0),
        "q" => Some(KeyCode::KEY_Q.0),
        "r" => Some(KeyCode::KEY_R.0),
        "s" => Some(KeyCode::KEY_S.0),
        "t" => Some(KeyCode::KEY_T.0),
        "u" => Some(KeyCode::KEY_U.0),
        "v" => Some(KeyCode::KEY_V.0),
        "w" => Some(KeyCode::KEY_W.0),
        "x" => Some(KeyCode::KEY_X.0),
        "y" => Some(KeyCode::KEY_Y.0),
        "z" => Some(KeyCode::KEY_Z.0),
        "0" => Some(KeyCode::KEY_0.0),
        "1" => Some(KeyCode::KEY_1.0),
        "2" => Some(KeyCode::KEY_2.0),
        "3" => Some(KeyCode::KEY_3.0),
        "4" => Some(KeyCode::KEY_4.0),
        "5" => Some(KeyCode::KEY_5.0),
        "6" => Some(KeyCode::KEY_6.0),
        "7" => Some(KeyCode::KEY_7.0),
        "8" => Some(KeyCode::KEY_8.0),
        "9" => Some(KeyCode::KEY_9.0),
        "return" | "enter" => Some(KeyCode::KEY_ENTER.0),
        "escape" | "esc" => Some(KeyCode::KEY_ESC.0),
        "backspace" => Some(KeyCode::KEY_BACKSPACE.0),
        "tab" => Some(KeyCode::KEY_TAB.0),
        "space" => Some(KeyCode::KEY_SPACE.0),
        "delete" => Some(KeyCode::KEY_DELETE.0),
        "home" => Some(KeyCode::KEY_HOME.0),
        "end" => Some(KeyCode::KEY_END.0),
        "left" => Some(KeyCode::KEY_LEFT.0),
        "up" => Some(KeyCode::KEY_UP.0),
        "right" => Some(KeyCode::KEY_RIGHT.0),
        "down" => Some(KeyCode::KEY_DOWN.0),
        "page_up" => Some(KeyCode::KEY_PAGEUP.0),
        "page_down" => Some(KeyCode::KEY_PAGEDOWN.0),
        "control_l" | "ctrl" | "control" => Some(KeyCode::KEY_LEFTCTRL.0),
        "control_r" => Some(KeyCode::KEY_RIGHTCTRL.0),
        "shift_l" | "shift" => Some(KeyCode::KEY_LEFTSHIFT.0),
        "shift_r" => Some(KeyCode::KEY_RIGHTSHIFT.0),
        "alt_l" | "alt" => Some(KeyCode::KEY_LEFTALT.0),
        "alt_r" => Some(KeyCode::KEY_RIGHTALT.0),
        "super_l" | "super" | "win" | "meta" => Some(KeyCode::KEY_LEFTMETA.0),
        "super_r" => Some(KeyCode::KEY_RIGHTMETA.0),
        "f1" => Some(KeyCode::KEY_F1.0),
        "f2" => Some(KeyCode::KEY_F2.0),
        "f3" => Some(KeyCode::KEY_F3.0),
        "f4" => Some(KeyCode::KEY_F4.0),
        "f5" => Some(KeyCode::KEY_F5.0),
        "f6" => Some(KeyCode::KEY_F6.0),
        "f7" => Some(KeyCode::KEY_F7.0),
        "f8" => Some(KeyCode::KEY_F8.0),
        "f9" => Some(KeyCode::KEY_F9.0),
        "f10" => Some(KeyCode::KEY_F10.0),
        "f11" => Some(KeyCode::KEY_F11.0),
        "f12" => Some(KeyCode::KEY_F12.0),
        "mute" | "audiomute" | "xf86audiomute" => Some(KeyCode::KEY_MUTE.0),
        "volumedown" | "audiolowervolume" | "xf86audiolowervolume" => Some(KeyCode::KEY_VOLUMEDOWN.0),
        "volumeup" | "audioraisevolume" | "xf86audioraisevolume" => Some(KeyCode::KEY_VOLUMEUP.0),
        "playpause" | "audioplaypause" | "xf86audioplay" => Some(KeyCode::KEY_PLAYPAUSE.0),
        "play" | "audioplay" => Some(KeyCode::KEY_PLAY.0),
        "pause" | "audiopause" => Some(KeyCode::KEY_PAUSE.0),
        "stop" | "audiostop" | "xf86audiostop" => Some(KeyCode::KEY_STOPCD.0),
        "next" | "audionext" | "xf86audionext" => Some(KeyCode::KEY_NEXTSONG.0),
        "prev" | "audioprev" | "xf86audioprev" => Some(KeyCode::KEY_PREVIOUSSONG.0),
        "www" | "xf86www" => Some(KeyCode::KEY_WWW.0),
        "explorer" | "xf86explorer" => Some(KeyCode::KEY_FILE.0),
        "homepage" | "xf86homepage" => Some(KeyCode::KEY_HOMEPAGE.0),
        "mail" | "xf86mail" => Some(KeyCode::KEY_MAIL.0),
        "search" | "xf86search" => Some(KeyCode::KEY_SEARCH.0),
        "calc" | "xf86calculator" => Some(KeyCode::KEY_CALC.0),
        "print" | "printscreen" => Some(KeyCode::KEY_SYSRQ.0),
        "insert" => Some(KeyCode::KEY_INSERT.0),
        "prior" => Some(KeyCode::KEY_PAGEUP.0),
        "minus" | "underscore" => Some(KeyCode::KEY_MINUS.0),
        "equal" | "plus" => Some(KeyCode::KEY_EQUAL.0),
        "bracketleft" | "braceleft" => Some(KeyCode::KEY_LEFTBRACE.0),
        "bracketright" | "braceright" => Some(KeyCode::KEY_RIGHTBRACE.0),
        "semicolon" | "colon" => Some(KeyCode::KEY_SEMICOLON.0),
        "apostrophe" | "quotedbl" => Some(KeyCode::KEY_APOSTROPHE.0),
        "grave" | "asciitilde" => Some(KeyCode::KEY_GRAVE.0),
        "backslash" | "bar" => Some(KeyCode::KEY_BACKSLASH.0),
        "comma" | "less" => Some(KeyCode::KEY_COMMA.0),
        "period" | "greater" => Some(KeyCode::KEY_DOT.0),
        "slash" | "question" => Some(KeyCode::KEY_SLASH.0),
        "exclam" => Some(KeyCode::KEY_1.0),
        "at" => Some(KeyCode::KEY_2.0),
        "numbersign" => Some(KeyCode::KEY_3.0),
        "dollar" => Some(KeyCode::KEY_4.0),
        "percent" => Some(KeyCode::KEY_5.0),
        "asciicircum" => Some(KeyCode::KEY_6.0),
        "ampersand" => Some(KeyCode::KEY_7.0),
        "asterisk" => Some(KeyCode::KEY_8.0),
        "parenleft" => Some(KeyCode::KEY_9.0),
        "parenright" => Some(KeyCode::KEY_0.0),
        _ => None,
    }
}

/// Resolves a shortcut like `Control_L+Alt_L+t` to its key codes. A shortcut with an unknown
/// key is rejected as a whole, because pressing only the known part would be a different
/// shortcut. Keys are physical positions on a US layout, not the characters a layout produces.
#[cfg(target_os = "linux")]
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

    /// Presses and releases a single key.
    pub fn tap_key(&self, code: u16) {
        self.emit(&[evdev::InputEvent::new(evdev::EventType::KEY.0, code, 1)]);
        self.emit(&[evdev::InputEvent::new(evdev::EventType::KEY.0, code, 0)]);
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
    fn the_file_manager_and_homepage_keys_are_different() {
        assert_eq!(parse_key_combo("XF86Explorer"), Ok(vec![144]));
        assert_eq!(parse_key_combo("XF86HomePage"), Ok(vec![172]));
    }

    #[test]
    fn key_combo_with_an_unknown_key_is_rejected_whole() {
        assert_eq!(
            parse_key_combo("Control_L+NoSuchKey"),
            Err("NoSuchKey".to_string())
        );
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
    fn trigger_codes_are_the_real_button_codes() {
        assert_eq!(trigger_button_code(1), Some(KeyCode::BTN_LEFT.0));
        assert_eq!(trigger_button_code(2), Some(KeyCode::BTN_MIDDLE.0));
        assert_eq!(trigger_button_code(3), Some(KeyCode::BTN_RIGHT.0));
        assert_eq!(trigger_button_code(8), Some(KeyCode::BTN_SIDE.0));
        assert_eq!(trigger_button_code(9), Some(KeyCode::BTN_EXTRA.0));
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
