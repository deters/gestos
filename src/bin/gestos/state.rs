use crate::extension::ExtensionStatus;
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

pub struct AppState {
    /// Short-lived notices, such as one to undo a deletion.
    pub toasts: adw::ToastOverlay,
    pub config: Configuration,
    pub main_list: gtk::ListBox,
    /// The gestures by the app they're for, each under its heading, in place of the list when
    /// they're for more than one.
    pub gesture_groups: gtk::Box,
    pub search_entry: gtk::SearchEntry,
    pub window: gtk::ApplicationWindow,
    pub newly_added_gestures: Vec<String>,
    /// The gesture to show once the search is cleared, as one just made from a search.
    pub show_after_search: Option<String>,
    pub dbus_conn: Option<zbus::blocking::Connection>,
    pub empty_state_box: adw::StatusPage,
    pub action_options: Vec<EditorActionOption>,
    /// Action templates offered only for some apps, on their pages.
    pub app_templates: Vec<crate::action_templates::ActionTemplate>,
    pub visible_suggestions: Vec<crate::suggestions::Suggestion>,
    /// What a category offers apart, before its own: the one to make its own of.
    pub apart_list: gtk::ListBox,
    pub visible_apart: Vec<EditorActionOption>,
    pub suggestion_box: gtk::Box,
    pub suggestion_no_match: gtk::Label,
    pub suggestion_heading: gtk::Label,
    pub suggestion_list: gtk::ListBox,
    /// Whether the GNOME extension can run the actions that need it.
    pub extension_status: ExtensionStatus,
    /// Says when the extension running is older than this Gestos, and what updates it.
    pub extension_banner: crate::ui::main_window::ExtensionBanner,
    /// The category of actions open, in a window of its own over the gestures.
    pub open_category: Option<crate::suggestions::Category>,
    pub category_page: Option<crate::ui::main_window::CategoryPage>,
    /// An app's own shortcuts, in a window over its category's, while open.
    pub app_page: Option<crate::ui::main_window::AppPage>,
}
