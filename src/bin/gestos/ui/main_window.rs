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
    action_category, arrange_suggestions, find_suggestions, gesture_category, gestures_by_category, search_matches,
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
        let icon = category.icon();
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

/// A category's page, in place of the main page: the gestures in it, then its actions to make
/// more, with the one to make its own of first.
#[derive(Clone)]
pub struct CategoryPage {
    page: gtk::Widget,
    /// Those to make one's own of, like a custom shortcut, with the gestures made with them, apart,
    /// then the rest.
    apart: gtk::ListBox,
    actions: gtk::ListBox,
    /// What the lists show, by row, for when one is activated.
    shown: Rc<RefCell<(Vec<Entry>, Vec<Entry>)>>,
}

/// The window the editor and other dialogs open over: the main window, whichever page it shows.
pub fn dialog_parent(state: &AppState) -> gtk::Window {
    state.window.clone().upcast()
}

/// Shows a category's or an app's page in place of the one shown, with the back button to go
/// back to it. Typing goes to the page then, rather than to the main page's search.
fn show_page(state_rc: &Rc<RefCell<AppState>>, page: &gtk::Widget) {
    let state = state_rc.borrow();
    state.pages.add_child(page);
    state.pages.set_visible_child(page);
    state.back_button.set_visible(true);
    state.search_entry.set_key_capture_widget(None::<&gtk::Widget>);
    drop(state);
    // The keyboard goes on in the page, not in the one it replaced.
    page.child_focus(gtk::DirectionType::TabForward);
    drop_hidden_pages(state_rc);
}

/// Goes back a page: from an app's to the category's it was opened from, or else to the main
/// page, and from a category's to the main page. Whether there was a page to go back from.
pub fn go_back(state_rc: &Rc<RefCell<AppState>>) -> bool {
    let mut state = state_rc.borrow_mut();
    let target = if state.app_page.take().is_some() {
        state.category_page.as_ref().map(|page| page.page.clone())
    } else if state.category_page.take().is_some() {
        state.open_category = None;
        None
    } else {
        return false;
    };
    match target {
        Some(page) => {
            state.pages.set_visible_child(&page);
            page.child_focus(gtk::DirectionType::TabForward);
        }
        None => {
            state.pages.set_visible_child_name(MAIN_PAGE);
            state.back_button.set_visible(false);
            state.search_entry.set_key_capture_widget(Some(&state.window));
            state.search_entry.grab_focus();
        }
    }
    drop(state);
    drop_hidden_pages(state_rc);
    true
}

/// The main page's name among the pages.
const MAIN_PAGE: &str = "main";

/// Lets go of the pages gone back from, or replaced, once they've slid out of sight.
fn drop_hidden_pages(state_rc: &Rc<RefCell<AppState>>) {
    let Ok(state) = state_rc.try_borrow() else { return };
    if state.pages.is_transition_running() {
        return;
    }
    let kept: Vec<gtk::Widget> = [
        state.pages.child_by_name(MAIN_PAGE),
        state.category_page.as_ref().map(|page| page.page.clone()),
        state.app_page.as_ref().map(|page| page.page.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut child = state.pages.first_child();
    while let Some(page) = child {
        child = page.next_sibling();
        if !kept.contains(&page) {
            state.pages.remove(&page);
        }
    }
}

/// A keyboard shortcut of one's own for an app, named for it, as it's for that app alone, and so
/// are the gestures named after it.
fn shortcut_on_app(state: &AppState, app: &str) -> Option<crate::state::EditorActionOption> {
    let option = state
        .action_options
        .iter()
        .find(|option| crate::suggestions::is_custom_shortcut(option))?;
    Some(crate::state::EditorActionOption {
        name: format!("Custom Keyboard Shortcut on {}", crate::ui::app_picker::app_name(app)),
        ..option.clone()
    })
}

/// An app's own page: each action inside it, each with its gesture's drawing if it has one. It's
/// made again each time it's shown, as an action's row becomes its gesture's.
#[derive(Clone)]
pub struct AppPage {
    page: gtk::Widget,
    content: gtk::Box,
    app: String,
}

/// The gestures made inside an app, which work only there, and maybe in others too.
fn gestures_inside<'a>(state: &'a AppState, app: &'a str) -> impl Iterator<Item = &'a Gesture> {
    state.config.gestures.iter().filter(move |g| !g.is_deleted && g.apps.iter().any(|its| its == app))
}

/// Opens an app's own page in place of its category's.
fn open_app_page(state_rc: &Rc<RefCell<AppState>>, app: &str) {
    // One app's at a time, in place of its category's, never of another app's.
    state_rc.borrow_mut().app_page = None;
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_child(Some(&clamped(&content)));
    state_rc.borrow_mut().app_page =
        Some(AppPage { page: scrolled.clone().upcast(), content, app: app.to_string() });
    refresh_app_page(state_rc, None);
    show_page(state_rc, &scrolled.upcast());
}

/// A list of rows on an app's page, each opening what it shows.
fn entry_list(state_rc: &Rc<RefCell<AppState>>, entries: Vec<Entry>, rows: Vec<gtk::ListBoxRow>) -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_margin_bottom(24);
    for row in &rows {
        list.append(row);
    }
    let state_open = Rc::downgrade(state_rc);
    list.connect_row_activated(move |_, row| {
        if let (Some(state_rc), Some(entry)) = (state_open.upgrade(), entries.get(row.index() as usize)) {
            open_entry(&state_rc, entry.clone());
        }
    });
    list
}

/// Makes the app's page open, if one is, again: what can be done inside it, its templates, the shortcuts its Keyboard Shortcuts window lists, under their groups, and those of
/// one's own, each action with its gesture's drawing if it has one. The gesture named, such as the
/// one just saved, gets the focus.
fn refresh_app_page(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) {
    let state = state_rc.borrow();
    let Some(page) = state.app_page.clone() else { return };
    let app = page.app.as_str();
    let name = crate::ui::app_picker::app_name(app);
    while let Some(child) = page.content.first_child() {
        page.content.remove(&child);
    }

    let heading = gtk::Box::new(gtk::Orientation::Vertical, 6);
    heading.set_margin_top(12);
    heading.set_margin_bottom(18);
    heading.set_halign(gtk::Align::Center);
    heading.append(&crate::ui::app_picker::app_icon(app, 64));
    let title = gtk::Label::new(Some(&name));
    title.add_css_class("title-1");
    title.set_wrap(true);
    title.set_justify(gtk::Justification::Center);
    heading.append(&title);
    page.content.append(&heading);
    let list_of = |row: &gtk::ListBoxRow| {
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        list.append(row);
        list
    };

    // While gestures are paused in the app, an exception, a notice says so, with a way to resume
    // them, and nothing else is shown. Where the desktop says when the app in use changes, the
    // button is left to the app before it's pressed. Elsewhere, only the gesture drawn isn't run.
    if state.config.gestures_paused.iter().any(|other| other == app) {
        let what_it_does = if gestos::active_app::is_x11_session() {
            format!("{name} gets the mouse button you draw with, as it would without Gestos.")
        } else {
            "Gestures you draw in it don't run.".to_string()
        };
        let resume = gtk::Button::with_mnemonic("_Resume");
        let state_resume = Rc::downgrade(state_rc);
        let app_resume = app.to_string();
        resume.connect_clicked(move |_| {
            if let Some(state_rc) = state_resume.upgrade() {
                set_gestures_paused(&state_rc, &app_resume, false);
            }
        });
        let notice = list_of(&titled_row(&format!("Gestures are automatically paused in {name}"), &what_it_does, &resume));
        page.content.append(&notice);
        return;
    }

    // A list's rows and what each opens, made together.
    struct Rows(Vec<Entry>, Vec<gtk::ListBoxRow>);
    let gesture_rows = |rows: &mut Rows, gestures: &[Gesture]| {
        for gesture in gestures {
            let row = create_gesture_row(gesture, state_rc, false);
            if select_name == Some(gesture.name.as_str()) {
                let row = row.clone();
                glib::idle_add_local_once(move || {
                    row.grab_focus();
                    scroll_to(&row);
                });
            }
            rows.1.push(row);
            rows.0.push(Entry::Gesture(gesture.id.clone()));
        }
    };

    let mut inside: Vec<Gesture> = gestures_inside(&state, app).cloned().collect();

    let only_here = vec![app.to_string()];
    // First, the action templates for this app, by its ID or its kind, like a file manager's, as
    // they do more than one of its shortcuts.
    let categories = gtk::gio::DesktopAppInfo::new(&format!("{}.desktop", app.trim_end_matches(".desktop")))
        .and_then(|info| info.categories().map(|categories| categories.to_string()))
        .unwrap_or_default();
    let templates: Vec<_> = state
        .app_templates
        .iter()
        .filter(|template| template.is_for(app, &categories))
        .map(crate::action_templates::option)
        .collect();
    let mut rows = Rows(Vec::new(), Vec::new());
    for option in &templates {
        let its = take_gestures(&mut inside, |g| {
            g.actions.first().is_some_and(|action| crate::suggestions::is_action_of(action, option))
        });
        if its.is_empty() {
            rows.1.push(suggestion_row(option, &state.extension_status, ""));
            rows.0.push(Entry::ActionIn(option.clone(), only_here.clone()));
        }
        gesture_rows(&mut rows, &its);
    }
    if !rows.0.is_empty() {
        page.content.append(&entry_list(state_rc, rows.0, rows.1));
    }

    // The app's own shortcuts, each group of them under its name. The names are the app's, so
    // they're smaller and dimmer than Gestos's own headings.
    let shortcuts = gestos::app_shortcuts::shortcuts_of(app);
    let mut sections: Vec<(&str, Vec<&gestos::app_shortcuts::AppShortcut>)> = Vec::new();
    for shortcut in shortcuts.iter() {
        match sections.iter_mut().find(|(section, _)| *section == shortcut.section) {
            Some((_, in_it)) => in_it.push(shortcut),
            None => sections.push((&shortcut.section, vec![shortcut])),
        }
    }
    for (section, in_it) in sections {
        if !section.is_empty() {
            let label = gtk::Label::new(Some(section));
            label.add_css_class("caption-heading");
            label.add_css_class("dim-label");
            label.set_xalign(0.0);
            label.set_margin_bottom(6);
            page.content.append(&label);
        }
        let mut rows = Rows(Vec::new(), Vec::new());
        for shortcut in in_it {
            let its = take_gestures(&mut inside, |g| {
                matches!(g.actions.first(),
                    Some(gestos::config::ActionType::Keypress(keys)) if crate::actions::same_keys(keys, &shortcut.keys))
            });
            if its.is_empty() {
                rows.1.push(app_shortcut_row(shortcut));
                rows.0.push(Entry::AppShortcut(app.to_string(), shortcut.keys.clone()));
            }
            gesture_rows(&mut rows, &its);
        }
        page.content.append(&entry_list(state_rc, rows.0, rows.1));
    }

    // Last, for a shortcut it doesn't list and for an advanced action, each named for the app it's
    // made in, then the gestures made with them, and any other made for the app.
    let advanced = state
        .action_options
        .iter()
        .find(|option| {
            matches!(&option.action_type, gestos::config::ActionType::Keyboard(steps) if steps.is_empty())
        })
        .cloned();
    let custom_shortcut = shortcut_on_app(&state, app).map(|option| (option.clone(), option));
    let advanced = advanced.map(|option| {
        let shown = crate::state::EditorActionOption { name: format!("{} on {name}", option.name), ..option.clone() };
        (shown, option)
    });
    let mut rows = Rows(Vec::new(), Vec::new());
    for (shown, option) in [custom_shortcut, advanced].into_iter().flatten() {
        let row = suggestion_row(&shown, &state.extension_status, "");
        row.add_css_class("build-your-own");
        rows.1.push(row);
        rows.0.push(Entry::ActionIn(option, only_here.clone()));
    }
    gesture_rows(&mut rows, &inside);
    if !rows.0.is_empty() {
        page.content.append(&entry_list(state_rc, rows.0, rows.1));
    }

    // Last, as pausing gestures in an app is an exception, for apps that use the button
    // themselves, like games.
    let switch = gtk::Switch::new();
    switch.update_property(&[gtk::accessible::Property::Label(&format!("Pause Gestures in {name}"))]);
    let state_switch = Rc::downgrade(state_rc);
    let app_switch = app.to_string();
    switch.connect_state_set(move |_, on| {
        if let Some(state_rc) = state_switch.upgrade() {
            set_gestures_paused(&state_rc, &app_switch, on);
        }
        glib::Propagation::Proceed
    });
    let pause = list_of(&titled_row(
        "Pause Gestures",
        "For apps that use the mouse button you draw with themselves, such as games",
        &switch,
    ));
    pause.set_margin_bottom(24);
    page.content.append(&pause);
}

/// An app, which opens its own page, with how many gestures are made for it, if any.
fn app_row(state: &AppState, app: &str) -> gtk::ListBoxRow {
    app_row_named(state, app, &crate::ui::app_picker::app_name(app))
}

/// An app's row under a name of its own, where the page it's on doesn't say what it leads to, with
/// how many gestures are made inside it, or that they're paused there.
fn app_row_named(state: &AppState, app: &str, title: &str) -> gtk::ListBoxRow {
    let count = gestures_inside(state, app).count();
    let paused = state.config.gestures_paused.iter().any(|other| other == app);
    let row = gtk::ListBoxRow::new();
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    row_box.append(&crate::ui::app_picker::app_icon(app, 16));
    let name = gtk::Label::new(Some(title));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row_box.append(&name);
    // Where gestures are paused, that's what matters about it, rather than how many it has.
    let count_label = gtk::Label::new(Some(&if paused { "Paused".to_string() } else { count.to_string() }));
    count_label.add_css_class("dim-label");
    count_label.set_visible(paused || count > 0);
    row_box.append(&count_label);
    row_box.append(&gtk::Image::from_icon_name("go-next-symbolic"));
    row.set_child(Some(&row_box));
    row
}

/// The apps Gestos tells by their windows that are listed among the apps: those turned on, and
/// those with gestures made inside them, so that turning one off doesn't hide its gestures.
fn apps_told_by_windows(state: &AppState) -> Vec<gestos::programs::Program> {
    gestos::programs::list()
        .into_iter()
        .filter(|listed| listed.on || gestures_inside(state, &listed.program.app_id()).count() > 0)
        .map(|listed| listed.program)
        .collect()
}

/// Every app that gestures can be made inside, as gestures name it, with its name: the installed
/// ones and those Gestos tells by their windows, by name.
fn apps_to_be_inside(state: &AppState) -> Vec<(String, String)> {
    let mut apps: Vec<(String, String)> = state
        .action_options
        .iter()
        .filter_map(|option| crate::ui::app_picker::opened_app(&option.action_type))
        .map(|app| (app.to_string(), crate::ui::app_picker::app_name(app)))
        .chain(apps_told_by_windows(state).into_iter().map(|program| (program.app_id(), program.name)))
        .collect();
    apps.sort_by_cached_key(|(_, name)| name.to_lowercase());
    apps
}

/// Pauses or resumes gestures in an app, and has the daemon and the pages follow. If it can't be
/// saved, it's said why, and nothing changes.
fn set_gestures_paused(state_rc: &Rc<RefCell<AppState>>, app: &str, paused: bool) {
    // Busy only while a page is refreshed to show what's saved, which already is this.
    let Ok(mut state) = state_rc.try_borrow_mut() else { return };
    if state.config.gestures_paused.iter().any(|other| other == app) == paused {
        return;
    }
    let before = state.config.gestures_paused.clone();
    state.config.gestures_paused.retain(|other| other != app);
    if paused {
        state.config.gestures_paused.push(app.to_string());
    }
    let parent = dialog_parent(&state);
    if let Err(e) = state.config.save_to_file() {
        state.config.gestures_paused = before;
        drop(state);
        crate::ui::components::show_error_dialog(&parent, &format!("Couldn't save the change: {e}"));
        refresh_gesture_list(state_rc, None);
        return;
    }
    let conn = state.dbus_conn.clone();
    drop(state);
    crate::daemon::reload_daemon(conn.as_ref(), Some(&parent));
    refresh_gesture_list(state_rc, None);
}

/// A row with a title and a smaller, dimmer line under it, and a widget at its end.
fn titled_row(title: &str, subtitle: &str, suffix: &impl IsA<gtk::Widget>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_hexpand(true);
    texts.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some(title));
    title.set_xalign(0.0);
    title.set_wrap(true);
    texts.append(&title);
    let subtitle = gtk::Label::new(Some(subtitle));
    subtitle.set_xalign(0.0);
    subtitle.set_wrap(true);
    subtitle.add_css_class("dim-label");
    subtitle.add_css_class("caption");
    texts.append(&subtitle);
    row_box.append(&texts);
    suffix.set_valign(gtk::Align::Center);
    row_box.append(suffix);
    row.set_child(Some(&row_box));
    row
}

/// One of an app's shortcuts: its name, and its keys as shortcuts' names write them. Without an
/// arrow, as it opens the editor, not another page.
fn app_shortcut_row(shortcut: &gestos::app_shortcuts::AppShortcut) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    let title = gtk::Label::new(Some(&opens_editor(&shortcut.title)));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_wrap(true);
    row_box.append(&title);
    let keys = gtk::Label::new(Some(&shortcut.keys.replace("Control", "Ctrl")));
    keys.add_css_class("dim-label");
    row_box.append(&keys);
    row.set_child(Some(&row_box));
    row
}

/// Opens a category's page in place of the main page, or with None, goes back to the main page.
pub fn open_category(state_rc: &Rc<RefCell<AppState>>, category: Option<Category>) {
    let Some(category) = category else {
        while go_back(state_rc) {}
        return;
    };
    {
        let mut state = state_rc.borrow_mut();
        state.app_page = None;
        state.category_page = None;
        state.open_category = Some(category);
    }

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
    let apart = list();
    apart.set_margin_bottom(16);
    let actions = list();
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_child(Some(&clamped(&content)));

    let page = CategoryPage {
        page: scrolled.clone().upcast(),
        apart,
        actions,
        shown: Default::default(),
    };
    for (index, list) in [(0, &page.apart), (1, &page.actions)] {
        let state_rc = Rc::downgrade(state_rc);
        let shown = Rc::clone(&page.shown);
        list.connect_row_activated(move |_, row| {
            let shown = shown.borrow();
            let entries = if index == 0 { &shown.0 } else { &shown.1 };
            let entry = entries.get(row.index() as usize).cloned();
            drop(shown);
            if let (Some(state_rc), Some(entry)) = (state_rc.upgrade(), entry) {
                open_entry(&state_rc, entry);
            }
        });
    }

    state_rc.borrow_mut().category_page = Some(page);
    refresh_category_page(state_rc, None);
    show_page(state_rc, &scrolled.upcast());
}

/// What a row in a list of actions is: a gesture, which is an action with its drawing, an action
/// without one yet, or a page of them.
#[derive(Clone)]
pub enum Entry {
    Gesture(String),
    Action(crate::state::EditorActionOption),
    /// An action made to work only in some apps.
    ActionIn(crate::state::EditorActionOption, Vec<String>),
    /// One of an app's own shortcuts, by its keys.
    AppShortcut(String, String),
    /// An app's page, of what's done inside it.
    AppPage(String),
    Category(Category),
}

/// Opens what a row is: a gesture in its editor, an action in a new gesture's, after it's drawn,
/// or a page: an app's, or a category's.
fn open_entry(state_rc: &Rc<RefCell<AppState>>, entry: Entry) {
    match entry {
        Entry::Gesture(id) => {
            let gesture = state_rc.borrow().config.gestures.iter().find(|g| g.id == id).cloned();
            if let Some(gesture) = gesture {
                open_gesture_editor(state_rc, gesture);
            }
        }
        Entry::Action(option) => open_gesture_editor_for_action(state_rc, option),
        Entry::AppPage(app) => open_app_page(state_rc, &app),
        Entry::ActionIn(option, apps) => crate::ui::editor::open_gesture_editor_in_apps(state_rc, option, &apps),
        Entry::AppShortcut(app, keys) => crate::ui::editor::open_gesture_editor_for_app_shortcut(state_rc, &app, &keys),
        Entry::Category(category) => open_category(state_rc, Some(category)),
    }
}

/// Adds a gesture's row to a list, and to what the list shows, giving it the focus if it's the one
/// named.
fn add_gesture_row(
    state_rc: &Rc<RefCell<AppState>>,
    list: &gtk::ListBox,
    entries: &mut Vec<Entry>,
    gesture: &Gesture,
    select_name: Option<&str>,
) {
    let row = create_gesture_row(gesture, state_rc, false);
    list.append(&row);
    entries.push(Entry::Gesture(gesture.id.clone()));
    if select_name == Some(gesture.name.as_str()) {
        row.grab_focus();
        scroll_to(&row);
    }
}

/// Takes the gestures out of `left` that `is_its` says are an action's, in their order.
fn take_gestures(left: &mut Vec<Gesture>, is_its: impl Fn(&Gesture) -> bool) -> Vec<Gesture> {
    let (taken, kept) = std::mem::take(left).into_iter().partition(|g| is_its(g));
    *left = kept;
    taken
}

/// Shows what's in the category open, if one is: each action, with its gesture's drawing if it has
/// one, in the order they're offered. Those of one's own, like a command, follow the action they're
/// made with. The gesture named, such as the one just saved, gets the focus.
fn refresh_category_page(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) {
    let state = state_rc.borrow();
    let (Some(page), Some(category)) = (state.category_page.clone(), state.open_category) else {
        return;
    };
    for list in [&page.apart, &page.actions] {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
    }
    let (mut apart_shown, mut actions_shown) = (Vec::new(), Vec::new());

    // Every action, those with a gesture too, as a gesture shows on its action's row.
    let found = find_suggestions(&state.action_options, &[], "");
    let gestures_in = gestures_by_category(&state.config.gestures, &state.action_options);
    let (apart, found) = arrange_suggestions(found, false, Some(category), &gestures_in);

    if category == Category::InsideApps {
        // Every app, those Gestos tells by their windows among them, as each opens its own page,
        // by name, in one list, with how many gestures are made inside it.
        for (app, _) in apps_to_be_inside(&state) {
            page.actions.append(&app_row(&state, &app));
            actions_shown.push(Entry::AppPage(app));
        }
    } else {
        let mut left = get_visible_gestures(&state, Some(category));
        for option in &apart {
            let row = suggestion_row(option, &state.extension_status, "");
            row.add_css_class("build-your-own");
            page.apart.append(&row);
            apart_shown.push(Entry::Action(option.clone()));
            let own = take_gestures(&mut left, |g| {
                g.actions.first().is_some_and(|action| crate::actions::action_matches(action, option))
            });
            for gesture in &own {
                add_gesture_row(state_rc, &page.apart, &mut apart_shown, gesture, select_name);
            }
        }
        // The apps' own shortcuts are on their pages, which the Apps page leads to, after the
        // custom shortcut, as both are for shortcuts this page doesn't list.
        if category == Category::KeyboardShortcuts {
            let row = category_row_named(Category::InsideApps, "Shortcuts in Apps", 0);
            row.add_css_class("build-your-own");
            page.apart.append(&row);
            apart_shown.push(Entry::Category(Category::InsideApps));
        }
        for suggestion in &found {
            match suggestion {
                Suggestion::Action(option) => {
                    let its = take_gestures(&mut left, |g| {
                        g.actions.first().is_some_and(|action| crate::suggestions::is_action_of(action, option))
                    });
                    if its.is_empty() {
                        page.actions.append(&suggestion_row(option, &state.extension_status, ""));
                        actions_shown.push(Entry::Action(option.clone()));
                    }
                    for gesture in &its {
                        add_gesture_row(state_rc, &page.actions, &mut actions_shown, gesture, select_name);
                    }
                }
                Suggestion::Category(category, count) => {
                    page.actions.append(&category_row(*category, *count));
                    actions_shown.push(Entry::Category(*category));
                }
                Suggestion::App(app) => {
                    page.actions.append(&app_row(&state, app));
                    actions_shown.push(Entry::AppPage(app.clone()));
                }
            }
        }
        // Any whose action isn't offered here, last.
        for gesture in &left {
            add_gesture_row(state_rc, &page.actions, &mut actions_shown, gesture, select_name);
        }
    }
    page.apart.set_visible(!apart_shown.is_empty());
    page.actions.set_visible(!actions_shown.is_empty());
    *page.shown.borrow_mut() = (apart_shown, actions_shown);
}

/// A category, with how many gestures there are in it, if any, which opens its page.
fn category_row(category: Category, count: usize) -> gtk::ListBoxRow {
    category_row_named(category, category.name(), count)
}

/// A category's row under a name of its own, where the page it's on says what it leads to.
fn category_row_named(category: Category, title: &str, count: usize) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    row_box.append(&category.icon());
    let name = gtk::Label::new(Some(title));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    row_box.append(&name);
    let count_label = gtk::Label::new(Some(&count.to_string()));
    count_label.add_css_class("dim-label");
    count_label.set_visible(count > 0);
    row_box.append(&count_label);
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

/// A row's label for opening the editor, which asks for the gesture before anything is made: with an
/// ellipsis, as GNOME's labels have for asking more first.
fn opens_editor(label: &str) -> String {
    if label.ends_with('…') {
        label.to_string()
    } else {
        format!("{label}…")
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
            let icon = gtk::Image::from_icon_name(&icon_name);
            icon.add_css_class(icon_class);
            icon
        }
    };
    row_box.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    // An advanced action or a keyboard shortcut created from a search is named after it, so they
    // offer to create that.
    let title = match &option.action_type {
        gestos::config::ActionType::Keyboard(steps) if steps.is_empty() => {
            crate::ui::editor::name_from_search(query, &option.name)
                .map(|name| format!("Create Advanced Action: \u{201c}{name}\u{201d}"))
        }
        gestos::config::ActionType::Keypress(keys) if keys.is_empty() => {
            crate::ui::editor::name_from_search(query, &option.name)
                .map(|name| format!("Create Keyboard Shortcut: \u{201c}{name}\u{201d}"))
        }
        _ => None,
    };
    let name = gtk::Label::new(Some(&opens_editor(title.as_deref().unwrap_or(&option.name))));
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
    // No arrow, as it opens the editor over the page, not another page.
    row_box.append(&text);

    row.set_child(Some(&row_box));
    row
}

/// Scrolls the window to a row just made, so that it's seen, once it has its place: until then
/// it has no size, as a new row isn't laid out until the next frame.
fn scroll_to(row: &gtk::ListBoxRow) {
    row.add_tick_callback(|row, _| {
        if row.height() == 0 {
            return glib::ControlFlow::Continue;
        }
        let scrolled = row.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>();
        // The scrolled content, where the row's place doesn't depend on the scrolling.
        let content = scrolled.as_ref().and_then(|s| s.child()).and_then(|viewport| viewport.first_child());
        if let (Some(scrolled), Some(content)) = (scrolled, content) {
            if let Some(bounds) = row.compute_bounds(&content) {
                let adjustment = scrolled.vadjustment();
                let (top, height) = (bounds.y() as f64, bounds.height() as f64);
                let page = adjustment.page_size();
                // Centered, unless it's already in sight.
                if top < adjustment.value() || top + height > adjustment.value() + page {
                    adjustment.set_value(top - (page - height).max(0.0) / 2.0);
                }
            }
        }
        glib::ControlFlow::Break
    });
}

/// Whether the main page shows only the categories, each with its gestures on its own page: while
/// nothing is searched for and there are enough actions to sort into categories.
fn shows_categories(state: &AppState) -> bool {
    state.search_entry.text().trim().is_empty()
        && find_suggestions(&state.action_options, &state.config.gestures, "").len()
            >= crate::suggestions::FEWEST_TO_CATEGORIZE
}

/// Below the search results, offers actions that match the search but no gesture uses yet.
fn refresh_suggestions(state_rc: &Rc<RefCell<AppState>>, no_gestures_shown: bool) {
    let state = state_rc.borrow();
    // The categories alone need no heading, as they're all the page has. They keep the room it
    // took under the search, so they don't crowd it.
    let categories = shows_categories(&state);
    state.suggestion_heading.set_visible(!categories);
    state.suggestion_box.set_margin_top(if categories { 12 } else { 0 });
    for list in [&state.suggestion_list, &state.apart_list] {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
    }

    let query = state.search_entry.text().to_string();
    let found = find_suggestions(&state.action_options, &state.config.gestures, &query);
    let searching = !query.trim().is_empty();
    let gestures_in = gestures_by_category(&state.config.gestures, &state.action_options);
    let (apart, mut found) = arrange_suggestions(found, searching, None, &gestures_in);
    // An app searched for by its name leads to what's done inside it, after opening it, if that's
    // still offered, or first.
    if searching {
        let apps = apps_to_be_inside(&state)
            .into_iter()
            // By a word of its name starting with what's searched, so that a letter or two don't
            // bring every app.
            .filter(|(_, name)| {
                let name = name.to_lowercase();
                let first = query.trim().to_lowercase();
                let first = first.split_whitespace().next().unwrap_or_default().to_string();
                search_matches(&name, &query)
                    && name.split(|c: char| !c.is_alphanumeric()).any(|word| word.starts_with(&first))
            })
            .map(|(app, _)| app)
            .collect::<Vec<_>>();
        for (index, app) in apps.into_iter().enumerate() {
            let opens = found.iter().position(|suggestion| {
                matches!(suggestion, Suggestion::Action(option)
                    if crate::ui::app_picker::opened_app(&option.action_type) == Some(app.as_str()))
            });
            found.insert(opens.map_or(index, |opens| opens + 1), Suggestion::App(app));
        }
    }
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
            Suggestion::App(app) => {
                let title = format!("Actions inside {}", crate::ui::app_picker::app_name(app));
                app_row_named(&state, app, &title)
            }
        };
        state.suggestion_list.append(&row);
    }
    state.suggestion_list.set_visible(!found.is_empty());

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
    drop(state);
    let mut state = state_rc.borrow_mut();
    state.visible_apart = apart;
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

/// Shows a gesture just made from the main page where it's kept: on its category's page, when
/// the page shows categories, or in the page's own list. The search that found its action is
/// cleared, as it would hide the gesture.
pub fn show_new_gesture(state_rc: &Rc<RefCell<AppState>>, name: &str) {
    let search = state_rc.borrow().search_entry.clone();
    if !search.text().is_empty() {
        // Shown once the search is cleared, which takes a moment.
        state_rc.borrow_mut().show_after_search = Some(name.to_string());
        search.set_text("");
    }
    let (category, app) = {
        let state = state_rc.borrow();
        let categorized = find_suggestions(&state.action_options, &state.config.gestures, "").len()
            >= crate::suggestions::FEWEST_TO_CATEGORIZE;
        let gesture = state.config.gestures.iter().find(|g| g.name == name).filter(|_| categorized);
        let category = gesture.and_then(|g| gesture_category(g, &state.action_options));
        // One made inside an app is on that app's page. One that opens an app is among the apps.
        let app = gesture.filter(|_| category == Some(Category::InsideApps)).and_then(|g| match g.apps.as_slice() {
            [app] => Some(app.clone()),
            _ => None,
        });
        (category, app)
    };
    if category.is_some() {
        open_category(state_rc, category);
    }
    if let Some(app) = app {
        open_app_page(state_rc, &app);
    }
    refresh_gesture_list(state_rc, Some(name));
}

pub fn refresh_gesture_list(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) {
    let no_gestures_shown = show_gestures(state_rc, select_name);
    refresh_suggestions(state_rc, no_gestures_shown);
    refresh_category_page(state_rc, select_name);
    refresh_app_page(state_rc, select_name);
}

/// Shows the gestures that match the search. The one named, such as the one just saved, gets the
/// focus, so that the keyboard carries on from it.
/// Gestures by the category of what they do, in the categories' order, those that work only inside
/// some apps in theirs, or None when they're all in the same one, which needs no grouping.
fn by_category(state: &AppState, gestures: &[Gesture]) -> Option<Vec<(Category, Vec<Gesture>)>> {
    let mut groups: Vec<(Category, Vec<Gesture>)> = Vec::new();
    for gesture in gestures {
        let category = if gesture.apps.is_empty() {
            action_category(gesture, &state.action_options).unwrap_or(Category::Other)
        } else {
            Category::InsideApps
        };
        match groups.iter_mut().find(|(other, _)| *other == category) {
            Some((_, in_it)) => in_it.push(gesture.clone()),
            None => groups.push((category, vec![gesture.clone()])),
        }
    }
    if groups.len() < 2 {
        return None;
    }
    groups.sort_by_key(|(category, _)| *category);
    Some(groups)
}

fn show_gestures(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) -> bool {
    let state = state_rc.borrow();

    // Clear list
    while let Some(child) = state.main_list.first_child() {
        state.main_list.remove(&child);
    }
    while let Some(child) = state.gesture_groups.first_child() {
        state.gesture_groups.remove(&child);
    }
    let visible = get_visible_gestures(&state, None);
    // Above the categories too, unless Preferences ask for only the categories. With none, the
    // categories are all there is.
    if shows_categories(&state) && (state.config.hide_gestures || visible.is_empty()) {
        state.main_list.set_visible(false);
        state.empty_state_box.set_visible(false);
        return true;
    }
    let mut focused = false;

    if visible.is_empty() {
        let searching = !state.search_entry.text().trim().is_empty();
        state.empty_state_box.set_title(if searching { "Nothing Found" } else { "No Gestures" });
        state.empty_state_box.set_description(searching.then_some("Try different words."));
        state.empty_state_box.set_visible(true);
        state.main_list.set_visible(false);
    } else if let Some(groups) = by_category(&state, &visible) {
        state.empty_state_box.set_visible(false);
        state.main_list.set_visible(false);
        for (category, gestures) in groups {
            let heading = gtk::Label::new(Some(category.name()));
            heading.add_css_class("section-header");
            heading.set_xalign(0.0);
            state.gesture_groups.append(&heading);
            let list = gtk::ListBox::new();
            list.add_css_class("boxed-list");
            list.set_selection_mode(gtk::SelectionMode::None);
            list.set_margin_bottom(24);
            for gesture in &gestures {
                let row = create_gesture_row(gesture, state_rc, true);
                list.append(&row);
                if select_name == Some(gesture.name.as_str()) {
                    focused = row.grab_focus();
                    scroll_to(&row);
                }
            }
            let ids: Vec<String> = gestures.iter().map(|g| g.id.clone()).collect();
            let state_edit = Rc::downgrade(state_rc);
            list.connect_row_activated(move |_, row| {
                let Some(state_rc) = state_edit.upgrade() else { return };
                let gesture = ids.get(row.index() as usize).and_then(|id| {
                    state_rc.borrow().config.gestures.iter().find(|g| &g.id == id).cloned()
                });
                if let Some(gesture) = gesture {
                    open_gesture_editor(&state_rc, gesture);
                }
            });
            state.gesture_groups.append(&list);
        }
    } else {
        state.empty_state_box.set_visible(false);
        state.main_list.set_visible(true);

        for gesture in &visible {
            let row = create_gesture_row(gesture, state_rc, true);
            state.main_list.append(&row);

            if select_name == Some(gesture.name.as_str()) {
                focused = row.grab_focus();
                scroll_to(&row);
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
pub fn load_action_options(state: &Rc<RefCell<AppState>>) {
    let load_options = gtk::gio::spawn_blocking(|| {
        let mut options = get_static_action_options();
        options.extend(fetch_gnome_action_options());
        options.extend(fetch_kde_action_options());
        options.extend(app_action_options());
        // Templates for any app are offered as other actions are, and the others on their apps'
        // pages only.
        let (for_some_apps, for_any): (Vec<_>, Vec<_>) =
            crate::action_templates::load().into_iter().partition(|template| !template.apps.is_empty());
        options.extend(for_any.iter().map(crate::action_templates::option));
        (options, for_some_apps)
    });
    let state_options = Rc::clone(state);
    glib::spawn_future_local(async move {
        if let Ok((options, app_templates)) = load_options.await {
            let mut state = state_options.borrow_mut();
            // Templates turned off in Preferences aren't offered.
            let disabled = state.config.disabled_templates.clone();
            let off = |id: &str| disabled.iter().any(|disabled| disabled == id);
            state.action_options = options
                .into_iter()
                .filter(|option| !matches!(&option.action_type, gestos::config::ActionType::Template(id) if off(id)))
                .collect();
            state.app_templates = app_templates.into_iter().filter(|template| !off(&template.id)).collect();
            drop(state);
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
    let back_button = gtk::Button::from_icon_name("go-previous-symbolic");
    back_button.set_tooltip_text(Some("Back"));
    back_button.update_property(&[gtk::accessible::Property::Label("Back")]);
    back_button.set_visible(false);
    header.pack_start(&back_button);

    // Preferences button
    let settings_btn = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_btn.set_tooltip_text(Some("Preferences"));
    settings_btn.update_property(&[gtk::accessible::Property::Label("Preferences")]);
    header.pack_end(&settings_btn);

    // Content VBox
    let content_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_vbox.add_css_class("main-window-content");
    // A category's page, and an app's, slide in over the main page, and back out.
    let pages = gtk::Stack::new();
    pages.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    pages.add_named(&content_vbox, Some(MAIN_PAGE));
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&pages));
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
    let gesture_groups = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_container.append(&gesture_groups);

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

    let suggestion_heading = gtk::Label::new(Some("Create New Gesture"));
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

    if let Err(e) = crate::system::initialize_config() {
        eprintln!("Warning: Failed to initialize configuration: {}", e);
    }
    let mut config = Configuration::load_from_defaults();
    let dbus_conn = zbus::blocking::Connection::session().ok();
    // A gesture is for one app or for all of them, so one for several becomes one for each.
    if config.split_gestures_for_several_apps(gestos::key_steps::app_name) {
        match config.save_to_file() {
            Ok(()) => crate::daemon::reload_daemon(dbus_conn.as_ref(), Some(&window)),
            Err(e) => eprintln!("Could not save the gestures made one per app: {e}"),
        }
    }

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
        gesture_groups,
        search_entry,
        window,
        newly_added_gestures: Vec::new(),
        show_after_search: None,
        dbus_conn,
        empty_state_box,
        action_options: Vec::new(),
        app_templates: Vec::new(),
        visible_suggestions: Vec::new(),
        apart_list,
        visible_apart: Vec::new(),
        suggestion_box,
        suggestion_no_match,
        suggestion_heading,
        suggestion_list,
        extension_status,
        extension_banner: extension_banner.clone(),
        open_category: None,
        category_page: None,
        app_page: None,
        pages,
        back_button,
    }));

    // Back with the header's button, Escape, Alt+Left or the mouse's back button, as in GNOME's
    // own apps. Escape on the main page is left to its search.
    let state_back = Rc::downgrade(&state);
    state.borrow().back_button.connect_clicked(move |_| {
        if let Some(state) = state_back.upgrade() {
            go_back(&state);
        }
    });
    let keys = gtk::ShortcutController::new();
    let state_back = Rc::downgrade(&state);
    keys.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Escape|<Alt>Left"),
        Some(gtk::CallbackAction::new(move |_, _| match state_back.upgrade() {
            Some(state) if go_back(&state) => glib::Propagation::Stop,
            _ => glib::Propagation::Proceed,
        })),
    ));
    state.borrow().window.add_controller(keys);
    let mouse_back = gtk::GestureClick::new();
    mouse_back.set_button(8);
    let state_back = Rc::downgrade(&state);
    mouse_back.connect_pressed(move |_, _, _, _| {
        if let Some(state) = state_back.upgrade() {
            go_back(&state);
        }
    });
    state.borrow().window.add_controller(mouse_back);
    let state_pages = Rc::downgrade(&state);
    state.borrow().pages.connect_transition_running_notify(move |_| {
        if let Some(state) = state_pages.upgrade() {
            drop_hidden_pages(&state);
        }
    });


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
            let show = state_clone.borrow_mut().show_after_search.take();
            refresh_gesture_list(&state_clone, show.as_deref());
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
                Some(Suggestion::Action(option)) => open_gesture_editor_for_action(&state_suggestion, option),
                Some(Suggestion::App(app)) => {
                    open_category(&state_suggestion, Some(Category::InsideApps));
                    open_app_page(&state_suggestion, &app);
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
         searchentry, entry.search { border-radius: 18px; }\n\
         .context-title { font-size: 1.5em; font-weight: bold; }\n\
         .icon-purple { color: @purple_2; }\n\
         .action-label { font-size: 0.9em; opacity: 0.7; }\n\
         .settings-row { padding: 8px; }\n\
         .section-header { font-weight: bold; margin-top: 8px; margin-bottom: 4px; }\n\
         .status-label { font-weight: bold; }\n\
         .warning-box { padding: 8px; background-color: alpha(@warning_color, 0.15); border: 1px solid alpha(@warning_color, 0.3); border-radius: 6px; }\n\
         .warning-label { color: @warning_color; font-size: 0.95em; }\n\
         .extension-banner { padding: 6px 12px; min-height: 36px; background-color: alpha(@warning_color, 0.15); }\n\
         .keycap-small { padding: 0 4px; background-color: alpha(currentColor, 0.08); border: 1px solid alpha(currentColor, 0.15); border-radius: 4px; font-weight: bold; font-size: 0.7em; }\n\
         .keycap-row { padding: 0 6px; background-color: alpha(currentColor, 0.08); border: 1px solid alpha(currentColor, 0.15); border-radius: 4px; font-weight: bold; font-size: 0.85em; }\n\
         .keycap { padding: 6px 12px; background-color: alpha(currentColor, 0.08); border: 1px solid alpha(currentColor, 0.15); border-radius: 6px; font-weight: bold; font-size: 1.1em; }\n\
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
