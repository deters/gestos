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

/// A category's page, in place of the main page: the gestures in it, then its actions to make
/// more, with the one to make its own of first.
#[derive(Clone)]
pub struct CategoryPage {
    page: gtk::Widget,
    /// The headings over the gestures and over the actions to make more, as on an app's page.
    gestures_heading: gtk::Label,
    create_heading: gtk::Label,
    gestures: gtk::ListBox,
    apart: gtk::ListBox,
    actions: gtk::ListBox,
    /// What the lists show, by row, for when one is activated.
    shown: Rc<RefCell<(Vec<String>, Vec<crate::state::EditorActionOption>, Vec<Suggestion>)>>,
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

/// An app's own page: the gestures made for it, then opening it and its own shortcuts.
#[derive(Clone)]
pub struct AppPage {
    page: gtk::Widget,
    app: String,
    /// The gestures only for the app, and those for every app that open it, apart, each under
    /// its heading, those for every app shown first.
    only_here: (gtk::Label, gtk::ListBox),
    everywhere: (gtk::Label, gtk::ListBox),
    /// Opening it, which makes a gesture for every app, under its heading, offered until a gesture
    /// does.
    opens: (gtk::Label, gtk::ListBox),
    /// The app's own shortcuts, by group: its heading, if any, its list, and each row with its
    /// keys, offered until a gesture for the app presses them.
    shortcut_groups: Rc<RefCell<Vec<(Option<gtk::Label>, gtk::ListBox, Vec<(gtk::ListBoxRow, String)>)>>>,
    /// The gestures each list shows, by row, for when one is activated.
    shown: Rc<RefCell<(Vec<String>, Vec<String>)>>,
}

/// How many gestures are made for an app.
fn gestures_for_app(state: &AppState, app: &str) -> usize {
    state.config.gestures.iter().filter(|g| !g.is_deleted && is_for_app(g, app)).count()
}

/// Whether a gesture is made for an app: only for it, or to open it first.
fn is_for_app(gesture: &Gesture, app: &str) -> bool {
    gesture.apps.iter().any(|id| id == app)
        || gesture.actions.first().and_then(crate::ui::app_picker::first_app) == Some(app)
}

/// Shows the gestures made for the app open, if one is. The one named, such as the one just
/// saved, gets the focus.
fn refresh_app_page(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) {
    let state = state_rc.borrow();
    let Some(page) = state.app_page.clone() else { return };
    let (only_here, everywhere): (Vec<Gesture>, Vec<Gesture>) = state
        .config
        .gestures
        .iter()
        .filter(|g| !g.is_deleted && is_for_app(g, &page.app))
        .cloned()
        .partition(|g| !g.apps.is_empty());
    let show = |(heading, list): &(gtk::Label, gtk::ListBox), gestures: &[Gesture]| {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        for gesture in gestures {
            let row = create_gesture_row(gesture, state_rc);
            list.append(&row);
            if select_name == Some(gesture.name.as_str()) {
                row.grab_focus();
                scroll_to(&row);
            }
        }
        heading.set_visible(!gestures.is_empty());
        list.set_visible(!gestures.is_empty());
    };
    show(&page.only_here, &only_here);
    show(&page.everywhere, &everywhere);
    let opened = state.config.gestures.iter().any(|g| {
        !g.is_deleted
            && g.actions.first().and_then(crate::ui::app_picker::opened_app) == Some(page.app.as_str())
    });
    page.opens.0.set_visible(!opened);
    page.opens.1.set_visible(!opened);
    // A shortcut already pressed by a gesture for the app isn't offered again.
    let used = |keys: &str| {
        state.config.gestures.iter().any(|g| {
            !g.is_deleted
                && g.apps.iter().any(|app| *app == page.app)
                && matches!(g.actions.first(),
                    Some(gestos::config::ActionType::Keypress(pressed)) if crate::actions::same_keys(pressed, keys))
        })
    };
    for (label, list, rows) in page.shortcut_groups.borrow().iter() {
        let mut any = false;
        for (row, keys) in rows {
            let offered = !used(keys);
            row.set_visible(offered);
            any |= offered;
        }
        list.set_visible(any);
        if let Some(label) = label {
            label.set_visible(any);
        }
    }
    let ids = |gestures: Vec<Gesture>| gestures.into_iter().map(|g| g.id).collect();
    *page.shown.borrow_mut() = (ids(only_here), ids(everywhere));
}

/// Opens an app's own page in place of its category's: opening the app first, then each of the
/// shortcuts its Keyboard Shortcuts window lists, under their groups, if it has that window.
fn open_app_page(state_rc: &Rc<RefCell<AppState>>, open: crate::state::EditorActionOption, app: &str) {
    // One app's at a time, in place of its category's, never of another app's.
    state_rc.borrow_mut().app_page = None;
    let shortcuts = gestos::app_shortcuts::shortcuts_of(app);
    let name = crate::ui::app_picker::app_name(app);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
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
    content.append(&heading);
    let list = || {
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_margin_bottom(24);
        list
    };

    let heading = |text: &str| {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("section-header");
        label.set_xalign(0.0);
        label
    };
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_child(Some(&clamped(&content)));
    let page = AppPage {
        page: scrolled.clone().upcast(),
        app: app.to_string(),
        only_here: (heading(&format!("Gestures Inside {name}")), list()),
        everywhere: (heading("Gestures"), list()),
        opens: (heading("Create New Gesture"), list()),
        shortcut_groups: Default::default(),
        shown: Default::default(),
    };
    // Those for every app first, then those only for this one.
    for (index, (heading, list)) in [(1, &page.everywhere), (0, &page.only_here)] {
        let state_edit = Rc::downgrade(state_rc);
        let shown = Rc::clone(&page.shown);
        list.connect_row_activated(move |_, row| {
            let Some(state_rc) = state_edit.upgrade() else { return };
            let shown = shown.borrow();
            let ids = if index == 0 { &shown.0 } else { &shown.1 };
            let id = ids.get(row.index() as usize).cloned();
            drop(shown);
            let gesture = id
                .and_then(|id| state_rc.borrow().config.gestures.iter().find(|g| g.id == id).cloned());
            if let Some(gesture) = gesture {
                open_gesture_editor(&state_rc, gesture);
            }
        });
        content.append(heading);
        content.append(list);
    }

    // What can be made for the app, after the gestures it already has, set well apart from them:
    // opening it, a gesture for every app, then the gestures only for it.
    let (create, opens) = page.opens.clone();
    create.set_margin_top(24);
    content.append(&create);
    opens.append(&suggestion_row(&open, &state_rc.borrow().extension_status, ""));
    let state_open = Rc::downgrade(state_rc);
    opens.connect_row_activated(move |_, _| {
        if let Some(state_rc) = state_open.upgrade() {
            open_gesture_editor_for_action(&state_rc, open.clone());
        }
    });
    content.append(&opens);
    let inside = heading(&format!("Create a New Gesture Inside {name}"));
    inside.set_margin_top(12);
    content.append(&inside);

    // First, the action templates for this app, by its ID or its kind, like a file manager's, as
    // they do more than one of its shortcuts.
    let categories = gtk::gio::DesktopAppInfo::new(&format!("{}.desktop", app.trim_end_matches(".desktop")))
        .and_then(|info| info.categories().map(|categories| categories.to_string()))
        .unwrap_or_default();
    let templates: Vec<_> = state_rc
        .borrow()
        .app_templates
        .iter()
        .filter(|template| template.is_for(app, &categories))
        .map(crate::action_templates::option)
        .collect();
    if !templates.is_empty() {
        let rows = list();
        for option in &templates {
            rows.append(&suggestion_row(option, &state_rc.borrow().extension_status, ""));
        }
        let state_template = Rc::downgrade(state_rc);
        let app_template = app.to_string();
        rows.connect_row_activated(move |_, row| {
            let (Some(state_rc), Some(option)) = (state_template.upgrade(), templates.get(row.index() as usize))
            else {
                return;
            };
            crate::ui::editor::open_gesture_editor_in_apps(&state_rc, option.clone(), &[app_template.clone()]);
        });
        content.append(&rows);
    }

    let mut sections: Vec<(&str, Vec<&gestos::app_shortcuts::AppShortcut>)> = Vec::new();
    for shortcut in shortcuts.iter() {
        match sections.iter_mut().find(|(section, _)| *section == shortcut.section) {
            Some((_, in_it)) => in_it.push(shortcut),
            None => sections.push((&shortcut.section, vec![shortcut])),
        }
    }
    // The app's own shortcuts, each group of them under its name. The names are the app's, so
    // they're smaller and dimmer than Gestos's own headings.
    for (section, in_it) in sections {
        let label = (!section.is_empty()).then(|| {
            let label = gtk::Label::new(Some(section));
            label.add_css_class("caption-heading");
            label.add_css_class("dim-label");
            label.set_xalign(0.0);
            label.set_margin_bottom(6);
            label
        });
        if let Some(label) = &label {
            content.append(label);
        }
        let rows = list();
        let mut shown_rows = Vec::new();
        for shortcut in &in_it {
            let row = app_shortcut_row(shortcut);
            rows.append(&row);
            shown_rows.push((row, shortcut.keys.clone()));
        }
        page.shortcut_groups.borrow_mut().push((label, rows.clone(), shown_rows));
        let keys: Vec<String> = in_it.iter().map(|shortcut| shortcut.keys.clone()).collect();
        let state_add = Rc::downgrade(state_rc);
        let app = app.to_string();
        rows.connect_row_activated(move |_, row| {
            if let (Some(state_rc), Some(keys)) = (state_add.upgrade(), keys.get(row.index() as usize)) {
                crate::ui::editor::open_gesture_editor_for_app_shortcut(&state_rc, &app, keys);
            }
        });
        content.append(&rows);
    }

    // Last, after the shortcuts the app lists, for one it doesn't, and for an advanced action.
    let advanced = state_rc
        .borrow()
        .action_options
        .iter()
        .find(|option| {
            matches!(&option.action_type, gestos::config::ActionType::Keyboard(steps) if steps.is_empty())
        })
        .cloned();
    // Each as it's shown, named for the app it's made in, and as the editor gets it.
    let custom_shortcut = shortcut_on_app(&state_rc.borrow(), app).map(|option| (option.clone(), option));
    let advanced = advanced.map(|option| {
        let shown = crate::state::EditorActionOption {
            name: format!("{} on {name}", option.name),
            ..option.clone()
        };
        (shown, option)
    });
    let (shown, own_options): (Vec<_>, Vec<_>) = [custom_shortcut, advanced].into_iter().flatten().unzip();
    if !own_options.is_empty() {
        let own = list();
        for option in &shown {
            let row = suggestion_row(option, &state_rc.borrow().extension_status, "");
            row.add_css_class("build-your-own");
            own.append(&row);
        }
        let state_own = Rc::downgrade(state_rc);
        let app_own = app.to_string();
        own.connect_row_activated(move |_, row| {
            let (Some(state_rc), Some(option)) = (state_own.upgrade(), own_options.get(row.index() as usize))
            else {
                return;
            };
            crate::ui::editor::open_gesture_editor_in_apps(&state_rc, option.clone(), &[app_own.clone()]);
        });
        content.append(&own);
    }

    state_rc.borrow_mut().app_page = Some(page);
    refresh_app_page(state_rc, None);
    show_page(state_rc, &scrolled.upcast());
}

/// An app, which opens its own page, with how many gestures are made for it, if any.
fn app_row(app: &str, count: usize) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    row_box.append(&crate::ui::app_picker::app_icon(app, 16));
    let name = gtk::Label::new(Some(&crate::ui::app_picker::app_name(app)));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row_box.append(&name);
    let count_label = gtk::Label::new(Some(&count.to_string()));
    count_label.add_css_class("dim-label");
    count_label.set_visible(count > 0);
    row_box.append(&count_label);
    row_box.append(&gtk::Image::from_icon_name("go-next-symbolic"));
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
    let title = gtk::Label::new(Some(&shortcut.title));
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
    let section = |text: &str| {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("section-header");
        label.set_xalign(0.0);
        content.append(&label);
        label
    };
    let gestures_heading = section("Gestures");
    let gestures = list();
    let create_heading = section("Create New Gesture");
    let apart = list();
    apart.set_margin_bottom(16);
    let actions = list();
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_child(Some(&clamped(&content)));

    let page = CategoryPage {
        page: scrolled.clone().upcast(),
        gestures_heading,
        create_heading,
        gestures,
        apart,
        actions,
        shown: Default::default(),
    };
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
            let Some(state_rc) = state_rc.upgrade() else { return };
            match option {
                Some(option) => match crate::ui::app_picker::opened_app(&option.action_type).map(str::to_string) {
                    Some(app) => open_app_page(&state_rc, option, &app),
                    None => open_gesture_editor_for_action(&state_rc, option),
                },
                // The row after them, on the Keyboard Shortcuts page, leads to the apps.
                None => open_category(&state_rc, Some(Category::Apps)),
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
                // An app opens its own page, with opening it and any shortcuts it lists.
                Some(Suggestion::Action(option)) => {
                    let app = crate::ui::app_picker::opened_app(&option.action_type).map(str::to_string);
                    match app {
                        Some(app) => open_app_page(&state_rc, option, &app),
                        None => open_gesture_editor_for_action(&state_rc, option),
                    }
                }
                Some(Suggestion::Category(category, _)) => open_category(&state_rc, Some(category)),
                None => {}
            }
        });
    }

    state_rc.borrow_mut().category_page = Some(page);
    refresh_category_page(state_rc, None);
    show_page(state_rc, &scrolled.upcast());
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
    // The apps' gestures are each on its own app's page instead.
    let gestures = match category {
        Category::Apps => Vec::new(),
        _ => get_visible_gestures(&state, Some(category)),
    };
    for gesture in &gestures {
        let row = create_gesture_row(gesture, state_rc);
        page.gestures.append(&row);
        if select_name == Some(gesture.name.as_str()) {
            row.grab_focus();
            scroll_to(&row);
        }
    }
    page.gestures.set_visible(!gestures.is_empty());

    let found = find_suggestions(&state.action_options, &state.config.gestures, "");
    let gestures_in = gestures_by_category(&state.config.gestures, &state.action_options);
    let (mut apart, mut found) = arrange_suggestions(found, false, Some(category), &gestures_in);
    // Every app, as each opens its own page, even one already opened by a gesture, by name.
    // Those with gestures come first, apart from the rest.
    if category == Category::Apps {
        let mut apps: Vec<_> = state
            .action_options
            .iter()
            .filter(|option| crate::ui::app_picker::opened_app(&option.action_type).is_some())
            .cloned()
            .collect();
        apps.sort_by_cached_key(|option| option.name.to_lowercase());
        let (with_gestures, without): (Vec<_>, Vec<_>) = apps.into_iter().partition(|option| {
            crate::ui::app_picker::opened_app(&option.action_type)
                .is_some_and(|app| gestures_for_app(&state, app) > 0)
        });
        apart = with_gestures;
        found = without.into_iter().map(Suggestion::Action).collect();
    }
    for option in &apart {
        let row = match crate::ui::app_picker::opened_app(&option.action_type) {
            Some(app) => app_row(app, gestures_for_app(&state, app)),
            None => {
                let row = suggestion_row(option, &state.extension_status, "");
                row.add_css_class("build-your-own");
                row
            }
        };
        page.apart.append(&row);
    }
    // The apps' own shortcuts are on their pages, which the Apps page leads to, after the custom
    // shortcut, as both are for shortcuts this page doesn't list.
    let to_apps = category == Category::KeyboardShortcuts;
    if to_apps {
        let row = category_row_named(Category::Apps, "Shortcuts in Apps", 0);
        row.add_css_class("build-your-own");
        page.apart.append(&row);
    }
    page.apart.set_visible(!apart.is_empty() || to_apps);
    for suggestion in &found {
        let row = match suggestion {
            Suggestion::Action(option) => match crate::ui::app_picker::opened_app(&option.action_type) {
                Some(app) => app_row(app, gestures_for_app(&state, app)),
                None => suggestion_row(option, &state.extension_status, ""),
            },
            Suggestion::Category(category, count) => category_row(*category, *count),
        };
        page.actions.append(&row);
    }
    page.actions.set_visible(!found.is_empty());
    // The Apps page lists apps, which aren't gestures or actions, so it needs neither heading.
    let headed = category != Category::Apps;
    page.gestures_heading.set_visible(headed && page.gestures.get_visible());
    page.create_heading.set_visible(headed && !(apart.is_empty() && found.is_empty()));
    *page.shown.borrow_mut() = (gestures.into_iter().map(|g| g.id).collect(), apart, found);
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
    row_box.append(&gtk::Image::from_icon_name(category.icon_name()));
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
    let (apart, found) = arrange_suggestions(found, searching, None, &gestures_in);
    for option in &apart {
        let row = suggestion_row(option, &state.extension_status, &query);
        row.add_css_class("build-your-own");
        state.apart_list.append(&row);
    }
    state.apart_list.set_visible(!apart.is_empty());
    for suggestion in &found {
        let row = match suggestion {
            // An app is its page, which opening it is on.
            Suggestion::Action(option) => match crate::ui::app_picker::opened_app(&option.action_type) {
                Some(app) => app_row(app, gestures_for_app(&state, app)),
                None => suggestion_row(option, &state.extension_status, &query),
            },
            Suggestion::Category(category, count) => category_row(*category, *count),
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
        // An app's gesture is on that app's page, with how to open it.
        let app = gesture
            .filter(|_| category == Some(Category::Apps))
            .and_then(|g| match g.apps.as_slice() {
                [app] => Some(app.clone()),
                _ => g.actions.first().and_then(crate::ui::app_picker::first_app).map(str::to_string),
            })
            .and_then(|app| {
                let open = state.action_options.iter().find(|option| {
                    crate::ui::app_picker::opened_app(&option.action_type) == Some(app.as_str())
                });
                open.cloned().map(|open| (app, open))
            });
        (category, app)
    };
    if category.is_some() {
        open_category(state_rc, category);
    }
    if let Some((app, open)) = app {
        open_app_page(state_rc, open, &app);
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
/// Gestures by the app they're for, those for every app first, then each app's by its name, or
/// None when they're all for the same, which needs no grouping.
fn by_app(gestures: &[Gesture]) -> Option<Vec<(Option<String>, Vec<Gesture>)>> {
    let mut groups: Vec<(Option<String>, Vec<Gesture>)> = Vec::new();
    for gesture in gestures {
        let app = gesture.apps.first().cloned();
        match groups.iter_mut().find(|(other, _)| *other == app) {
            Some((_, in_it)) => in_it.push(gesture.clone()),
            None => groups.push((app, vec![gesture.clone()])),
        }
    }
    if groups.len() < 2 {
        return None;
    }
    groups.sort_by_cached_key(|(app, _)| {
        app.as_ref().map(|app| crate::ui::app_picker::app_name(app).to_lowercase())
    });
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
    } else if let Some(groups) = by_app(&visible) {
        // A gesture is for one app or for all of them, so they're shown by app.
        state.empty_state_box.set_visible(false);
        state.main_list.set_visible(false);
        for (app, gestures) in groups {
            let heading = gtk::Label::new(Some(&match &app {
                Some(app) => format!("Gestures inside {}", crate::ui::app_picker::app_name(app)),
                None => "Gestures".to_string(),
            }));
            heading.add_css_class("section-header");
            heading.set_xalign(0.0);
            state.gesture_groups.append(&heading);
            let list = gtk::ListBox::new();
            list.add_css_class("boxed-list");
            list.set_selection_mode(gtk::SelectionMode::None);
            list.set_margin_bottom(24);
            for gesture in &gestures {
                let row = create_gesture_row(gesture, state_rc);
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
            let row = create_gesture_row(gesture, state_rc);
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
                Some(Suggestion::Action(option)) => {
                    match crate::ui::app_picker::opened_app(&option.action_type).map(str::to_string) {
                        Some(app) => open_app_page(&state_suggestion, option, &app),
                        None => open_gesture_editor_for_action(&state_suggestion, option),
                    }
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
         .icon-purple { color: @purple_2; }\n\
         .action-label { font-size: 0.9em; opacity: 0.7; }\n\
         .settings-row { padding: 8px; }\n\
         .section-header { font-weight: bold; margin-top: 8px; margin-bottom: 4px; }\n\
         .status-label { font-weight: bold; }\n\
         .warning-box { padding: 8px; background-color: alpha(@warning_color, 0.15); border: 1px solid alpha(@warning_color, 0.3); border-radius: 6px; }\n\
         .warning-label { color: @warning_color; font-size: 0.95em; }\n\
         .extension-banner { padding: 6px 12px; min-height: 36px; background-color: alpha(@warning_color, 0.15); }\n\
         .keycap-small { padding: 0 4px; background-color: alpha(currentColor, 0.08); border: 1px solid alpha(currentColor, 0.15); border-radius: 4px; font-weight: bold; font-size: 0.7em; }\n\
         .keycap { padding: 6px 12px; background-color: alpha(currentColor, 0.08); border: 1px solid alpha(currentColor, 0.15); border-radius: 6px; font-weight: bold; font-size: 1.1em; }\n\
         .drag-handle { opacity: 0.55; margin-right: 4px; }\n\
         .drag-handle:hover { opacity: 1; }\n\
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
