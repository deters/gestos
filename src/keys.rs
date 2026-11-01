//! Turning shortcut text into the keys to press. This belongs to the daemon: the input service
//! only ever sees key codes.

use evdev::KeyCode;

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

/// Resolves a shortcut like `Control_L+Alt_L+t` to the keys to press on a layout. A symbol that
/// needs Shift or AltGr on it brings that key along. Anything the layout doesn't know falls back
/// to the fixed table. A shortcut with an unknown key is rejected as a whole, because pressing
/// only the known part would be a different shortcut.
#[cfg(target_os = "linux")]
pub fn parse_key_combo_on(
    keys: &str,
    layout: Option<&crate::keymap::LayoutKeys>,
) -> Result<Vec<u16>, String> {
    let mut codes = Vec::new();
    let mut add = |code: u16| {
        if !codes.contains(&code) {
            codes.push(code);
        }
    };
    for token in keys.split(['+', ' ']).filter(|token| !token.is_empty()) {
        match layout.and_then(|layout| layout.lookup(token)) {
            Some((key, shift, altgr)) => {
                if shift {
                    add(KeyCode::KEY_LEFTSHIFT.0);
                }
                if altgr {
                    add(KeyCode::KEY_RIGHTALT.0);
                }
                add(key);
            }
            None => add(name_to_keycode(token).ok_or_else(|| token.to_string())?),
        }
    }
    Ok(codes)
}

/// The same, using physical positions on a US layout.
#[cfg(target_os = "linux")]
pub fn parse_key_combo(keys: &str) -> Result<Vec<u16>, String> {
    parse_key_combo_on(keys, None)
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
    fn shortcuts_follow_the_layout_and_add_what_a_symbol_needs() {
        use crate::keymap::{LayoutKeys, LayoutName};
        let compile = |layout: &str| {
            LayoutKeys::compile(&LayoutName {
                layout: layout.to_string(),
                ..Default::default()
            })
        };
        let Some(french) = compile("fr") else { return };
        assert_eq!(
            parse_key_combo_on("Control+Z", Some(&french)),
            Ok(vec![KeyCode::KEY_LEFTCTRL.0, KeyCode::KEY_W.0])
        );
        let Some(us) = compile("us") else { return };
        // Shift is only pressed once, however many ask for it.
        assert_eq!(
            parse_key_combo_on("Shift+exclam", Some(&us)),
            Ok(vec![KeyCode::KEY_LEFTSHIFT.0, KeyCode::KEY_1.0])
        );
        // The table still covers names the layout doesn't use.
        assert_eq!(
            parse_key_combo_on("escape", Some(&us)),
            Ok(vec![KeyCode::KEY_ESC.0])
        );
        assert_eq!(
            parse_key_combo_on("Control+NoSuchKey", Some(&us)),
            Err("NoSuchKey".to_string())
        );
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

}
