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

/// The keys a layout may make modifiers, by evdev code: Ctrl, Alt, Shift and Super on both sides,
/// Caps Lock and Menu, which layouts can turn into Ctrl or Super. These are the only keys whose
/// state the input service reports.
pub const MODIFIER_KEYS: [u16; 10] = [29, 97, 56, 100, 42, 54, 125, 126, 58, 127];

/// Which of the keys that can be modifiers are down. What each means depends on the layout:
/// with Ctrl and Win swapped, the Win key is Ctrl.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HeldKeys(Vec<u16>);

impl HeldKeys {
    /// From the evdev codes of the keys that are down. Other keys are left out.
    pub fn from_key_codes(codes: impl IntoIterator<Item = u16>) -> Self {
        let mut keys: Vec<u16> =
            codes.into_iter().filter(|code| MODIFIER_KEYS.contains(code)).collect();
        keys.sort_unstable();
        keys.dedup();
        HeldKeys(keys)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn codes(&self) -> &[u16] {
        &self.0
    }

    /// What they mean where nothing is remapped.
    pub fn on_standard_layout(&self) -> Modifiers {
        Modifiers::from_key_codes(self.0.iter().copied())
    }

    /// The codes, then what they mean on a standard layout, joined by `+` like `29+125+ctrl+super`,
    /// or `none`. A daemon from before an update reads only the names, and skips the codes as
    /// names it doesn't know, so it keeps working until it's restarted.
    pub fn encode(&self) -> String {
        if self.0.is_empty() {
            return "none".to_string();
        }
        let mut parts: Vec<String> = self.0.iter().map(u16::to_string).collect();
        let standard = self.on_standard_layout();
        if !standard.is_empty() {
            parts.push(standard.to_string());
        }
        parts.join("+")
    }

    /// Reads `encode`'s text. A service from before an update only names modifiers, like
    /// `ctrl+super`, which are taken as the keys that mean them on a standard layout.
    pub fn decode(text: &str) -> Self {
        let codes: Vec<u16> = text.split('+').filter_map(|part| part.trim().parse().ok()).collect();
        if !codes.is_empty() {
            return Self::from_key_codes(codes);
        }
        let named = Modifiers::parse(text);
        let keys = [(named.ctrl, 29), (named.alt, 56), (named.shift, 42), (named.super_key, 125)];
        Self::from_key_codes(keys.into_iter().filter(|(held, _)| *held).map(|(_, code)| code))
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

    #[test]
    fn only_keys_that_can_be_modifiers_are_held_keys() {
        let held = HeldKeys::from_key_codes([30, 125, 29, 57, 125]);
        assert_eq!(held.codes(), [29, 125]);
        assert!(HeldKeys::from_key_codes([30, 57, 1]).is_empty());
    }

    #[test]
    fn held_keys_survive_being_written_and_read_back() {
        let held = HeldKeys::from_key_codes([58, 97]);
        assert_eq!(held.encode(), "58+97+ctrl");
        assert_eq!(HeldKeys::decode(&held.encode()), held);
        let caps_only = HeldKeys::from_key_codes([58]);
        assert_eq!(caps_only.encode(), "58");
        assert_eq!(HeldKeys::decode(&caps_only.encode()), caps_only);
        assert_eq!(HeldKeys::default().encode(), "none");
        assert!(HeldKeys::decode("none").is_empty());
    }

    #[test]
    fn an_older_daemon_reads_the_usual_meaning_of_held_keys() {
        // It reads them with `Modifiers::parse`, as it did before held keys were sent.
        let held = HeldKeys::from_key_codes([29, 125, 54]);
        assert_eq!(Modifiers::parse(&held.encode()), Modifiers::parse("ctrl+shift+super"));
        assert!(Modifiers::parse(&HeldKeys::default().encode()).is_empty());
    }

    #[test]
    fn modifiers_named_by_an_older_service_are_read_as_their_usual_keys() {
        let held = HeldKeys::decode("ctrl+super");
        assert_eq!(held.codes(), [29, 125]);
        assert_eq!(held.on_standard_layout(), Modifiers::parse("ctrl+super"));
    }
}
