use gtk::prelude::*;
use gtk::{gdk, glib};
use crate::ui::components::get_action_human_readable;
use gtk4 as gtk;
use std::rc::Rc;
use std::cell::RefCell;
use gestos::config::{Configuration, Gesture};
use crate::state::AppState;
use crate::actions::{
    app_action_options, fetch_gnome_action_options, fetch_kde_action_options,
    get_static_action_options,
};
use crate::suggestions::{
    arrange_suggestions, find_suggestions, gesture_category, gestures_by_category, search_matches,
    Category, Suggestion,
};
use crate::ui::components::get_action_category_icon;
use crate::ui::editor::{open_gesture_editor, open_gesture_editor_for_action};
use crate::ui::settings::open_settings_window;
use crate::ui::components::create_gesture_row;
use crate::ui::gesture_status::GestureStatus;

/// The gestures shown: in a category, those in it, and otherwise those that match the search.
pub fn get_visible_gestures(state: &AppState, category: Option<Category>) -> Vec<Gesture> {
    let filter = if category.is_some() { String::new() } else { state.search_entry.text().to_string() };

    let gestures: Vec<Gesture> = state
        .config
        .gestures
        .iter()
        .filter(|g| !g.is_deleted)
        .filter(|g| {
            category.is_none_or(|category| {
                gesture_category(g, &state.action_options) == Some(category)
            })
        })
        .filter(|g| {
            search_matches(&g.name, &filter)
                || (!g.actions.is_empty()
                    && search_matches(&get_action_human_readable(&g.actions[0]), &filter))
        })
        .cloned()
        .collect();

    gestures
}
/// What an action that needs the GNOME extension says about it, or None for any other action.
pub fn extension_note(
    action: &gestos::config::ActionType,
    status: &crate::extension::ExtensionStatus,
) -> Option<&'static str> {
    if !action.needs_gnome_extension() {
        return None;
    }
    Some(match status {
        // Named as GNOME's Extensions app lists it, so it can be found there.
        crate::extension::ExtensionStatus::Running => {
            "Uses the \u{201c}Gestos\u{201d} GNOME extension"
        }
        crate::extension::ExtensionStatus::Outdated(_) => {
            "Needs the \u{201c}Gestos\u{201d} GNOME extension to be updated"
        }
        _ => "Needs the \u{201c}Gestos\u{201d} GNOME extension",
    })
}

/// At the top of a category's page, so that it's plain which one it is: its icon, large, with its
/// name.
#[derive(Clone)]
struct PageHeading {
    area: gtk::Box,
}

impl PageHeading {
    fn new(category: Category) -> Self {
        let area = gtk::Box::new(gtk::Orientation::Vertical, 6);
        area.set_margin_top(12);
        area.set_margin_bottom(18);
        area.set_halign(gtk::Align::Center);
        let icon = gtk::Image::from_icon_name(category.icon_name());
        icon.set_pixel_size(64);
        icon.add_css_class("dim-label");
        area.append(&icon);
        let title = gtk::Label::new(Some(category.name()));
        title.add_css_class("title-1");
        title.set_wrap(true);
        title.set_justify(gtk::Justification::Center);
        area.append(&title);
        PageHeading { area }
    }
}

/// A category's page, in a window over the gestures: the gestures in it, then its actions to make
/// more, with the one to make its own of first.
#[derive(Clone)]
pub struct CategoryPage {
    window: gtk::Window,
    gestures: gtk::ListBox,
    apart: gtk::ListBox,
    actions: gtk::ListBox,
    /// What the lists show, by row, for when one is activated.
    shown: Rc<RefCell<(Vec<String>, Vec<crate::state::EditorActionOption>, Vec<Suggestion>)>>,
}

/// The window the editor and other dialogs open over: a category's, while one is open.
pub fn dialog_parent(state: &AppState) -> gtk::Window {
    match &state.category_page {
        Some(page) => page.window.clone(),
        None => state.window.clone().upcast(),
    }
}

/// Opens a category's page in a window of its own, or with None, closes it.
pub fn open_category(state_rc: &Rc<RefCell<AppState>>, category: Option<Category>) {
    let open = state_rc.borrow_mut().category_page.take();
    if let Some(page) = open {
        page.window.destroy();
    }
    state_rc.borrow_mut().open_category = category;
    let Some(category) = category else { return };

    let parent = state_rc.borrow().window.clone();
    let window = gtk::Window::new();
    window.set_transient_for(Some(&parent));
    window.set_modal(true);
    window.set_destroy_with_parent(true);
    window.set_title(Some(category.name()));
    window.set_default_size((parent.width() - 60).clamp(360, 640), (parent.height() - 60).max(400));
    // The page's heading names it, large, so the header doesn't too.
    let header = gtk::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(None)));
    window.set_titlebar(Some(&header));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&PageHeading::new(category).area);
    let list = || {
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_margin_bottom(24);
        content.append(&list);
        list
    };
    let gestures = list();
    let apart = list();
    apart.set_margin_bottom(16);
    let actions = list();
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_child(Some(&clamped(&content)));
    window.set_child(Some(&scrolled));

    let page = CategoryPage { window: window.clone(), gestures, apart, actions, shown: Default::default() };
    {
        let state_rc = Rc::downgrade(state_rc);
        let shown = Rc::clone(&page.shown);
        page.gestures.connect_row_activated(move |_, row| {
            let Some(state_rc) = state_rc.upgrade() else { return };
            let id = shown.borrow().0.get(row.index() as usize).cloned();
            let gesture = id.and_then(|id| {
                state_rc.borrow().config.gestures.iter().find(|g| g.id == id).cloned()
            });
            if let Some(gesture) = gesture {
                open_gesture_editor(&state_rc, gesture);
            }
        });
    }
    {
        let state_rc = Rc::downgrade(state_rc);
        let shown = Rc::clone(&page.shown);
        page.apart.connect_row_activated(move |_, row| {
            let option = shown.borrow().1.get(row.index() as usize).cloned();
            if let (Some(state_rc), Some(option)) = (state_rc.upgrade(), option) {
                open_gesture_editor_for_action(&state_rc, option);
            }
        });
    }
    {
        let state_rc = Rc::downgrade(state_rc);
        let shown = Rc::clone(&page.shown);
        page.actions.connect_row_activated(move |_, row| {
            let suggestion = shown.borrow().2.get(row.index() as usize).cloned();
            let Some(state_rc) = state_rc.upgrade() else { return };
            match suggestion {
                Some(Suggestion::Action(option)) => open_gesture_editor_for_action(&state_rc, option),
                Some(Suggestion::Category(category, _)) => open_category(&state_rc, Some(category)),
                None => {}
            }
        });
    }

    // Escape closes it, as other dialogs.
    let keys = gtk::ShortcutController::new();
    keys.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Escape|<Control>w"),
        Some(gtk::CallbackAction::new(|window, _| {
            if let Some(window) = window.downcast_ref::<gtk::Window>() {
                window.close();
            }
            glib::Propagation::Stop
        })),
    ));
    window.add_controller(keys);
    let state_close = Rc::downgrade(state_rc);
    window.connect_close_request(move |window| {
        if let Some(state_rc) = state_close.upgrade() {
            if let Ok(mut state) = state_rc.try_borrow_mut() {
                if state.category_page.as_ref().is_some_and(|page| &page.window == window) {
                    state.category_page = None;
                    state.open_category = None;
                }
            }
        }
        glib::Propagation::Proceed
    });

    state_rc.borrow_mut().category_page = Some(page);
    refresh_category_page(state_rc, None);
    window.present();
}

/// Shows what's in the category open, if one is. The gesture named, such as the one just saved,
/// gets the focus.
fn refresh_category_page(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) {
    let state = state_rc.borrow();
    let (Some(page), Some(category)) = (state.category_page.clone(), state.open_category) else {
        return;
    };
    for list in [&page.gestures, &page.apart, &page.actions] {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
    }
    let gestures = get_visible_gestures(&state, Some(category));
    for gesture in &gestures {
        let row = create_gesture_row(gesture, state_rc);
        page.gestures.append(&row);
        if select_name == Some(gesture.name.as_str()) {
            row.grab_focus();
        }
    }
    page.gestures.set_visible(!gestures.is_empty());

    let found = find_suggestions(&state.action_options, &state.config.gestures, "");
    let gestures_in = gestures_by_category(&state.config.gestures, &state.action_options);
    let (apart, found) = arrange_suggestions(found, false, Some(category), &gestures_in);
    for option in &apart {
        let row = suggestion_row(option, &state.extension_status, "");
        row.add_css_class("build-your-own");
        page.apart.append(&row);
    }
    page.apart.set_visible(!apart.is_empty());
    for suggestion in &found {
        let row = match suggestion {
            Suggestion::Action(option) => suggestion_row(option, &state.extension_status, ""),
            Suggestion::Category(category, count) => category_row(*category, *count),
        };
        page.actions.append(&row);
    }
    page.actions.set_visible(!found.is_empty());
    *page.shown.borrow_mut() = (gestures.into_iter().map(|g| g.id).collect(), apart, found);
}

fn category_row(category: Category, count: usize) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    row_box.append(&gtk::Image::from_icon_name(category.icon_name()));
    let name = gtk::Label::new(Some(category.name()));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    row_box.append(&name);
    // How many gestures there are in it, if any.
    let count = gtk::Label::new(Some(&count.to_string()));
    count.add_css_class("dim-label");
    count.set_visible(count.text() != "0");
    row_box.append(&count);
    row_box.append(&gtk::Image::from_icon_name("go-next-symbolic"));
    row.set_child(Some(&row_box));
    row
}

/// A banner across the top of the window, right under the header bar, while the extension running
/// is older than this Gestos, as that lasts until it's updated. It has a short title, and a button
/// for what updates it, as libadwaita's banners do from 1.3 on.
#[derive(Clone)]
pub struct ExtensionBanner {
    area: gtk::Box,
    title: gtk::Label,
    button: gtk::Button,
}

impl ExtensionBanner {
    fn new() -> Self {
        let area = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        area.add_css_class("extension-banner");
        area.set_visible(false);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_hexpand(true);
        content.set_halign(gtk::Align::Center);
        area.append(&content);
        let title = gtk::Label::new(None);
        title.set_wrap(true);
        title.set_justify(gtk::Justification::Center);
        content.append(&title);
        let button = gtk::Button::new();
        button.set_valign(gtk::Align::Center);
        content.append(&button);
        ExtensionBanner { area, title, button }
    }

    fn show(&self, status: &crate::extension::ExtensionStatus) {
        use crate::extension::{ExtensionStatus, OldCopy};
        let (title, button) = match status {
            ExtensionStatus::Outdated(OldCopy::Replaced) => {
                ("Log out to finish updating the GNOME extension", Some("_Log Out"))
            }
            ExtensionStatus::Outdated(OldCopy::InUserFolder(_)) => {
                ("An old copy of the GNOME extension is in use", Some("_Remove Old Copy"))
            }
            ExtensionStatus::Outdated(OldCopy::NotReplaced) => {
                ("The GNOME extension is older than Gestos", None)
            }
            _ => {
                self.area.set_visible(false);
                return;
            }
        };
        self.title.set_text(title);
        if let Some(label) = button {
            self.button.set_label(label);
            self.button.set_use_underline(true);
        }
        self.button.set_visible(button.is_some());
        self.area.set_visible(true);
    }
}

/// What the banner's button does, for the status shown when it's clicked.
fn update_extension(state_rc: &Rc<RefCell<AppState>>) {
    use crate::extension::{ExtensionStatus, OldCopy};
    let status = state_rc.borrow().extension_status.clone();
    match status {
        ExtensionStatus::Outdated(OldCopy::Replaced) => {
            let conn = state_rc.borrow().dbus_conn.clone();
            if let Some(Err(e)) = conn.map(|conn| crate::extension::log_out(&conn)) {
                eprintln!("Could not log out: {e}");
            }
        }
        ExtensionStatus::Outdated(OldCopy::InUserFolder(copy)) => {
            // To the trash, so that it can be put back.
            match crate::extension::trash_copy(&copy) {
                Ok(()) => refresh_extension_status(state_rc),
                Err(e) => eprintln!("Could not remove {}: {e}", copy.display()),
            }
        }
        _ => {}
    }
}

fn suggestion_row(
    option: &crate::state::EditorActionOption,
    status: &crate::extension::ExtensionStatus,
    query: &str,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);

    let icon = match crate::ui::app_picker::opened_app(&option.action_type) {
        Some(app) => crate::ui::app_picker::app_icon(app, 16),
        None => {
            let (icon_name, icon_class) = get_action_category_icon(&option.action_type);
            let icon = gtk::Image::from_icon_name(icon_name);
            icon.add_css_class(icon_class);
            icon
        }
    };
    row_box.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    // A custom action created from a search is named after it, so it offers to create that.
    let title = match &option.action_type {
        gestos::config::ActionType::Keyboard(steps) if steps.is_empty() => {
            crate::ui::editor::name_from_search(query, &option.name)
                .map(|name| format!("Create New Action: \u{201c}{name}\u{201d}"))
        }
        _ => None,
    };
    let name = gtk::Label::new(Some(title.as_deref().unwrap_or(&option.name)));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&name);
    if let Some(note) = extension_note(&option.action_type, status) {
        let subtitle = gtk::Label::new(Some(note));
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("caption");
        subtitle.set_xalign(0.0);
        subtitle.set_wrap(true);
        text.append(&subtitle);
    }
    row_box.append(&text);
    row_box.append(&gtk::Image::from_icon_name("go-next-symbolic"));

    row.set_child(Some(&row_box));
    row
}

/// Below the search results, offers actions that match the search but no gesture uses yet.
fn refresh_suggestions(state_rc: &Rc<RefCell<AppState>>, no_gestures_shown: bool) {
    let mut state = state_rc.borrow_mut();
    for list in [&state.suggestion_list, &state.apart_list] {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
    }

    let query = state.search_entry.text().to_string();
    let found = find_suggestions(&state.action_options, &state.config.gestures, &query);
    let searching = !query.trim().is_empty();
    let gestures_in = gestures_by_category(&state.config.gestures, &state.action_options);
    let (apart, found) = arrange_suggestions(found, searching, None, &gestures_in);
    for option in &apart {
        let row = suggestion_row(option, &state.extension_status, &query);
        row.add_css_class("build-your-own");
        state.apart_list.append(&row);
    }
    state.apart_list.set_visible(!apart.is_empty());
    for suggestion in &found {
        let row = match suggestion {
            Suggestion::Action(option) => suggestion_row(option, &state.extension_status, &query),
            Suggestion::Category(category, count) => category_row(*category, *count),
        };
        state.suggestion_list.append(&row);
    }
    state.suggestion_list.set_visible(!found.is_empty());
    state.visible_apart = apart.clone();

    state
        .suggestion_no_match
        .set_text(&format!("No gestures match \u{201c}{}\u{201d}.", query.trim()));
    state
        .suggestion_no_match
        .set_visible(no_gestures_shown && !query.trim().is_empty());
    let any = !found.is_empty() || !apart.is_empty();
    state.suggestion_box.set_visible(any);
    if no_gestures_shown && any {
        state.empty_state_box.set_visible(false);
    }
    state.visible_suggestions = found;
}

pub fn refresh_extension_status(state_rc: &Rc<RefCell<AppState>>) {
    let status = crate::extension::status(state_rc.borrow().dbus_conn.as_ref());
    if state_rc.borrow().extension_status != status {
        state_rc.borrow().extension_banner.show(&status);
        state_rc.borrow_mut().extension_status = status;
        refresh_gesture_list(state_rc, None);
    }
}

pub fn refresh_gesture_list(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) {
    let no_gestures_shown = show_gestures(state_rc, select_name);
    refresh_suggestions(state_rc, no_gestures_shown);
    refresh_category_page(state_rc, select_name);
}

/// Shows the gestures that match the search. The one named, such as the one just saved, gets the
/// focus, so that the keyboard carries on from it.
fn show_gestures(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) -> bool {
    let state = state_rc.borrow();

    // Clear list
    while let Some(child) = state.main_list.first_child() {
        state.main_list.remove(&child);
    }

    let visible = get_visible_gestures(&state, None);
    let mut focused = false;

    if visible.is_empty() {
        let searching = !state.search_entry.text().trim().is_empty();
        state.empty_state_box.set_title(if searching { "Nothing Found" } else { "No Gestures" });
        state.empty_state_box.set_description(searching.then_some("Try different words."));
        state.empty_state_box.set_visible(true);
        state.main_list.set_visible(false);
    } else {
        state.empty_state_box.set_visible(false);
        state.main_list.set_visible(true);

        for gesture in &visible {
            let row = create_gesture_row(gesture, state_rc);
            state.main_list.append(&row);

            if select_name == Some(gesture.name.as_str()) {
                focused = row.grab_focus();
            }
        }
    }
    // One the search hides can't take it, so it goes back to the search.
    if select_name.is_some() && !focused {
        state.search_entry.grab_focus();
    }
    visible.is_empty()
}
pub fn show_about_dialog<W: IsA<gtk::Window>>(parent: &W) {
    let dialog = adw::AboutWindow::builder()
        .transient_for(parent)
        .modal(true)
        .application_name("Gestos")
        .application_icon("gestos")
        .version(env!("GESTOS_SHOWN_VERSION"))
        .comments("Mouse gestures for Linux")
        .developer_name("Lucas Augusto Deters")
        .developers(["Lucas Augusto Deters <lucasdeters@gmail.com>"])
        .website("https://github.com/deters/gestos")
        .issue_url("https://github.com/deters/gestos/issues")
        .license_type(gtk::License::Gpl20)
        .build();
    crate::ui::components::close_with_ctrl_w(&dialog);
    dialog.present();
}

const SHORTCUTS_UI: &str = r#"<interface>
  <object class="GtkShortcutsWindow" id="shortcuts">
    <property name="modal">1</property>
    <child>
      <object class="GtkShortcutsSection">
        <property name="section-name">shortcuts</property>
        <child>
          <object class="GtkShortcutsGroup">
            <property name="title">General</property>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Search</property>
                <property name="accelerator">&lt;Control&gt;f</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Preferences</property>
                <property name="accelerator">&lt;Control&gt;comma</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Keyboard Shortcuts</property>
                <property name="accelerator">&lt;Control&gt;question</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Close Window</property>
                <property name="accelerator">&lt;Control&gt;w</property>
              </object>
            </child>
            <child>
              <object class="GtkShortcutsShortcut">
                <property name="title">Quit</property>
                <property name="accelerator">&lt;Control&gt;q</property>
              </object>
            </child>
          </object>
        </child>
      </object>
    </child>
  </object>
</interface>"#;

fn show_shortcuts_window(parent: &gtk::ApplicationWindow) {
    let builder = gtk::Builder::from_string(SHORTCUTS_UI);
    let window: gtk::ShortcutsWindow = builder.object("shortcuts").expect("shortcuts window");
    window.set_transient_for(Some(parent));
    crate::ui::components::close_with_ctrl_w(&window);
    window.present();
}

/// The window's keyboard shortcuts, as actions, the way GNOME's own apps have them. Dialogs aren't
/// part of the app, so in them Ctrl+Q does nothing, and their unsaved changes are never lost to it.
fn add_actions(app: &gtk::Application, state: &Rc<RefCell<AppState>>) {
    let window = state.borrow().window.clone();

    let search = gtk::gio::SimpleAction::new("search", None);
    let search_entry = state.borrow().search_entry.clone();
    search.connect_activate(move |_, _| {
        search_entry.grab_focus();
    });
    window.add_action(&search);

    let preferences = gtk::gio::SimpleAction::new("preferences", None);
    let state_preferences = Rc::clone(state);
    preferences.connect_activate(move |_, _| open_settings_window(&state_preferences));
    app.add_action(&preferences);

    let shortcuts = gtk::gio::SimpleAction::new("shortcuts", None);
    let shortcuts_parent = window.clone();
    shortcuts.connect_activate(move |_, _| show_shortcuts_window(&shortcuts_parent));
    app.add_action(&shortcuts);

    // Gestures keep working once Gestos is closed, as the daemon runs them.
    let quit = gtk::gio::SimpleAction::new("quit", None);
    let quit_app = app.clone();
    quit.connect_activate(move |_, _| quit_app.quit());
    app.add_action(&quit);

    app.set_accels_for_action("win.search", &["<Control>f"]);
    app.set_accels_for_action("app.preferences", &["<Control>comma"]);
    app.set_accels_for_action("app.shortcuts", &["<Control>question"]);
    app.set_accels_for_action("window.close", &["<Control>w"]);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
}

/// Holds the window's content at the width it reads best at, with room at the sides, and lets it
/// narrow along with the window.
fn clamped(child: &impl IsA<gtk::Widget>) -> adw::Clamp {
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(500);
    clamp.set_tightening_threshold(500);
    clamp.set_margin_start(24);
    clamp.set_margin_end(24);
    clamp.set_child(Some(child));
    clamp
}

/// The actions to make gestures for. Reading the desktop's action lists can be slow, so it doesn't
/// happen on the UI thread.
fn load_action_options(state: &Rc<RefCell<AppState>>) {
    let load_options = gtk::gio::spawn_blocking(|| {
        let mut options = get_static_action_options();
        options.extend(fetch_gnome_action_options());
        options.extend(fetch_kde_action_options());
        options.extend(app_action_options());
        options
    });
    let state_options = Rc::clone(state);
    glib::spawn_future_local(async move {
        if let Ok(options) = load_options.await {
            state_options.borrow_mut().action_options = options;
            refresh_gesture_list(&state_options, None);
        }
    });
}

pub fn build_ui(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some("Gestos"));
    // Wide enough for the header to keep its title centered with any status.
    window.set_default_size(700, 700);

    let header = gtk::HeaderBar::new();
    window.set_titlebar(Some(&header));

    // Preferences button
    let settings_btn = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_btn.set_tooltip_text(Some("Preferences"));
    settings_btn.update_property(&[gtk::accessible::Property::Label("Preferences")]);
    header.pack_end(&settings_btn);

    // Content VBox
    let content_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_vbox.add_css_class("main-window-content");
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&content_vbox));
    window.set_child(Some(&toasts));

    // The search stays visible, since it's also how new gestures start. Typing anywhere in the
    // window goes to it.
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_margin_top(12);
    search_entry.set_margin_bottom(12);
    search_entry.set_placeholder_text(Some("Search gestures and actions…"));
    search_entry.set_key_capture_widget(Some(&window));
    search_entry.connect_stop_search(|entry| entry.set_text(""));
    let search_clamp = clamped(&search_entry);
    content_vbox.append(&search_clamp);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    content_vbox.append(&scrolled);

    let list_container = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let main_list = gtk::ListBox::new();
    main_list.set_margin_bottom(24);
    main_list.add_css_class("boxed-list");
    main_list.set_selection_mode(gtk::SelectionMode::None);
    list_container.append(&main_list);

    let empty_state_box = adw::StatusPage::new();
    empty_state_box.set_icon_name(Some("edit-find-symbolic"));
    empty_state_box.set_vexpand(true);
    list_container.append(&empty_state_box);

    let suggestion_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    suggestion_box.set_margin_bottom(24);
    suggestion_box.set_visible(false);

    let suggestion_no_match = gtk::Label::new(None);
    suggestion_no_match.set_xalign(0.0);
    suggestion_no_match.set_margin_top(24);
    suggestion_no_match.set_margin_bottom(8);
    suggestion_box.append(&suggestion_no_match);

    let suggestion_heading = gtk::Label::new(Some("Create a Gesture For"));
    suggestion_heading.add_css_class("section-header");
    suggestion_heading.set_xalign(0.0);
    suggestion_box.append(&suggestion_heading);

    let apart_list = gtk::ListBox::new();
    apart_list.add_css_class("boxed-list");
    apart_list.set_selection_mode(gtk::SelectionMode::None);
    apart_list.set_margin_bottom(16);
    suggestion_box.append(&apart_list);

    let suggestion_list = gtk::ListBox::new();
    suggestion_list.add_css_class("boxed-list");
    suggestion_list.set_selection_mode(gtk::SelectionMode::None);
    suggestion_box.append(&suggestion_list);

    list_container.append(&suggestion_box);
    scrolled.set_child(Some(&clamped(&list_container)));

    if let Err(e) = gestos::config::initialize_user_config_if_missing() {
        eprintln!("Warning: Failed to initialize configuration: {}", e);
    }
    let config = Configuration::load_from_defaults();
    let dbus_conn = zbus::blocking::Connection::session().ok();

    let gesture_status = GestureStatus::new("Gestures", dbus_conn.clone(), &toasts);
    let extension_status = crate::extension::status(dbus_conn.as_ref());
    header.set_title_widget(Some(gesture_status.title()));
    header.pack_end(gesture_status.action_button());
    content_vbox.insert_child_after(&clamped(gesture_status.error_area()), Some(&search_clamp));
    let extension_banner = ExtensionBanner::new();
    extension_banner.show(&extension_status);
    content_vbox.prepend(&extension_banner.area);

    let state = Rc::new(RefCell::new(AppState {
        toasts,
        config,
        main_list,
        search_entry,
        window,
        newly_added_gestures: Vec::new(),
        dbus_conn,
        empty_state_box,
        action_options: Vec::new(),
        visible_suggestions: Vec::new(),
        apart_list,
        visible_apart: Vec::new(),
        suggestion_box,
        suggestion_no_match,
        suggestion_list,
        extension_status,
        extension_banner: extension_banner.clone(),
        open_category: None,
        category_page: None,
    }));


    let state_banner = Rc::clone(&state);
    extension_banner.button.connect_clicked(move |_| update_extension(&state_banner));

    // The extension can be turned on, off, or installed elsewhere, so its status is read again
    // whenever the window comes back to the front.
    let state_active = Rc::clone(&state);
    state.borrow().window.connect_is_active_notify(move |window| {
        if window.is_active() {
            refresh_extension_status(&state_active);
        }
    });

    // Refresh initially
    refresh_gesture_list(&state, None);

    // Connect search entry
    let state_clone = Rc::clone(&state);
    state
        .borrow()
        .search_entry
        .connect_search_changed(move |_| {
            refresh_gesture_list(&state_clone, None);
        });

    let state_suggestion = Rc::clone(&state);
    state
        .borrow()
        .suggestion_list
        .connect_row_activated(move |_, row| {
            let suggestion = state_suggestion
                .borrow()
                .visible_suggestions
                .get(row.index() as usize)
                .cloned();
            match suggestion {
                Some(Suggestion::Action(option)) => {
                    open_gesture_editor_for_action(&state_suggestion, option)
                }
                Some(Suggestion::Category(category, _)) => {
                    open_category(&state_suggestion, Some(category))
                }
                None => {}
            }
        });

    let state_build = Rc::clone(&state);
    state.borrow().apart_list.connect_row_activated(move |_, row| {
        let option = state_build.borrow().visible_apart.get(row.index() as usize).cloned();
        if let Some(option) = option {
            open_gesture_editor_for_action(&state_build, option);
        }
    });

    load_action_options(&state);
    // Apps installed or removed while Gestos is open come and go from the list too.
    let apps = gtk::gio::AppInfoMonitor::get();
    let state_apps = Rc::downgrade(&state);
    apps.connect_changed(move |_| {
        if let Some(state) = state_apps.upgrade() {
            load_action_options(&state);
        }
    });
    // The monitor only says what changed for as long as it's kept.
    state.borrow().window.connect_destroy(move |_| {
        let _ = &apps;
    });


    // Connect row activation for editing
    let state_clone3 = Rc::clone(&state);
    state
        .borrow()
        .main_list
        .connect_row_activated(move |_, row| {
            let idx = row.index();
            let state_borrow = state_clone3.borrow();

            let visible_gestures = get_visible_gestures(&state_borrow, None);

            if idx >= 0 && (idx as usize) < visible_gestures.len() {
                let gesture = visible_gestures[idx as usize].clone();
                drop(state_borrow);
                open_gesture_editor(&state_clone3, gesture);
            }
        });

    settings_btn.set_action_name(Some("app.preferences"));
    add_actions(app, &state);

    // Stylesheet injection
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "headerbar { background: @window_bg_color; border: none; box-shadow: none; }\n\
         .main-window-content, .dialog-content { background-color: @window_bg_color; }\n\
         .gesture-preview-frame { background: @window_bg_color; border-radius: 6px; }\n\
         searchentry, entry.search { border-radius: 18px; }\n\
         .context-title { font-size: 1.5em; font-weight: bold; }\n\
         .gesture-row { padding: 6px; }\n\
         .icon-holder { padding: 4px; }\n\
         .icon-swiss-red { color: @red_3; }\n\
         .icon-purple { color: @purple_2; }\n\
         .action-label { font-size: 0.9em; opacity: 0.7; }\n\
         .settings-row { padding: 8px; }\n\
         .section-header { font-weight: bold; margin-top: 8px; margin-bottom: 4px; }\n\
         .status-label { font-weight: bold; }\n\
         .warning-box { padding: 8px; background-color: alpha(@warning_color, 0.15); border: 1px solid alpha(@warning_color, 0.3); border-radius: 6px; }\n\
         .warning-label { color: @warning_color; font-size: 0.95em; }\n\
         .extension-banner { padding: 6px 12px; min-height: 36px; background-color: alpha(@warning_color, 0.15); }\n\
         .keycap { padding: 6px 12px; background-color: alpha(currentColor, 0.08); border: 1px solid alpha(currentColor, 0.15); border-radius: 6px; font-weight: bold; font-size: 1.1em; }\n\
         .drag-handle { opacity: 0.55; cursor: grab; margin-right: 4px; }\n\
         .drag-handle:hover { opacity: 1; }\n\
         .drag-handle:active { cursor: grabbing; }\n\
         .build-your-own label { font-weight: bold; }\n\
         togglebutton.key-held, button.key-held { box-shadow: 0 0 0 3px alpha(@accent_bg_color, 0.6); }\n\
         .dialog-content scrolledwindow > undershoot.bottom { box-shadow: inset 0 -12px 12px -12px alpha(black, 0.25); }\n\
         .dialog-content scrolledwindow > undershoot.top { box-shadow: inset 0 12px 12px -12px alpha(black, 0.25); }\n"
    );

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    state.borrow().window.present();
    state.borrow().search_entry.grab_focus();
}
