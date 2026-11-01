use gestos::config::{ActionType, Configuration};
use gtk4 as gtk;
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct EditorActionOption {
    pub category: usize,
    pub action_type: ActionType,
    pub name: String,
    pub tooltip: String,
}

pub const CATEGORY_NAMES: &[&str] = &[
    "Input Emulation",
    "Window Management",
    "Workspaces & Overview",
    "Media & Audio",
    "System & Settings",
    "Applications",
    "GNOME Actions (Native)",
    "Other/Internal",
    "KDE Actions (Native)",
];

pub struct AppState {
    pub config: Configuration,
    pub main_list: gtk::ListBox,
    pub search_entry: gtk::SearchEntry,
    pub window: gtk::ApplicationWindow,
    pub newly_added_gestures: Vec<String>,
    pub dbus_conn: Option<zbus::blocking::Connection>,
    pub empty_state_box: gtk::Box,
    pub action_options: Vec<EditorActionOption>,
    pub visible_suggestions: Vec<EditorActionOption>,
    pub show_all_suggestions: bool,
    pub suggestion_box: gtk::Box,
    pub suggestion_no_match: gtk::Label,
    pub suggestion_list: gtk::ListBox,
    pub suggestion_show_all: gtk::Button,
}
