//! The keyboard shortcuts an app lists in its own Keyboard Shortcuts window, or keeps in its
//! settings, read without starting it, so that gestures can be made for them by their names. GTK
//! apps keep that window's layout among their resources, built into the program or in a file
//! beside it. Only shortcuts with their keys written out are read: a newer app may name its command
//! instead, and its keys are then only known while it runs. An app whose shortcuts can be changed,
//! like Ptyxis, keeps them in settings of their own, with the keys set now.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gio::{glib, prelude::*};

/// One of an app's shortcuts, by the name its window gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct AppShortcut {
    /// The group it's in, like "Editing".
    pub section: String,
    pub title: String,
    /// As Gestos writes keys: `Control+Shift+Z`.
    pub keys: String,
}

thread_local! {
    // Read once each, as reading means going through the app's program.
    static FOUND: RefCell<HashMap<String, Rc<Vec<AppShortcut>>>> = RefCell::new(HashMap::new());
}

/// The shortcuts an app lists, by its desktop file ID, with or without `.desktop` as Gestos keeps
/// it, or none for an app without the list.
pub fn shortcuts_of(app_id: &str) -> Rc<Vec<AppShortcut>> {
    if let Some(found) = FOUND.with(|found| found.borrow().get(app_id).cloned()) {
        return found;
    }
    let found = Rc::new(read_shortcuts(app_id));
    FOUND.with(|cache| cache.borrow_mut().insert(app_id.to_string(), Rc::clone(&found)));
    found
}

/// What an app calls a shortcut, if it lists those keys.
pub fn title_for(app_id: &str, keys: &str) -> Option<String> {
    shortcuts_of(app_id)
        .iter()
        .find(|shortcut| crate::actions::same_keys(&shortcut.keys, keys))
        .map(|shortcut| shortcut.title.clone())
}

fn read_shortcuts(app_id: &str) -> Vec<AppShortcut> {
    let stem = app_id.trim_end_matches(".desktop");
    let Some(info) = gio::DesktopAppInfo::new(&format!("{stem}.desktop")) else { return Vec::new() };
    let program = info.executable();
    let program = if program.is_absolute() {
        Some(program)
    } else {
        glib::find_program_in_path(&program)
    };
    let mut resources: Vec<gio::Resource> =
        program.iter().flat_map(|program| resources_in_program(program)).collect();
    let program_name = info.executable().file_name().map(|name| name.to_string_lossy().to_lowercase());
    for file in resource_files(&stem.to_lowercase(), program_name.as_deref()) {
        if let Ok(resource) = gio::Resource::load(&file) {
            resources.push(resource);
        }
    }
    // Settings are named as the app is, capitals and all.
    let mut shortcuts: Vec<AppShortcut> = shortcuts_in_settings(stem);
    for resource in resources {
        for layout in shortcut_layouts(&resource, "/") {
            for shortcut in parse_layout(&layout) {
                // Some are listed twice, as for writing in either direction.
                if !shortcuts.iter().any(|known| crate::actions::same_keys(&known.keys, &shortcut.keys)) {
                    shortcuts.push(shortcut);
                }
            }
        }
    }
    shortcuts
}

/// The shortcuts an app keeps in settings of their own, like `org.gnome.Ptyxis.Shortcuts`, each
/// named by its key's summary, with the keys set now. One with none set is left out.
fn shortcuts_in_settings(app_stem: &str) -> Vec<AppShortcut> {
    let Some(source) = gio::SettingsSchemaSource::default() else { return Vec::new() };
    let found = ["Shortcuts", "shortcuts", "keybindings"].iter().find_map(|suffix| {
        let id = format!("{app_stem}.{suffix}");
        source.lookup(&id, true).map(|schema| (id, schema))
    });
    let Some((id, schema)) = found else { return Vec::new() };
    let settings = gio::Settings::new(&id);
    let mut shortcuts: Vec<AppShortcut> = schema
        .list_keys()
        .iter()
        .filter_map(|key| {
            let value = settings.value(key);
            // One accelerator, or a list of them, of which the first.
            let accelerator = value
                .str()
                .map(str::to_string)
                .or_else(|| value.get::<Vec<String>>().and_then(|all| all.into_iter().next()))?;
            let keys = keys_from_accelerator(&accelerator)?;
            let title = schema.key(key).summary().map(|summary| summary.to_string())?;
            Some(AppShortcut { section: String::new(), title, keys })
        })
        .collect();
    shortcuts.sort_by_cached_key(|shortcut| in_order(&shortcut.title));
    shortcuts
}

/// A name to sort by, with a number at its end in order: Tab 2 before Tab 10.
fn in_order(title: &str) -> (String, u32) {
    let digits = title.len() - title.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (text, number) = title.split_at(title.len() - digits);
    (text.to_lowercase(), number.parse().unwrap_or(0))
}

/// The resources built into a program, which ELF programs keep in sections of their own.
fn resources_in_program(program: &Path) -> Vec<gio::Resource> {
    elf_sections(program, ".gresource.")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|data| gio::Resource::from_data(&glib::Bytes::from_owned(data)).ok())
        .collect()
}

/// The data of each section whose name starts so, in a 64-bit little-endian ELF file. Only the
/// headers and those sections are read, as programs can be large.
fn elf_sections(path: &Path, prefix: &str) -> std::io::Result<Vec<Vec<u8>>> {
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header)?;
    if &header[..4] != b"\x7fELF" || header[4] != 2 || header[5] != 1 {
        return Ok(Vec::new());
    }
    let u16_at = |bytes: &[u8], at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
    let u32_at = |bytes: &[u8], at: usize| {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    };
    let u64_at = |bytes: &[u8], at: usize| {
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    };
    let table_at = u64_at(&header, 0x28);
    let (entry_size, count, names_index) =
        (u16_at(&header, 0x3A), u16_at(&header, 0x3C), u16_at(&header, 0x3E));
    if entry_size < 0x28 || names_index >= count {
        return Ok(Vec::new());
    }
    let mut table = vec![0u8; entry_size * count];
    file.seek(SeekFrom::Start(table_at))?;
    file.read_exact(&mut table)?;
    let section = |index: usize| {
        let entry = &table[index * entry_size..(index + 1) * entry_size];
        (u32_at(entry, 0), u64_at(entry, 0x18), u64_at(entry, 0x20))
    };
    let mut read = |offset: u64, size: u64| -> std::io::Result<Vec<u8>> {
        let mut data = vec![0u8; size as usize];
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut data)?;
        Ok(data)
    };
    let (_, names_at, names_size) = section(names_index);
    let names = read(names_at, names_size)?;
    let mut sections = Vec::new();
    for index in 0..count {
        let (name_at, offset, size) = section(index);
        let name = names.get(name_at..).unwrap_or_default();
        let name = &name[..name.iter().position(|&byte| byte == 0).unwrap_or(name.len())];
        if name.starts_with(prefix.as_bytes()) && size > 0 {
            sections.push(read(offset, size)?);
        }
    }
    Ok(sections)
}

/// Resource files kept beside a program, in a folder of the data folders named after the app or
/// its program, like `/usr/share/gnome-maps/org.gnome.Maps.data.gresource`.
fn resource_files(app_stem: &str, program_name: Option<&str>) -> Vec<PathBuf> {
    let matches = |name: &str| {
        let name = name.to_lowercase();
        name.contains(app_stem) || program_name.is_some_and(|program| name == program)
    };
    let mut files = Vec::new();
    for data_dir in glib::system_data_dirs() {
        let Ok(folders) = std::fs::read_dir(&data_dir) else { continue };
        for folder in folders.flatten() {
            let folder_name = folder.file_name().to_string_lossy().to_string();
            let Ok(inside) = std::fs::read_dir(folder.path()) else { continue };
            for file in inside.flatten() {
                let file_name = file.file_name().to_string_lossy().to_string();
                if file_name.ends_with(".gresource") && (matches(&folder_name) || matches(&file_name)) {
                    files.push(file.path());
                }
            }
        }
    }
    files
}

/// The layouts of the Keyboard Shortcuts windows in a resource.
fn shortcut_layouts(resource: &gio::Resource, folder: &str) -> Vec<String> {
    let Ok(children) = resource.enumerate_children(folder, gio::ResourceLookupFlags::NONE) else {
        return Vec::new();
    };
    let mut layouts = Vec::new();
    for child in children {
        let path = format!("{folder}{child}");
        if child.ends_with('/') {
            layouts.extend(shortcut_layouts(resource, &path));
        } else if child.ends_with(".ui") && (child.contains("help-overlay") || child.contains("shortcuts")) {
            if let Ok(data) = resource.lookup_data(&path, gio::ResourceLookupFlags::NONE) {
                layouts.push(String::from_utf8_lossy(&data).into_owned());
            }
        }
    }
    layouts
}

/// The shortcuts in a window's layout, under the group each is in, as GTK's and libadwaita's
/// shortcuts windows write them.
fn parse_layout(layout: &str) -> Vec<AppShortcut> {
    let property = regex::Regex::new(r#"<property name="([\w-]+)"[^>]*>([^<]*)</property>"#).unwrap();
    let mut section = String::new();
    let mut shortcuts = Vec::new();
    for object in layout.split("<object class=\"").skip(1) {
        let class = &object[..object.find('"').unwrap_or(0)];
        // Only up to the object inside it, if any.
        let own = &object[..object.find("<object").unwrap_or(object.len())];
        let value = |name: &str| {
            property
                .captures_iter(own)
                .find(|captures| &captures[1] == name)
                .map(|captures| unescape(&captures[2]))
        };
        match class {
            "GtkShortcutsGroup" | "AdwShortcutsSection" => {
                section = value("title").unwrap_or_default();
            }
            "GtkShortcutsShortcut" | "AdwShortcutsItem" => {
                let keys = value("accelerator").as_deref().and_then(keys_from_accelerator);
                if let (Some(keys), Some(title)) = (keys, value("title")) {
                    shortcuts.push(AppShortcut { section: section.clone(), title, keys });
                }
            }
            _ => {}
        }
    }
    shortcuts
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Keys as Gestos writes them, from an accelerator as GTK writes it: `<ctrl><shift>z` is
/// `Control+Shift+Z`. Of several, the first. One of keys pressed in turn, or a range of them, is
/// not a shortcut a gesture can press.
fn keys_from_accelerator(accelerator: &str) -> Option<String> {
    let accelerator = accelerator.split_whitespace().next()?;
    let mut rest = accelerator;
    let (mut control, mut alt, mut shift, mut super_key) = (false, false, false, false);
    while let Some(after) = rest.strip_prefix('<') {
        let end = after.find('>')?;
        match after[..end].to_lowercase().as_str() {
            "ctrl" | "control" | "primary" => control = true,
            "alt" | "mod1" => alt = true,
            "shift" => shift = true,
            "super" | "meta" | "hyper" => super_key = true,
            _ => return None,
        }
        rest = &after[end + 1..];
    }
    if rest.is_empty() || rest.contains('+') || rest.contains("...") {
        return None;
    }
    let key = if rest.chars().count() == 1 { rest.to_uppercase() } else { rest.to_string() };
    let modifiers = [(control, "Control"), (alt, "Alt"), (shift, "Shift"), (super_key, "Super")];
    let mut keys: Vec<&str> = modifiers.iter().filter(|(held, _)| *held).map(|(_, name)| *name).collect();
    keys.push(&key);
    Some(keys.join("+"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reads the apps installed here, so it's only run on request.
    #[test]
    #[ignore]
    fn installed_apps_lists_are_read() {
        let calculator = shortcuts_of("org.gnome.Calculator");
        assert!(calculator.iter().any(|s| s.title == "Switch to Hexadecimal" && s.keys == "Control+H"));
        assert_eq!(title_for("org.gnome.Calculator.desktop", "Control+H").as_deref(), Some("Switch to Hexadecimal"));
        assert!(!shortcuts_of("org.gnome.Calculator.desktop").is_empty());
        let ptyxis = shortcuts_of("org.gnome.Ptyxis");
        assert!(ptyxis.iter().any(|s| s.title == "Close Tab"), "{ptyxis:?}");
        let maps = shortcuts_of("org.gnome.Maps.desktop");
        assert!(!maps.is_empty(), "{maps:?}");
        assert!(shortcuts_of("no.such.App.desktop").is_empty());
    }

    #[test]
    fn numbered_names_are_in_order() {
        let mut titles = ["Focus Tab 10", "Focus Tab 2", "Close Tab", "Focus Tab 1"];
        titles.sort_by_cached_key(|title| in_order(title));
        assert_eq!(titles, ["Close Tab", "Focus Tab 1", "Focus Tab 2", "Focus Tab 10"]);
    }

    #[test]
    fn accelerators_are_written_as_gestos_keys() {
        assert_eq!(keys_from_accelerator("<ctrl><shift>z").as_deref(), Some("Control+Shift+Z"));
        assert_eq!(keys_from_accelerator("<primary>w <ctrl>q").as_deref(), Some("Control+W"));
        assert_eq!(keys_from_accelerator("<alt>Left").as_deref(), Some("Alt+Left"));
        assert_eq!(keys_from_accelerator("F10").as_deref(), Some("F10"));
        assert_eq!(keys_from_accelerator("<alt>bracketleft+bracketright"), None);
        assert_eq!(keys_from_accelerator("<ctrl>0...9"), None);
        assert_eq!(keys_from_accelerator(""), None);
    }

    #[test]
    fn a_layout_lists_its_shortcuts_with_keys_under_their_groups() {
        let gtk = r#"<object class="GtkShortcutsWindow"><child><object class="GtkShortcutsSection">
            <child><object class="GtkShortcutsGroup"><property name="title" translatable="yes">Editing</property>
            <child><object class="GtkShortcutsShortcut"><property name="accelerator">&lt;ctrl&gt;z</property>
            <property name="title" translatable="yes">Undo &amp; Back</property></object></child>
            <child><object class="GtkShortcutsShortcut"><property name="shortcut-type">gesture-pinch</property>
            <property name="title">Zoom</property></object></child></object></child></object></child></object>"#;
        assert_eq!(
            parse_layout(gtk),
            [AppShortcut { section: "Editing".into(), title: "Undo & Back".into(), keys: "Control+Z".into() }]
        );
        let adw = r#"<object class="AdwShortcutsSection"><property name="title">General</property>
            <child><object class="AdwShortcutsItem"><property name="action-name">win.close</property>
            <property name="title">Close</property></object></child>
            <child><object class="AdwShortcutsItem"><property name="accelerator">&lt;ctrl&gt;H</property>
            <property name="title">Switch to Hexadecimal</property></object></child></object>"#;
        assert_eq!(
            parse_layout(adw),
            [AppShortcut { section: "General".into(), title: "Switch to Hexadecimal".into(), keys: "Control+H".into() }]
        );
    }
}
