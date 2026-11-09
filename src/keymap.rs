//! Finding which physical key produces a symbol on the user's keyboard layout. A shortcut names
//! symbols, like `Control+z`, but a virtual keyboard presses physical keys, and on a French layout
//! `z` is where a US layout has `w`.

use crate::modifiers::{HeldKeys, Modifiers, MODIFIER_KEYS};
use std::collections::HashMap;
use std::ffi::CString;
use std::ptr;
use std::sync::{Arc, Mutex};
use xkbcommon_dl::{
    xkb_context_flags, xkb_key_direction, xkb_keymap_compile_flags, xkb_keysym_flags,
    xkb_rule_names, xkb_state_component, xkbcommon_option, XKB_MOD_NAME_ALT, XKB_MOD_NAME_CTRL,
    XKB_MOD_NAME_LOGO, XKB_MOD_NAME_SHIFT,
};

// xkbcommon numbers keys 8 above the kernel.
const XKB_KEYCODE_OFFSET: u32 = 8;
// The virtual device only has keys up to here.
const MAX_KERNEL_KEY: u32 = 255;
const SHIFT_LEVEL: u32 = 1;
const ALTGR_LEVEL: u32 = 2;
const SHIFT_ALTGR_LEVEL: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LayoutName {
    pub layout: String,
    pub variant: String,
    pub options: String,
}

impl LayoutName {
    pub fn new(layout: &str, variant: &str) -> Self {
        LayoutName {
            layout: layout.to_string(),
            variant: variant.to_string(),
            options: String::new(),
        }
    }
}

/// The first keyboard layout in a GNOME value like `[('xkb', 'us+intl'), ('ibus', 'mozc')]`.
fn parse_gnome_sources(value: &str) -> Option<LayoutName> {
    value.split('(').skip(1).find_map(|entry| {
        let mut fields = entry.split('\'').skip(1).step_by(2);
        if fields.next()? != "xkb" {
            return None;
        }
        let value = fields.next()?;
        let (layout, variant) = value.split_once('+').unwrap_or((value, ""));
        Some(LayoutName::new(layout, variant))
    })
}

/// The first layout in `localectl status` output.
fn parse_localectl(output: &str) -> Option<LayoutName> {
    let field = |label: &str| {
        output
            .lines()
            .find_map(|line| line.trim().strip_prefix(label))
            .map(|value| value.split(',').next().unwrap_or("").trim().to_string())
    };
    let layout = field("X11 Layout:").filter(|layout| !layout.is_empty() && layout != "(unset)")?;
    let mut name = LayoutName::new(&layout, &field("X11 Variant:").unwrap_or_default());
    name.options = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("X11 Options:"))
        .unwrap_or("")
        .trim()
        .to_string();
    Some(name)
}

/// The first layout in KDE's `kxkbrc`.
fn parse_kxkbrc(content: &str) -> Option<LayoutName> {
    let layout = content
        .lines()
        .find_map(|line| line.strip_prefix("LayoutList="))?
        .split(',')
        .next()?
        .trim();
    let mut name = LayoutName::new(layout, "");
    name.options = content
        .lines()
        .find_map(|line| line.strip_prefix("Options="))
        .unwrap_or("")
        .trim()
        .to_string();
    (!layout.is_empty()).then_some(name)
}

/// The layout in an X server's `_XKB_RULES_NAMES`: rules, model, layouts, variants and options,
/// each ending in a zero byte.
fn parse_xkb_rules_names(value: &[u8]) -> Option<LayoutName> {
    let fields: Vec<String> = value
        .split(|byte| *byte == 0)
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect();
    let first = |index: usize| {
        fields.get(index).map(|field| field.split(',').next().unwrap_or("").trim().to_string())
    };
    let layout = first(2).filter(|layout| !layout.is_empty())?;
    let mut name = LayoutName::new(&layout, &first(3).unwrap_or_default());
    name.options = fields.get(4).cloned().unwrap_or_default();
    Some(name)
}

/// What the X server says its keyboard is, `setxkbmap`'s changes included.
fn x11_layout() -> Option<LayoutName> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;
    let atom = conn.intern_atom(true, b"_XKB_RULES_NAMES").ok()?.reply().ok()?.atom;
    let value = conn
        .get_property(false, root, atom, AtomEnum::STRING, 0, 1024)
        .ok()?
        .reply()
        .ok()?
        .value;
    parse_xkb_rules_names(&value)
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn gnome_layout() -> Option<LayoutName> {
    const INPUT_SOURCES: &str = "org.gnome.desktop.input-sources";
    // Read in this process when possible: it's looked up before every shortcut, and starting
    // gsettings for each key takes longer than pressing the shortcut.
    let setting = |key: &str| {
        if crate::wayland::gio_reads_user_settings() {
            return crate::wayland::read_gnome_setting(INPUT_SOURCES, key);
        }
        command_output("gsettings", &["get", INPUT_SOURCES, key])
    };
    // Most recently used first, which puts the layout in use now at the front.
    let mut layout = setting("mru-sources")
        .as_deref()
        .and_then(parse_gnome_sources)
        .or_else(|| setting("sources").as_deref().and_then(parse_gnome_sources))?;
    if let Some(options) = setting("xkb-options") {
        let options: Vec<&str> = options.split('\'').skip(1).step_by(2).collect();
        layout.options = options.join(",");
    }
    Some(layout)
}

/// The layout in use, as far as each desktop reveals it. Other desktops only say what's configured.
/// On X11 the server has the options in use, which may be changed without the desktop knowing.
pub fn active_layout() -> Option<LayoutName> {
    let layout = desktop_layout();
    if !crate::active_app::is_x11_session() {
        return layout;
    }
    match (layout, x11_layout()) {
        (Some(mut layout), Some(server)) => {
            layout.options = server.options;
            Some(layout)
        }
        (layout, server) => layout.or(server),
    }
}

fn desktop_layout() -> Option<LayoutName> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
    if desktop.contains("kde") {
        let kxkbrc = crate::config::config_home().join("kxkbrc");
        if let Some(layout) = std::fs::read_to_string(kxkbrc).ok().as_deref().and_then(parse_kxkbrc)
        {
            return Some(layout);
        }
    } else if !desktop.contains("sway") && !desktop.contains("hyprland") {
        if let Some(layout) = gnome_layout() {
            return Some(layout);
        }
    }
    command_output("localectl", &["status"]).as_deref().and_then(parse_localectl)
}

/// Which physical key and shift level produce each symbol on one layout, and each character.
pub struct LayoutKeys {
    symbols: HashMap<u32, (u16, u32)>,
    chars: HashMap<char, (u16, u32)>,
    /// What each key that can be a modifier does when held.
    modifier_keys: HashMap<u16, Modifiers>,
}

impl LayoutKeys {
    /// `None` if the library or the layout isn't available.
    pub fn compile(name: &LayoutName) -> Option<Self> {
        let xkb = xkbcommon_option()?;
        let layout = CString::new(name.layout.as_str()).ok()?;
        let variant = CString::new(name.variant.as_str()).ok()?;
        let options = CString::new(name.options.as_str()).ok()?;
        let names = xkb_rule_names {
            rules: ptr::null(),
            model: ptr::null(),
            layout: layout.as_ptr(),
            variant: variant.as_ptr(),
            options: options.as_ptr(),
        };

        let mut symbols: HashMap<u32, (u16, u32)> = HashMap::new();
        let mut chars: HashMap<char, (u16, u32)> = HashMap::new();
        let mut modifier_keys: HashMap<u16, Modifiers> = HashMap::new();
        // Safe because the pointers stay valid during these calls, the key's symbol array is read
        // within the bounds xkbcommon reports, and each object is released exactly once.
        unsafe {
            let context = (xkb.xkb_context_new)(xkb_context_flags::XKB_CONTEXT_NO_FLAGS);
            if context.is_null() {
                return None;
            }
            let keymap = (xkb.xkb_keymap_new_from_names)(
                context,
                &names,
                xkb_keymap_compile_flags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            );
            if keymap.is_null() {
                (xkb.xkb_context_unref)(context);
                return None;
            }
            for keycode in XKB_KEYCODE_OFFSET..=(MAX_KERNEL_KEY + XKB_KEYCODE_OFFSET) {
                let levels = (xkb.xkb_keymap_num_levels_for_key)(keymap, keycode, 0);
                for level in 0..levels.min(SHIFT_ALTGR_LEVEL + 1) {
                    let mut syms: *const u32 = ptr::null();
                    let count = (xkb.xkb_keymap_key_get_syms_by_level)(
                        keymap,
                        keycode,
                        0,
                        level,
                        &mut syms as *mut *const u32 as *mut _,
                    );
                    if count <= 0 || syms.is_null() {
                        continue;
                    }
                    for symbol in std::slice::from_raw_parts(syms, count as usize) {
                        let key = (keycode - XKB_KEYCODE_OFFSET) as u16;
                        // The easiest way to type it wins.
                        let keep = |entry: &mut (u16, u32)| {
                            if level < entry.1 {
                                *entry = (key, level);
                            }
                        };
                        keep(symbols.entry(*symbol).or_insert((key, level)));
                        if let Some(c) = char::from_u32((xkb.xkb_keysym_to_utf32)(*symbol))
                            .filter(|c| !c.is_control())
                        {
                            keep(chars.entry(c).or_insert((key, level)));
                        }
                    }
                }
            }
            for key in MODIFIER_KEYS {
                let state = (xkb.xkb_state_new)(keymap);
                if state.is_null() {
                    continue;
                }
                let keycode = key as u32 + XKB_KEYCODE_OFFSET;
                (xkb.xkb_state_update_key)(state, keycode, xkb_key_direction::XKB_KEY_DOWN);
                let active = |name: &[u8]| {
                    (xkb.xkb_state_mod_name_is_active)(
                        state,
                        name.as_ptr() as *const _,
                        xkb_state_component::XKB_STATE_MODS_EFFECTIVE,
                    ) > 0
                };
                let modifiers = Modifiers {
                    ctrl: active(XKB_MOD_NAME_CTRL),
                    alt: active(XKB_MOD_NAME_ALT),
                    shift: active(XKB_MOD_NAME_SHIFT),
                    super_key: active(XKB_MOD_NAME_LOGO),
                };
                (xkb.xkb_state_unref)(state);
                modifier_keys.insert(key, modifiers);
            }
            (xkb.xkb_keymap_unref)(keymap);
            (xkb.xkb_context_unref)(context);
        }
        Some(LayoutKeys { symbols, chars, modifier_keys })
    }

    /// What held keys mean on this layout. A key that's no modifier on it keeps its usual meaning,
    /// so right Alt is Alt where it types the third level, as it always was here.
    pub fn modifiers(&self, held: &HeldKeys) -> Modifiers {
        let mut modifiers = Modifiers::default();
        for key in held.codes() {
            let meaning = self
                .modifier_keys
                .get(key)
                .copied()
                .filter(|meaning| !meaning.is_empty())
                .unwrap_or_else(|| Modifiers::from_key_codes([*key]));
            modifiers.ctrl |= meaning.ctrl;
            modifiers.alt |= meaning.alt;
            modifiers.shift |= meaning.shift;
            modifiers.super_key |= meaning.super_key;
        }
        modifiers
    }

    /// The key that types a symbol, and any shift level it needs. Names are exact, because
    /// `Next` is the page key and `next` a media key. A letter means the key itself, since
    /// `Control+C` means the C key, not Control and Shift.
    pub fn lookup(&self, name: &str) -> Option<(u16, bool, bool)> {
        let xkb = xkbcommon_option()?;
        let mut letters = name.chars();
        let lowercase;
        let name = match (letters.next(), letters.next()) {
            (Some(letter), None) if letter.is_alphabetic() => {
                lowercase = letter.to_lowercase().to_string();
                lowercase.as_str()
            }
            _ => name,
        };
        let name = CString::new(name).ok()?;
        // Safe because the name is a valid C string for the length of the call.
        let symbol = unsafe {
            (xkb.xkb_keysym_from_name)(name.as_ptr(), xkb_keysym_flags::XKB_KEYSYM_NO_FLAGS)
        };
        let (key, level) = *self.symbols.get(&(symbol as u32))?;
        Some((
            key,
            level == SHIFT_LEVEL || level == SHIFT_ALTGR_LEVEL,
            level == ALTGR_LEVEL || level == SHIFT_ALTGR_LEVEL,
        ))
    }

    /// The key that types a character, and any shift level it needs. Line breaks and tabs are
    /// typed with their keys.
    pub fn lookup_char(&self, c: char) -> Option<(u16, bool, bool)> {
        match c {
            '\n' => return self.lookup("Return"),
            '\t' => return self.lookup("Tab"),
            _ => {}
        }
        let (key, level) = *self.chars.get(&c)?;
        Some((
            key,
            level == SHIFT_LEVEL || level == SHIFT_ALTGR_LEVEL,
            level == ALTGR_LEVEL || level == SHIFT_ALTGR_LEVEL,
        ))
    }
}

/// The dead key that puts an accent on the letter typed after it, by the accent's combining mark.
fn dead_key(mark: char) -> Option<&'static str> {
    Some(match mark {
        '\u{300}' => "dead_grave",
        '\u{301}' => "dead_acute",
        '\u{302}' => "dead_circumflex",
        '\u{303}' => "dead_tilde",
        '\u{304}' => "dead_macron",
        '\u{306}' => "dead_breve",
        '\u{307}' => "dead_abovedot",
        '\u{308}' => "dead_diaeresis",
        '\u{30A}' => "dead_abovering",
        '\u{30B}' => "dead_doubleacute",
        '\u{30C}' => "dead_caron",
        '\u{327}' => "dead_cedilla",
        '\u{328}' => "dead_ogonek",
        _ => return None,
    })
}

impl LayoutKeys {
    /// The keys that type a character, in order, each with the shift level it needs: its own key,
    /// or else the dead key for its accent and then the letter, as layouts like Brazilian or US
    /// International type `é`.
    pub fn keys_for_char(&self, c: char) -> Option<Vec<(u16, bool, bool)>> {
        use unicode_normalization::UnicodeNormalization;
        if let Some(key) = self.lookup_char(c) {
            return Some(vec![key]);
        }
        let mut parts = c.nfd();
        let (letter, mark) = (parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        Some(vec![self.lookup(dead_key(mark)?)?, self.lookup_char(letter)?])
    }
}

type CachedLayout = (LayoutName, Option<Arc<LayoutKeys>>);
static CURRENT_LAYOUT: Mutex<Option<CachedLayout>> = Mutex::new(None);

/// The keys of the layout in use, compiled again only when the layout changes. Where it can't be
/// told, as on systems without systemd's localectl, xkbcommon's default layout is used, which is
/// the one XKB_DEFAULT_LAYOUT names, or else US.
pub fn current_layout_keys() -> Option<Arc<LayoutKeys>> {
    let name = active_layout().unwrap_or_default();
    let mut cached = CURRENT_LAYOUT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached_name, keys)) = cached.as_ref() {
        if *cached_name == name {
            return keys.clone();
        }
    }
    let keys = LayoutKeys::compile(&name).map(Arc::new);
    *cached = Some((name, keys.clone()));
    keys
}

/// What held keys mean on the layout in use, remaps like Ctrl and Win swapped included.
pub fn held_modifiers(held: &HeldKeys) -> Modifiers {
    if held.is_empty() {
        return Modifiers::default();
    }
    match current_layout_keys() {
        Some(keys) => keys.modifiers(held),
        None => held.on_standard_layout(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::KeyCode;

    fn name(layout: &str, variant: &str) -> LayoutName {
        LayoutName::new(layout, variant)
    }

    #[test]
    fn gnome_values_give_the_first_keyboard_layout() {
        assert_eq!(parse_gnome_sources("[('xkb', 'br'), ('xkb', 'us')]"), Some(name("br", "")));
        assert_eq!(parse_gnome_sources("[('xkb', 'us+intl')]"), Some(name("us", "intl")));
        assert_eq!(
            parse_gnome_sources("[('ibus', 'mozc'), ('xkb', 'jp')]"),
            Some(name("jp", ""))
        );
        assert_eq!(parse_gnome_sources("@a(ss) []"), None);
    }

    #[test]
    fn localectl_gives_the_first_layout() {
        let output = "   System Locale: LANG=en_US.UTF-8\n       X11 Layout: br,us\n      X11 Variant: abnt2,\n";
        assert_eq!(parse_localectl(output), Some(name("br", "abnt2")));
        assert_eq!(parse_localectl("X11 Layout: (unset)\n"), None);
        assert_eq!(parse_localectl("System Locale: LANG=C\n"), None);
    }

    #[test]
    fn kxkbrc_gives_the_first_layout() {
        assert_eq!(parse_kxkbrc("[Layout]\nLayoutList=de,us\n"), Some(name("de", "")));
        assert_eq!(parse_kxkbrc("[Layout]\nUse=true\n"), None);
    }

    /// Skips where the library or layout data isn't installed.
    fn layout(layout: &str) -> Option<LayoutKeys> {
        let keys = LayoutKeys::compile(&name(layout, ""));
        if keys.is_none() {
            eprintln!("skipped: no xkbcommon or {layout} layout");
        }
        keys
    }

    fn key(keys: &LayoutKeys, symbol: &str) -> u16 {
        keys.lookup(symbol).unwrap().0
    }

    #[test]
    fn letters_follow_the_layout() {
        let Some(us) = layout("us") else { return };
        assert_eq!(key(&us, "z"), KeyCode::KEY_Z.0);
        let Some(french) = layout("fr") else { return };
        assert_eq!(key(&french, "z"), KeyCode::KEY_W.0);
        assert_eq!(key(&french, "a"), KeyCode::KEY_Q.0);
        let Some(german) = layout("de") else { return };
        assert_eq!(key(&german, "z"), KeyCode::KEY_Y.0);
    }

    #[test]
    fn a_capital_letter_is_the_same_key_without_shift() {
        let Some(us) = layout("us") else { return };
        assert_eq!(us.lookup("C"), Some((KeyCode::KEY_C.0, false, false)));
    }

    #[test]
    fn symbols_report_the_shift_level_they_need() {
        let Some(us) = layout("us") else { return };
        assert_eq!(us.lookup("exclam"), Some((KeyCode::KEY_1.0, true, false)));
        assert_eq!(us.lookup("minus"), Some((KeyCode::KEY_MINUS.0, false, false)));
        let Some(german) = layout("de") else { return };
        assert_eq!(german.lookup("at"), Some((KeyCode::KEY_Q.0, false, true)));
    }

    #[test]
    fn special_keys_stay_put_on_every_layout() {
        let Some(french) = layout("fr") else { return };
        assert_eq!(key(&french, "XF86Explorer"), KeyCode::KEY_FILE.0);
        assert_eq!(key(&french, "XF86WWW"), KeyCode::KEY_WWW.0);
        assert_eq!(key(&french, "Left"), KeyCode::KEY_LEFT.0);
        assert_eq!(key(&french, "F12"), KeyCode::KEY_F12.0);
        assert_eq!(key(&french, "Control_L"), KeyCode::KEY_LEFTCTRL.0);
    }

    #[test]
    fn your_brazilian_layout_works() {
        let Some(brazilian) = layout("br") else { return };
        assert_eq!(key(&brazilian, "c"), KeyCode::KEY_C.0);
        assert_eq!(key(&brazilian, "ccedilla"), KeyCode::KEY_SEMICOLON.0);
    }

    #[test]
    fn with_no_layout_named_the_default_types_punctuation() {
        let Some(default) = LayoutKeys::compile(&LayoutName::default()) else { return };
        assert!(default.lookup_char('.').is_some());
        assert!(default.lookup_char('/').is_some());
        assert!(default.lookup_char('a').is_some());
    }

    #[test]
    fn characters_are_typed_the_way_the_layout_has_them() {
        let Some(us) = layout("us") else { return };
        assert_eq!(us.lookup_char('a'), Some((KeyCode::KEY_A.0, false, false)));
        assert_eq!(us.lookup_char('A'), Some((KeyCode::KEY_A.0, true, false)));
        assert_eq!(us.lookup_char('1'), Some((KeyCode::KEY_1.0, false, false)));
        assert_eq!(us.lookup_char(' '), Some((KeyCode::KEY_SPACE.0, false, false)));
        assert_eq!(us.lookup_char('\n'), Some((KeyCode::KEY_ENTER.0, false, false)));
        assert_eq!(us.lookup_char('ç'), None);
        let Some(brazilian) = layout("br") else { return };
        assert_eq!(brazilian.lookup_char('Ç'), Some((KeyCode::KEY_SEMICOLON.0, true, false)));
        let Some(french) = layout("fr") else { return };
        assert_eq!(french.lookup_char('a'), Some((KeyCode::KEY_Q.0, false, false)));
    }

    #[test]
    fn accented_letters_are_typed_with_dead_keys() {
        let Some(brazilian) = layout("br") else { return };
        let acute = brazilian.lookup("dead_acute").unwrap();
        let tilde = brazilian.lookup("dead_tilde").unwrap();
        assert_eq!(brazilian.keys_for_char('é'), Some(vec![acute, (KeyCode::KEY_E.0, false, false)]));
        assert_eq!(brazilian.keys_for_char('Á'), Some(vec![acute, (KeyCode::KEY_A.0, true, false)]));
        assert_eq!(brazilian.keys_for_char('ã'), Some(vec![tilde, (KeyCode::KEY_A.0, false, false)]));
        // A letter the layout has needs no dead key.
        assert_eq!(brazilian.keys_for_char('ç').map(|keys| keys.len()), Some(1));
        // Without dead keys, it can't be typed.
        let Some(us) = layout("us") else { return };
        assert_eq!(us.keys_for_char('é'), None);
    }

    #[test]
    fn unknown_names_have_no_key() {
        let Some(us) = layout("us") else { return };
        assert_eq!(us.lookup("NoSuchSymbol"), None);
        assert_eq!(us.lookup("control"), None);
    }

    #[test]
    fn options_are_read_from_each_desktop_and_the_x_server() {
        let mut swapped = name("br", "abnt2");
        swapped.options = "ctrl:swap_lwin_lctl,ctrl:swap_rwin_rctl".to_string();
        assert_eq!(
            parse_localectl(
                "   X11 Layout: br,us\n  X11 Variant: abnt2,\n  X11 Options: ctrl:swap_lwin_lctl,ctrl:swap_rwin_rctl\n"
            ),
            Some(swapped.clone())
        );
        assert_eq!(
            parse_xkb_rules_names(
                b"evdev\0pc105\0br,us\0abnt2,\0ctrl:swap_lwin_lctl,ctrl:swap_rwin_rctl\0"
            ),
            Some(swapped)
        );
        let mut kde = name("de", "");
        kde.options = "caps:ctrl_modifier".to_string();
        assert_eq!(
            parse_kxkbrc("[Layout]\nLayoutList=de,us\nOptions=caps:ctrl_modifier\nResetOldOptions=true\n"),
            Some(kde)
        );
        assert_eq!(parse_xkb_rules_names(b"evdev\0pc105\0\0\0\0"), None);
    }

    fn with_options(options: &str) -> Option<LayoutKeys> {
        let mut layout = name("us", "");
        layout.options = options.to_string();
        LayoutKeys::compile(&layout)
    }

    fn meaning(keys: &LayoutKeys, codes: &[u16]) -> Modifiers {
        keys.modifiers(&HeldKeys::from_key_codes(codes.iter().copied()))
    }

    #[test]
    fn held_keys_mean_what_the_layout_makes_them() {
        let Some(us) = layout("us") else { return };
        assert_eq!(meaning(&us, &[29]), Modifiers::parse("ctrl"));
        assert_eq!(meaning(&us, &[125, 42]), Modifiers::parse("super+shift"));
        assert!(meaning(&us, &[58]).is_empty());

        let Some(swapped) = with_options("ctrl:swap_lwin_lctl,ctrl:swap_rwin_rctl") else { return };
        assert_eq!(meaning(&swapped, &[125]), Modifiers::parse("ctrl"));
        assert_eq!(meaning(&swapped, &[126]), Modifiers::parse("ctrl"));
        assert_eq!(meaning(&swapped, &[29]), Modifiers::parse("super"));
        assert_eq!(meaning(&swapped, &[97]), Modifiers::parse("super"));

        let Some(both_ctrl) = with_options("altwin:ctrl_win") else { return };
        assert_eq!(meaning(&both_ctrl, &[125]), Modifiers::parse("ctrl"));
        assert_eq!(meaning(&both_ctrl, &[29]), Modifiers::parse("ctrl"));

        let Some(caps_ctrl) = with_options("caps:ctrl_modifier") else { return };
        assert_eq!(meaning(&caps_ctrl, &[58]), Modifiers::parse("ctrl"));
    }

    #[test]
    fn right_alt_stays_alt_where_it_types_the_third_level() {
        let Some(brazilian) = layout("br") else { return };
        assert_eq!(meaning(&brazilian, &[100]), Modifiers::parse("alt"));
    }

    #[test]
    fn an_unknown_layout_does_not_compile() {
        assert!(LayoutKeys::compile(&name("no-such-layout", "")).is_none());
    }
}
