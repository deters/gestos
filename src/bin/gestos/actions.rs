use crate::state::*;
use gestos::config::ActionType;
use gtk::prelude::*;
use gtk4 as gtk;
use gtk::gio;

pub fn action_matches(a: &ActionType, opt: &EditorActionOption) -> bool {
    match (a, &opt.action_type) {
        (ActionType::Gnome(k1), ActionType::Gnome(k2)) => k1 == k2,
        (ActionType::Kde(c1, s1), ActionType::Kde(c2, s2)) => c1 == c2 && s1 == s2,
        (ActionType::Execute(cmd1), ActionType::Execute(cmd2)) => {
            if opt.category == 6 {
                // Custom GNOME shortcut command must match exactly
                cmd1 == cmd2
            } else {
                // Generic execute matches any command
                opt.category == 7
            }
        }
        // The generic Keyboard Shortcut has no keys of its own, so it matches any.
        (ActionType::Keypress(k1), ActionType::Keypress(k2)) => {
            opt.category == 0 && (k2.is_empty() || k1 == k2)
        }
        (ActionType::Click(_), ActionType::Click(_)) => opt.category == 0,
        (at1, at2) => at1 == at2,
    }
}

/// Whether an action needs a value from the user. Most don't: GNOME and KDE actions, custom
/// GNOME shortcuts and standard shortcuts are complete as they are. Only the generic Keyboard
/// Shortcut takes keys, and only the generic Run Command takes a command.
pub fn action_needs_parameter(opt: &EditorActionOption) -> bool {
    match &opt.action_type {
        ActionType::Keypress(keys) => keys.is_empty(),
        ActionType::Click(_) => true,
        ActionType::Execute(_) => opt.category == 7,
        _ => false,
    }
}

/// Keybinding keys hold a list of accelerators. The schemas also hold settings that aren't
/// actions, such as the volume step and the list of custom shortcut paths.
fn is_gnome_action_key(key: &str, holds_string_list: bool) -> bool {
    holds_string_list && key != "custom-keybindings"
}

/// The standard shortcuts apps share, from https://developer.gnome.org/hig/reference/keyboard.html
/// as name, keys and description. Left out are Duplicate, whose Ctrl+U is also Underline, and the
/// reserved system shortcuts, which aren't app actions.
const STANDARD_SHORTCUTS: &[(&str, &str, &str)] = &[
    // Basic
    ("Close", "Control+W", "Close the current tab or window"),
    ("Quit", "Control+Q", "Close the app and all its windows"),
    ("Help", "F1", "Open the app's help"),
    ("Side Pane", "F9", "Show or hide the side pane"),
    ("Menu", "F10", "Open the app's menu"),
    ("Keyboard Shortcuts", "Control+question", "Open the keyboard shortcuts dialog"),
    ("Preferences", "Control+comma", "Open the preferences window"),
    // Content & Files
    ("New", "Control+N", "Create a new item"),
    ("Open", "Control+O", "Open an existing item"),
    ("Save", "Control+S", "Save the current item"),
    ("Save As", "Shift+Control+S", "Save the current item under a new name"),
    ("Print", "Control+P", "Print the current item"),
    ("Print Preview", "Shift+Control+P", "Show a print preview of the current item"),
    ("Send", "Control+M", "Send or share the current item"),
    ("Properties", "Alt+Return", "Show the properties of the current item"),
    // Editing
    ("Undo", "Control+Z", "Reverse the last action"),
    ("Redo", "Shift+Control+Z", "Reapply the last action that was undone"),
    ("Cut", "Control+X", "Remove the selection and copy it to the clipboard"),
    ("Copy", "Control+C", "Copy the selection to the clipboard"),
    ("Paste", "Control+V", "Insert the clipboard contents"),
    ("Paste Special", "Shift+Control+V", "Insert another version of the clipboard contents"),
    ("Select All", "Control+A", "Select all content"),
    ("Deselect All", "Shift+Control+A", "Deselect all content"),
    ("Find", "Control+F", "Show the find bar"),
    ("Find Next", "Control+G", "Select the next search result"),
    ("Find Previous", "Shift+Control+G", "Select the previous search result"),
    ("Replace", "Control+H", "Show find and replace"),
    // View
    ("Zoom In", "Control+plus", "Enlarge the content"),
    ("Zoom Out", "Control+minus", "Reduce the content"),
    ("Normal Size", "Control+0", "Reset the zoom"),
    ("Reload", "Control+R", "Reload the current view or item"),
    // Formatting
    ("Bold", "Control+B", "Toggle bold for the selected text"),
    ("Italic", "Control+I", "Toggle italics for the selected text"),
    ("Underline", "Control+U", "Toggle underlining for the selected text"),
    // Bookmarks
    ("Add Bookmark", "Control+D", "Bookmark the current location"),
    ("Edit Bookmarks", "Control+Alt+D", "Open the list of bookmarks"),
    // Navigation
    ("Back", "Alt+Left", "Go to the previous location"),
    ("Forward", "Alt+Right", "Go to the next location"),
    ("Up", "Alt+Up", "Go to the parent location"),
    ("Home", "Alt+Home", "Go to the starting location"),
    ("Enter Location", "Control+L", "Type an address"),
    ("Previous Page", "Page_Up", "Go to the previous page"),
    ("Next Page", "Page_Down", "Go to the next page"),
    ("First Page", "Control+Home", "Go to the first page"),
    ("Last Page", "Control+End", "Go to the last page"),
];

/// A standard shortcut's name with its keys as the GNOME guidelines write them: "Copy (Ctrl+C)".
fn labelled_shortcut_name(name: &str, keys: &str) -> String {
    let label = keys
        .split('+')
        .map(|key| match key {
            "Control" => "Ctrl",
            "question" => "?",
            "comma" => ",",
            "plus" => "+",
            "minus" => "-",
            "Left" => "←",
            "Right" => "→",
            "Up" => "↑",
            "Page_Up" => "PageUp",
            "Page_Down" => "PageDown",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("+");
    format!("{name} ({label})")
}

/// The name of the standard shortcut with these keys, if they are one.
pub fn standard_shortcut_name(keys: &str) -> Option<String> {
    STANDARD_SHORTCUTS
        .iter()
        .find(|(_, shortcut_keys, _)| *shortcut_keys == keys)
        .map(|(name, keys, _)| labelled_shortcut_name(name, keys))
}

pub fn get_static_action_options() -> Vec<EditorActionOption> {
    // Before the generic Keyboard Shortcut, so that a gesture with these keys finds its own name.
    let mut options: Vec<EditorActionOption> = STANDARD_SHORTCUTS
        .iter()
        .map(|(name, keys, tooltip)| EditorActionOption {
            category: 0,
            action_type: ActionType::Keypress(keys.to_string()),
            name: labelled_shortcut_name(name, keys),
            tooltip: tooltip.to_string(),
        })
        .collect();
    options.extend([
        // Input Emulation (0)
        EditorActionOption {
            category: 0,
            action_type: ActionType::Keypress(String::new()),
            name: "Custom Keyboard Shortcut".to_string(),
            tooltip: "Simulate keys like Control_L+Alt_L+t".to_string(),
        },
        // Window Management (1)
        EditorActionOption {
            category: 1,
            action_type: ActionType::Kill,
            name: "Close Window".to_string(),
            tooltip: "Close the active application window".to_string(),
        },
        EditorActionOption {
            category: 1,
            action_type: ActionType::ToggleMaximized,
            name: "Toggle Maximized".to_string(),
            tooltip: "Toggle maximize state of active window".to_string(),
        },
        EditorActionOption {
            category: 1,
            action_type: ActionType::Maximize,
            name: "Maximize Window".to_string(),
            tooltip: "Maximize active window".to_string(),
        },
        EditorActionOption {
            category: 1,
            action_type: ActionType::Restore,
            name: "Restore Window".to_string(),
            tooltip: "Restore window from maximized state".to_string(),
        },
        EditorActionOption {
            category: 1,
            action_type: ActionType::Iconify,
            name: "Minimize Window".to_string(),
            tooltip: "Minimize active window".to_string(),
        },
        EditorActionOption {
            category: 1,
            action_type: ActionType::Raise,
            name: "Raise Window".to_string(),
            tooltip: "Bring window to front".to_string(),
        },
        EditorActionOption {
            category: 1,
            action_type: ActionType::Lower,
            name: "Lower Window".to_string(),
            tooltip: "Send window to back".to_string(),
        },
        EditorActionOption {
            category: 1,
            action_type: ActionType::ToggleFullscreen,
            name: "Toggle Fullscreen".to_string(),
            tooltip: "Toggle fullscreen mode".to_string(),
        },
        EditorActionOption {
            category: 1,
            action_type: ActionType::ShowDesktop,
            name: "Show Desktop".to_string(),
            tooltip: "Minimize all windows or toggle show desktop".to_string(),
        },
        // Workspaces & Overview (2)
        EditorActionOption {
            category: 2,
            action_type: ActionType::WorkspaceLeft,
            name: "Workspace Left".to_string(),
            tooltip: "Switch to workspace on the left".to_string(),
        },
        EditorActionOption {
            category: 2,
            action_type: ActionType::WorkspaceRight,
            name: "Workspace Right".to_string(),
            tooltip: "Switch to workspace on the right".to_string(),
        },
        EditorActionOption {
            category: 2,
            action_type: ActionType::WorkspaceUp,
            name: "Workspace Up".to_string(),
            tooltip: "Workspace Up".to_string(),
        },
        EditorActionOption {
            category: 2,
            action_type: ActionType::WorkspaceDown,
            name: "Workspace Down".to_string(),
            tooltip: "Workspace Down".to_string(),
        },
        EditorActionOption {
            category: 2,
            action_type: ActionType::ShowOverview,
            name: "Show Overview".to_string(),
            tooltip: "Toggle workspace overview".to_string(),
        },
        EditorActionOption {
            category: 2,
            action_type: ActionType::ShowAppGrid,
            name: "Show App Grid".to_string(),
            tooltip: "Toggle applications menu/grid".to_string(),
        },
        // Media & Audio (3)
        EditorActionOption {
            category: 3,
            action_type: ActionType::VolumeUp,
            name: "Volume Up".to_string(),
            tooltip: "Increase audio volume".to_string(),
        },
        EditorActionOption {
            category: 3,
            action_type: ActionType::VolumeDown,
            name: "Volume Down".to_string(),
            tooltip: "Decrease audio volume".to_string(),
        },
        EditorActionOption {
            category: 3,
            action_type: ActionType::VolumeMute,
            name: "Volume Mute".to_string(),
            tooltip: "Mute/unmute audio volume".to_string(),
        },
        EditorActionOption {
            category: 3,
            action_type: ActionType::MediaPlay,
            name: "Play/Pause Media".to_string(),
            tooltip: "Toggle playback of media players".to_string(),
        },
        EditorActionOption {
            category: 3,
            action_type: ActionType::MediaNext,
            name: "Next Track".to_string(),
            tooltip: "Skip to next track".to_string(),
        },
        EditorActionOption {
            category: 3,
            action_type: ActionType::MediaPrev,
            name: "Previous Track".to_string(),
            tooltip: "Skip to previous track".to_string(),
        },
        // System & Settings (4)
        EditorActionOption {
            category: 4,
            action_type: ActionType::LockScreen,
            name: "Lock Screen".to_string(),
            tooltip: "Lock the computer screen".to_string(),
        },
        EditorActionOption {
            category: 4,
            action_type: ActionType::Terminal,
            name: "Open Terminal".to_string(),
            tooltip: "Launch default terminal emulator".to_string(),
        },
        EditorActionOption {
            category: 4,
            action_type: ActionType::ControlCenter,
            name: "Control Center".to_string(),
            tooltip: "Launch system settings panel".to_string(),
        },
        EditorActionOption {
            category: 4,
            action_type: ActionType::Logout,
            name: "Log Out".to_string(),
            tooltip: "Log out of session".to_string(),
        },
        EditorActionOption {
            category: 4,
            action_type: ActionType::Screenshot,
            name: "Take Screenshot".to_string(),
            tooltip: "Take screen capture".to_string(),
        },
        EditorActionOption {
            category: 4,
            action_type: ActionType::ScreenshotWindow,
            name: "Screenshot Window".to_string(),
            tooltip: "Take screenshot of active window".to_string(),
        },
        EditorActionOption {
            category: 4,
            action_type: ActionType::ScreenshotArea,
            name: "Screenshot Area".to_string(),
            tooltip: "Take screenshot of selection area".to_string(),
        },
        // Applications (5)
        EditorActionOption {
            category: 5,
            action_type: ActionType::Www,
            name: "Web Browser".to_string(),
            tooltip: "Launch web browser".to_string(),
        },
        EditorActionOption {
            category: 5,
            action_type: ActionType::Home,
            name: "Home Folder".to_string(),
            tooltip: "Open file manager in home directory".to_string(),
        },
        EditorActionOption {
            category: 5,
            action_type: ActionType::Email,
            name: "Email Client".to_string(),
            tooltip: "Launch email reader".to_string(),
        },
        EditorActionOption {
            category: 5,
            action_type: ActionType::Search,
            name: "System Search".to_string(),
            tooltip: "Open system search tool".to_string(),
        },
        EditorActionOption {
            category: 5,
            action_type: ActionType::Calculator,
            name: "Calculator".to_string(),
            tooltip: "Open calculator application".to_string(),
        },
        // Other/Internal (7)
        EditorActionOption {
            category: 7,
            action_type: ActionType::Execute(String::new()),
            name: "Run Command (Execute)".to_string(),
            tooltip: "Run custom shell command".to_string(),
        },
        EditorActionOption {
            category: 7,
            action_type: ActionType::Abort,
            name: "Abort Gesture".to_string(),
            tooltip: "Ignore gesture matching".to_string(),
        },
    ]);
    // It runs in the GNOME Shell extension, so no other desktop can do it.
    let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_lowercase().contains("gnome"))
        .unwrap_or(false);
    if is_gnome {
        options.push(EditorActionOption {
            category: 1,
            action_type: ActionType::ToggleTileWindows,
            name: "Toggle Tile Windows".to_string(),
            tooltip: "Fit the windows on the workspace together, or put them back".to_string(),
        });
        options.push(EditorActionOption {
            category: 1,
            action_type: ActionType::ToggleArrangeWindows,
            name: "Toggle Arrange Windows".to_string(),
            tooltip: "Keep the windows on the workspace from overlapping, at their own sizes, or put them back".to_string(),
        });
    }
    options
}

pub fn fetch_kde_action_options() -> Vec<EditorActionOption> {
    let mut options = Vec::new();

    // Check if KDE is running to avoid unnecessary D-Bus calls/timeouts on other DEs
    let is_kde = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_lowercase().contains("kde"))
        .unwrap_or(false);
    if !is_kde {
        return options;
    }

    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: Failed to connect to D-Bus session bus: {}", e);
            return options;
        }
    };

    let kglobalaccel_proxy = match zbus::blocking::Proxy::new(
        &conn,
        "org.kde.kglobalaccel",
        "/kglobalaccel",
        "org.kde.KGlobalAccel",
    ) {
        Ok(p) => p,
        Err(_) => return options,
    };

    let component_paths: Vec<zbus::zvariant::OwnedObjectPath> = match kglobalaccel_proxy.call("allComponents", &()) {
        Ok(paths) => paths,
        Err(_) => {
            return options;
        }
    };

    for path in component_paths {
        let path_str = path.as_str();
        let comp_name = match path_str.rsplit('/').next() {
            Some(name) => name.to_string(),
            None => continue,
        };

        let proxy = match zbus::blocking::Proxy::new(
            &conn,
            "org.kde.kglobalaccel",
            path_str,
            "org.kde.kglobalaccel.Component",
        ) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let friendly_name: String = proxy.get_property("friendlyName").unwrap_or_else(|_| comp_name.clone());

        let shortcuts: Vec<String> = match proxy.call("shortcutNames", &()) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for shortcut in shortcuts {
            if shortcut.is_empty() {
                continue;
            }
            options.push(EditorActionOption {
                category: 8,
                action_type: ActionType::Kde(comp_name.clone(), shortcut.clone()),
                name: format!("{} - {}", friendly_name, shortcut),
                tooltip: format!("KDE action: {} of component {}", shortcut, comp_name),
            });
        }
    }

    options
}

pub fn fetch_gnome_action_options() -> Vec<EditorActionOption> {
    let mut options = Vec::new();

    let schemas = vec![
        "org.gnome.desktop.wm.keybindings",
        "org.gnome.settings-daemon.plugins.media-keys",
        "org.gnome.shell.keybindings",
    ];

    let source = match gio::SettingsSchemaSource::default() {
        Some(s) => s,
        None => {
            eprintln!("Warning: GSettings default schema source not found.");
            return options;
        }
    };

    for schema_id in schemas {
        let schema = match source.lookup(schema_id, true) {
            Some(s) => s,
            None => {
                eprintln!(
                    "Info: GSettings schema '{}' not found, skipping.",
                    schema_id
                );
                continue;
            }
        };

        let settings = gio::Settings::new(schema_id);

        for key in schema.list_keys() {
            let val = settings.value(&key);
            if !is_gnome_action_key(&key, val.is_type(gtk::glib::VariantTy::STRING_ARRAY)) {
                continue;
            }

            let skey = schema.key(&key);
            let mut summary = skey.summary().map(|s| s.to_string()).unwrap_or_default();
            if summary.is_empty() {
                summary = skey
                    .description()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
            }
            if summary.is_empty() {
                summary = key.to_string();
            }

            let mut accel = String::new();
            if let Some(s) = val.get::<String>() {
                accel = s;
            } else if let Some(arr) = val.get::<Vec<String>>() {
                if !arr.is_empty() {
                    accel = arr[0].clone();
                }
            }

            let tooltip = if !accel.is_empty() && accel != "disabled" {
                format!("Schema key: {} (Shortcut: {})", key, accel)
            } else {
                format!("Schema key: {} (No shortcut configured)", key)
            };

            options.push(EditorActionOption {
                category: 6, // CAT_GNOME
                action_type: ActionType::Gnome(key.to_string()),
                name: summary,
                tooltip,
            });
        }

        // Handle custom keybindings
        if schema_id == "org.gnome.settings-daemon.plugins.media-keys"
            && schema.has_key("custom-keybindings")
        {
            let paths: Vec<String> = settings.get("custom-keybindings");
            for path in paths {
                let custom = gio::Settings::with_path(
                    "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding",
                    &path,
                );
                let c_name: String = custom.get("name");
                let c_cmd: String = custom.get("command");

                if !c_name.is_empty() {
                    options.push(EditorActionOption {
                        category: 6, // CAT_GNOME
                        action_type: ActionType::Execute(c_cmd.clone()),
                        name: c_name,
                        tooltip: format!("Custom GNOME shortcut: {}", c_cmd),
                    });
                }
            }
        }
    }

    println!(
        "Fetched {} GNOME action options from GSettings.",
        options.len()
    );
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(category: usize, action_type: ActionType) -> EditorActionOption {
        EditorActionOption {
            category,
            action_type,
            name: String::new(),
            tooltip: String::new(),
        }
    }

    #[test]
    fn only_keys_clicks_and_run_command_need_a_parameter() {
        assert!(action_needs_parameter(&option(0, ActionType::Keypress(String::new()))));
        assert!(action_needs_parameter(&option(0, ActionType::Click(None))));
        assert!(action_needs_parameter(&option(7, ActionType::Execute(String::new()))));

        assert!(!action_needs_parameter(&option(6, ActionType::Gnome("close".to_string()))));
        assert!(!action_needs_parameter(&option(
            8,
            ActionType::Kde("kwin".to_string(), "Overview".to_string())
        )));
        assert!(!action_needs_parameter(&option(6, ActionType::Execute("firefox".to_string()))));
        assert!(!action_needs_parameter(&option(1, ActionType::Kill)));
        assert!(!action_needs_parameter(&option(0, ActionType::Keypress("Control+C".to_string()))));
        assert!(!action_needs_parameter(&option(7, ActionType::Abort)));
    }

    #[test]
    fn a_gesture_finds_its_standard_shortcut_before_the_generic_one() {
        let options = get_static_action_options();
        let find = |keys: &str| {
            let action = ActionType::Keypress(keys.to_string());
            options.iter().find(|opt| action_matches(&action, opt)).unwrap().name.clone()
        };
        assert_eq!(find("Control+C"), "Copy (Ctrl+C)");
        assert_eq!(find("Control+Alt+T"), "Custom Keyboard Shortcut");
    }

    #[test]
    fn standard_shortcuts_are_known_by_their_keys() {
        assert_eq!(standard_shortcut_name("Control+C").as_deref(), Some("Copy (Ctrl+C)"));
        assert_eq!(standard_shortcut_name("Control+Alt+T"), None);
    }

    #[test]
    fn shortcut_names_write_keys_as_the_guidelines_do() {
        for (keys, expected) in [
            ("Shift+Control+S", "X (Shift+Ctrl+S)"),
            ("Control+plus", "X (Ctrl++)"),
            ("Control+minus", "X (Ctrl+-)"),
            ("Control+question", "X (Ctrl+?)"),
            ("Control+comma", "X (Ctrl+,)"),
            ("Alt+Left", "X (Alt+←)"),
            ("Alt+Return", "X (Alt+Return)"),
            ("Page_Down", "X (PageDown)"),
            ("F1", "X (F1)"),
        ] {
            assert_eq!(labelled_shortcut_name("X", keys), expected);
        }
    }

    #[test]
    fn standard_shortcuts_are_distinct_and_can_be_pressed() {
        let mut seen = std::collections::HashSet::new();
        let us = gestos::keymap::LayoutKeys::compile(&gestos::keymap::LayoutName::new("us", ""));
        for (name, keys, _) in STANDARD_SHORTCUTS {
            assert!(seen.insert(*keys), "{name} repeats {keys}");
            assert!(gestos::keys::parse_key_combo(keys).is_ok(), "{keys}");
            if let Some(us) = &us {
                assert!(gestos::keys::parse_key_combo_on(keys, Some(us)).is_ok(), "{keys}");
            }
        }
    }

    #[test]
    fn only_keybinding_keys_are_gnome_actions() {
        assert!(is_gnome_action_key("close", true));
        assert!(!is_gnome_action_key("volume-step", false));
        assert!(!is_gnome_action_key("custom-keybindings", true));
    }
}
