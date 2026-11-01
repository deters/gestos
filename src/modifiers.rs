use std::fmt;

/// The modifier keys a gesture needs held while it's drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl Modifiers {
    pub fn is_empty(&self) -> bool {
        *self == Modifiers::default()
    }

    /// Reads a config value like `ctrl+alt`. Anything else in it is ignored.
    pub fn parse(text: &str) -> Self {
        let mut modifiers = Modifiers::default();
        for token in text.split('+') {
            match token.trim().to_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "alt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "super" | "meta" => modifiers.super_key = true,
                _ => {}
            }
        }
        modifiers
    }

    /// From the evdev codes of the keys that are down.
    pub fn from_key_codes(codes: impl IntoIterator<Item = u16>) -> Self {
        let mut modifiers = Modifiers::default();
        for code in codes {
            match code {
                29 | 97 => modifiers.ctrl = true,
                56 | 100 => modifiers.alt = true,
                42 | 54 => modifiers.shift = true,
                125 | 126 => modifiers.super_key = true,
                _ => {}
            }
        }
        modifiers
    }

    /// Names for showing to the user, in a fixed order.
    pub fn labels(&self) -> Vec<&'static str> {
        [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.super_key, "Super"),
        ]
        .into_iter()
        .filter(|(held, _)| *held)
        .map(|(_, label)| label)
        .collect()
    }
}

impl fmt::Display for Modifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.labels().join("+").to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_survive_being_written_and_read_back() {
        let modifiers = Modifiers {
            ctrl: true,
            alt: false,
            shift: true,
            super_key: true,
        };
        assert_eq!(modifiers.to_string(), "ctrl+shift+super");
        assert_eq!(Modifiers::parse(&modifiers.to_string()), modifiers);
    }

    #[test]
    fn parsing_ignores_case_spaces_and_unknown_names() {
        let parsed = Modifiers::parse(" Control + nonsense + ALT ");
        assert!(parsed.ctrl && parsed.alt && !parsed.shift && !parsed.super_key);
        assert!(Modifiers::parse("").is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_codes_are_the_real_modifier_keys() {
        use evdev::KeyCode;
        for key in [KeyCode::KEY_LEFTCTRL, KeyCode::KEY_RIGHTCTRL] {
            assert!(Modifiers::from_key_codes([key.0]).ctrl);
        }
        for key in [KeyCode::KEY_LEFTALT, KeyCode::KEY_RIGHTALT] {
            assert!(Modifiers::from_key_codes([key.0]).alt);
        }
        for key in [KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_RIGHTSHIFT] {
            assert!(Modifiers::from_key_codes([key.0]).shift);
        }
        for key in [KeyCode::KEY_LEFTMETA, KeyCode::KEY_RIGHTMETA] {
            assert!(Modifiers::from_key_codes([key.0]).super_key);
        }
    }

    #[test]
    fn both_sides_of_the_keyboard_count_and_other_keys_do_not() {
        assert!(Modifiers::from_key_codes([29]).ctrl);
        assert!(Modifiers::from_key_codes([97]).ctrl);
        assert!(Modifiers::from_key_codes([100]).alt);
        assert!(Modifiers::from_key_codes([54]).shift);
        assert!(Modifiers::from_key_codes([126]).super_key);
        assert!(Modifiers::from_key_codes([30, 57, 1]).is_empty());
    }
}
