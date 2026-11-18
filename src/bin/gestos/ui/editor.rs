use gtk::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use gestos::config::{ActionType, Gesture};
use gestos::config::generate_unique_id;
use crate::state::{AppState, EditorActionOption};
use crate::actions::{action_matches, action_needs_parameter};
use crate::ui::shortcut_recorder::open_shortcut_recorder;
use crate::ui::gesture_recorder::{
    drawing_corner, has_drawing, open_gesture_recorder, similar_gesture, warning_box, Drawing, ModifierKeys,
};
use crate::ui::keyboard_steps::append_steps_rows;
use gestos::key_steps::{format_steps, parse_steps, KeyStep};
use crate::ui::components::draw_gesture_path;
use crate::daemon::reload_daemon;
use crate::ui::main_window::{get_visible_gestures, refresh_gesture_list};

// The width of the editor's content, as the main window's lists have, and the margins around it.
const EDITOR_CONTENT_WIDTH: i32 = 500;
const EDITOR_SIDE_MARGIN: i32 = 24;
// The smallest the gesture editor gets: its content and margins wide, with room for the drawing
// and a few rows below it.
const EDITOR_MIN_WIDTH: i32 = EDITOR_CONTENT_WIDTH + 2 * EDITOR_SIDE_MARGIN;
const EDITOR_MIN_HEIGHT: i32 = 480;

/// Whether an option is the generic Custom Action, whose steps are filled in.
fn is_custom_action(option: &EditorActionOption) -> bool {
    matches!(&option.action_type, ActionType::Keyboard(steps) if steps.is_empty())
}

/// The steps an action has that can be customized: an app's own action's one step, or a
/// template's steps as installed now. Customizing copies them, so the gesture no longer follows
/// the template.
fn customizable_steps(action: &ActionType) -> Option<Vec<KeyStep>> {
    match action {
        ActionType::Keyboard(steps) if crate::actions::only_opens_an_app(action) => Some(steps.clone()),
        ActionType::Template(id) => crate::action_templates::find(id).map(|template| template.steps),
        _ => None,
    }
}

/// Whether a gesture with this action is named by the user, as a custom action or a custom
/// keyboard shortcut are, since the action alone doesn't say what they're for.
fn has_own_name(option: &EditorActionOption) -> bool {
    is_custom_action(option) || crate::suggestions::is_custom_shortcut(option)
}

/// Whether a custom action's name is one made from the action, rather than one the user gave it.
/// Names from before the action was renamed count too.
fn is_automatic_name(name: &str, action_name: &str) -> bool {
    let name = name.trim();
    name.is_empty()
        || name.starts_with(action_name)
        || name.starts_with("Custom Action")
        || name.starts_with("Custom Keyboard Action")
}

/// A gesture name from what was searched for, with its first letter capitalized, unless the search
/// was for the action itself.
pub fn name_from_search(query: &str, action_name: &str) -> Option<String> {
    let query = query.trim();
    if query.is_empty() || crate::suggestions::search_matches(action_name, query) {
        return None;
    }
    let mut letters = query.chars();
    let first = letters.next()?;
    Some(first.to_uppercase().chain(letters).collect())
}

pub fn get_default_gesture_name(opt: &EditorActionOption, detail: &str) -> String {
    match &opt.action_type {
        ActionType::Keypress(keys) => {
            // A standard shortcut's keys are fixed, and say nothing its name doesn't.
            if detail.trim().is_empty() || detail.trim() == keys {
                opt.name.clone()
            } else {
                // Its keys as the standard shortcuts' names write them: Ctrl+Y.
                crate::actions::labelled_shortcut_name(&opt.name, detail.trim())
            }
        }
        // A single shortcut is named after its keys, as it always was. Longer actions would make
        // long names, so they keep the action's own.
        ActionType::Keyboard(_) => match parse_steps(detail).as_slice() {
            [KeyStep::Press(keys)] => format!("{} ({})", opt.name, keys),
            _ => opt.name.clone(),
        },
        ActionType::Execute(_) if opt.category == 7 => {
            if detail.trim().is_empty() {
                opt.name.clone()
            } else {
                format!("{} ({})", opt.name, gestos::config::short_command(detail))
            }
        }
        ActionType::Click(_) => {
            if detail.trim().is_empty() {
                opt.name.clone()
            } else {
                format!("{} ({})", opt.name, detail.trim())
            }
        }
        _ => opt.name.clone(),
    }
}
/// The action a gesture already has, as the editor describes actions. One no longer listed, like
/// a desktop shortcut that's gone, is still shown under the gesture's own name.
fn option_for_gesture(
    gesture: &Gesture,
    options: &[EditorActionOption],
) -> Option<EditorActionOption> {
    let action = gesture.actions.first()?;
    // A custom action whose app's own action is gone, as when the app was uninstalled, edits as a
    // custom action, where the app can be changed.
    let custom = || {
        options
            .iter()
            .find(|option| is_custom_action(option))
            .filter(|_| matches!(action, ActionType::Keyboard(_)))
            .cloned()
    };
    // A custom keyboard shortcut whose keys are a predefined one's stays custom, as it was made:
    // it goes by a name other than the predefined one's.
    let custom_shortcut = || {
        options.iter().find(|option| crate::suggestions::is_custom_shortcut(option)).cloned()
    };
    let found = options.iter().find(|option| action_matches(action, option)).cloned();
    let made_custom = matches!(
        (&found, action),
        (Some(option), ActionType::Keypress(_))
            if !crate::suggestions::is_custom_shortcut(option) && option.name != gesture.name
    );
    if made_custom {
        if let Some(option) = custom_shortcut() {
            return Some(option);
        }
    }
    // A template offered only on its apps' pages isn't among the options.
    let template = || match action {
        ActionType::Template(id) => crate::action_templates::find(id).map(|t| crate::action_templates::option(&t)),
        _ => None,
    };
    found
        .or_else(template)
        .or_else(custom)
        .or_else(|| {
            Some(EditorActionOption {
                // The category for Run Command, the one kind whose details depend on it.
                category: 7,
                action_type: action.clone(),
                name: gesture.name.clone(),
                tooltip: String::new(),
            })
        })
}

/// A name no other gesture has, adding a number if needed: "Close Window (2)".
fn unique_gesture_name(name: &str, gestures: &[Gesture], own_id: Option<&str>) -> String {
    let is_taken = |candidate: &str| {
        gestures
            .iter()
            .any(|gesture| gesture.name == candidate && Some(gesture.id.as_str()) != own_id)
    };
    if !is_taken(name) {
        return name.to_string();
    }
    (2..)
        .map(|number| format!("{name} ({number})"))
        .find(|candidate| !is_taken(candidate))
        .unwrap()
}

/// Shows the parameter row only for an action that takes one, in the form that action uses.
fn show_action_parameter(
    opt: &EditorActionOption,
    row: &gtk::Box,
    label: &gtk::Label,
    entry: &gtk::Entry,
    shortcut_display: &gtk::Box,
    record_button: &gtk::Button,
    command_display: &gtk::Box,
    show_steps: &dyn Fn(bool),
) {
    // A custom action has its list of steps instead of the one value other actions take. An app's
    // own action, which only opens it, has none to show. A shortcut's keys are recorded before the
    // editor opens, and are what it is, so they're only shown, in the title.
    let has_steps = is_custom_action(opt);
    let is_keypress = matches!(&opt.action_type, ActionType::Keypress(_));
    // The list's own rows, because hiding only their content leaves an empty strip.
    if let Some(list_row) = row.parent() {
        list_row.set_visible(action_needs_parameter(opt) && !has_steps && !is_keypress);
    }
    show_steps(has_steps);

    // A command is written in a window of its own, with room for it and its variables.
    let is_command = matches!(&opt.action_type, ActionType::Execute(_));
    entry.set_visible(!is_keypress && !is_command);
    shortcut_display.set_visible(is_keypress);
    record_button.set_visible(is_keypress);
    command_display.set_visible(is_command);

    match &opt.action_type {
        ActionType::Keypress(_) => label.set_text("Keys to Send"),
        ActionType::Execute(_) => {
            label.set_text("Command to Execute");
            entry.set_placeholder_text(Some("e.g. firefox"));
        }
        ActionType::Click(_) => {
            label.set_text("Mouse Button");
            entry.set_placeholder_text(Some("e.g. 1 (Left), 2 (Middle), 3 (Right)"));
        }
        _ => {}
    }
}

pub fn open_gesture_editor(state_rc: &Rc<RefCell<AppState>>, gesture: Gesture) {
    open_editor(state_rc, Some(gesture), None, None, &[], None);
}

/// Deletes a gesture right away, with a toast to bring it back. The focus moves to the gesture
/// after it in the list, or before it at the end.
fn delete_gesture(state_rc: &Rc<RefCell<AppState>>, id: &str) {
    let mut state = state_rc.borrow_mut();
    let Some(pos) = state.config.gestures.iter().position(|g| g.id == id) else {
        return;
    };
    let visible = get_visible_gestures(&state, state.open_category);
    let next = visible.iter().position(|g| g.id == id).and_then(|i| {
        visible.get(i + 1).or_else(|| i.checked_sub(1).and_then(|i| visible.get(i)))
    });
    let next_name = next.map(|g| g.name.clone());
    let removed = state.config.gestures.remove(pos);
    if let Err(e) = state.config.save_to_file() {
        println!("Failed to save config to file on delete: {}", e);
    }
    println!("Gesture deleted successfully: {}", removed.name);
    reload_daemon(state.dbus_conn.as_ref(), Some(&crate::ui::main_window::dialog_parent(&state)));

    let toast = adw::Toast::new(&format!(
        "\u{201c}{}\u{201d} deleted",
        glib::markup_escape_text(&removed.name)
    ));
    toast.set_button_label(Some("_Undo"));
    let state_undo = Rc::downgrade(state_rc);
    toast.connect_button_clicked(move |_| {
        if let Some(state_rc) = state_undo.upgrade() {
            restore_gesture(&state_rc, &removed, pos);
        }
    });
    state.toasts.add_toast(toast);
    drop(state);
    refresh_gesture_list(state_rc, next_name.as_deref());
}

/// Puts a deleted gesture back where it was. Its name may have been taken since, and then it gets
/// a number, as a new gesture would.
fn restore_gesture(state_rc: &Rc<RefCell<AppState>>, removed: &Gesture, pos: usize) {
    let mut state = state_rc.borrow_mut();
    if state.config.gestures.iter().any(|g| g.id == removed.id) {
        return;
    }
    let mut gesture = removed.clone();
    gesture.name = unique_gesture_name(&gesture.name, &state.config.gestures, None);
    let name = gesture.name.clone();
    let pos = pos.min(state.config.gestures.len());
    state.config.gestures.insert(pos, gesture);
    if let Err(e) = state.config.save_to_file() {
        println!("Failed to save config to file on undo: {}", e);
    }
    reload_daemon(state.dbus_conn.as_ref(), Some(&crate::ui::main_window::dialog_parent(&state)));
    drop(state);
    refresh_gesture_list(state_rc, Some(&name));
}

/// Opens the editor for a new gesture. Its action comes from the main window's search, and the
/// editor never changes it. A custom keyboard shortcut is recorded first, as its keys are all
/// there is to it. The gesture is drawn next, and the editor only opens once it's set.
pub fn open_gesture_editor_for_action(state_rc: &Rc<RefCell<AppState>>, action: EditorActionOption) {
    open_gesture_editor_in_apps(state_rc, action, &[]);
}

/// The same, for a new gesture already set to work only in some apps.
pub fn open_gesture_editor_in_apps(
    state_rc: &Rc<RefCell<AppState>>,
    action: EditorActionOption,
    apps: &[String],
) {
    if !crate::suggestions::is_custom_shortcut(&action) {
        draw_new_gesture(state_rc, action, None, apps.to_vec());
        return;
    }
    let apps = apps.to_vec();
    let keys = gtk::Entry::new();
    let window = crate::ui::main_window::dialog_parent(&state_rc.borrow());
    let state_set = Rc::downgrade(state_rc);
    let keys_set = keys.clone();
    let on_set: Rc<dyn Fn()> = Rc::new(move || {
        let keys = keys_set.text().to_string();
        if let (Some(state_rc), false) = (state_set.upgrade(), keys.is_empty()) {
            draw_new_gesture(&state_rc, action.clone(), Some(keys), apps.clone());
        }
    });
    open_shortcut_recorder(&window, "", &keys, on_set);
}

/// Opens the editor for a new gesture for one of an app's own shortcuts, in that app only, and
/// named as the app names it.
pub fn open_gesture_editor_for_app_shortcut(state_rc: &Rc<RefCell<AppState>>, app: &str, keys: &str) {
    let custom_shortcut = state_rc
        .borrow()
        .action_options
        .iter()
        .find(|option| crate::suggestions::is_custom_shortcut(option))
        .cloned();
    if let Some(option) = custom_shortcut {
        draw_new_gesture(state_rc, option, Some(keys.to_string()), vec![app.to_string()]);
    }
}

/// Asks for a new gesture's drawing, then opens its editor with it. Cancelling the drawing adds
/// nothing.
fn draw_new_gesture(
    state_rc: &Rc<RefCell<AppState>>,
    action: EditorActionOption,
    keys: Option<String>,
    apps: Vec<String>,
) {
    let window = crate::ui::main_window::dialog_parent(&state_rc.borrow());
    // Named as the editor will name it: as the app names its shortcut, or after the action.
    let detail = keys.as_deref().unwrap_or_default();
    let name = match apps.as_slice() {
        [app] if !detail.is_empty() => gestos::app_shortcuts::title_for(app, detail),
        _ => None,
    }
    .unwrap_or_else(|| get_default_gesture_name(&action, detail));
    let state_set = Rc::downgrade(state_rc);
    let apps_set = apps.clone();
    let on_set: Rc<dyn Fn(Drawing)> = Rc::new(move |drawing| {
        if let Some(state_rc) = state_set.upgrade() {
            open_editor(&state_rc, None, Some(action.clone()), keys.as_deref(), &apps_set, Some(drawing));
        }
    });
    open_gesture_recorder(&window, state_rc, &name, &apps, on_set);
}

fn open_editor(
    state_rc: &Rc<RefCell<AppState>>,
    target_gesture: Option<Gesture>,
    preset_action: Option<EditorActionOption>,
    preset_keys: Option<&str>,
    preset_apps: &[String],
    preset_drawing: Option<Drawing>,
) {
    let state = state_rc.borrow();
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(&crate::ui::main_window::dialog_parent(&state)));
    dialog.set_modal(true);
    dialog.set_destroy_with_parent(true);
    let title_text = if target_gesture.is_some() { "Edit Gesture" } else { "Add Gesture" };
    dialog.set_title(Some(title_text));
    // A little narrower than the main window, so that it reads as being on top of it, and as tall
    // as what's in it, so that nothing in it is out of sight when it opens. Only what doesn't fit
    // on the screen scrolls. It never shrinks below what the trigger and its details need.
    dialog.set_size_request(EDITOR_MIN_WIDTH, EDITOR_MIN_HEIGHT);
    dialog.set_default_size((state.window.width() - 60).clamp(EDITOR_MIN_WIDTH, 640), -1);
    let screen_height = state
        .window
        .surface()
        .and_then(|surface| surface.display().monitor_at_surface(&surface))
        .map(|monitor| monitor.geometry().height())
        .unwrap_or(768);
    // Room for the header bar, the top bar and the window's frame.
    let max_height = (screen_height - 160).max(EDITOR_MIN_HEIGHT);

    crate::ui::components::close_with_keys(&dialog);

    let dialog_header = gtk::HeaderBar::new();
    dialog_header.set_show_title_buttons(false);
    let dialog_title = gtk::Label::new(Some(title_text));
    dialog_title.add_css_class("title");
    dialog_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    dialog_header.set_title_widget(Some(&dialog_title));
    dialog.set_titlebar(Some(&dialog_header));

    // The scrolling area reaches the window's sides, so its scrollbar stays clear of the content.
    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 18);
    main_box.add_css_class("dialog-content");
    main_box.set_margin_top(18);
    main_box.set_margin_bottom(18);
    dialog.set_child(Some(&main_box));

    // Scrollable area for all inputs
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_vscrollbar_policy(gtk::PolicyType::Automatic);
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);
    // The window takes the height of what's in it, and only scrolls past the most it opens at.
    scrolled.set_propagate_natural_height(true);
    scrolled.set_max_content_height(max_height);
    main_box.append(&scrolled);

    let scroll_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    scroll_content.set_margin_bottom(EDITOR_SIDE_MARGIN);
    scrolled.set_child(Some(&scroll_content));

    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some("e.g. Back Action"));
    if let Some(ref g) = target_gesture {
        name_entry.set_text(&g.name);
    }

    let is_name_customized = Rc::new(RefCell::new(target_gesture.is_some()));
    let is_updating_programmatically = Rc::new(RefCell::new(false));

    // Save can only be pressed once the gesture is complete, as GNOME's guidelines have it. This
    // checks again whenever something it depends on changes. It's set once Save exists.
    let check_complete: Rc<RefCell<Rc<dyn Fn()>>> = Rc::new(RefCell::new(Rc::new(|| {})));
    let check = {
        let check_complete = Rc::clone(&check_complete);
        move || {
            let check = Rc::clone(&check_complete.borrow());
            check();
        }
    };

    let cust_clone = Rc::clone(&is_name_customized);
    let prog_clone = Rc::clone(&is_updating_programmatically);
    let check_name = check.clone();
    name_entry.connect_changed(move |entry| {
        if !*prog_clone.borrow() {
            *cust_clone.borrow_mut() = !entry.text().trim().is_empty();
        }
        check_name();
    });

    // --- GESTURE PATH SECTION ---
    let path_section_label = gtk::Label::new(Some("Trigger"));
    path_section_label.set_halign(gtk::Align::Start);
    path_section_label.add_css_class("section-header");
    scroll_content.append(&path_section_label);

    // As wide as the rows below, and as tall as half of that. A new gesture comes drawn already.
    let canvas = crate::ui::gesture_canvas::GestureCanvas::new();
    let drawing = preset_drawing.unwrap_or_else(|| Drawing {
        points: target_gesture.as_ref().map(|g| g.points.clone()).unwrap_or_default(),
        modifiers: target_gesture.as_ref().map(|g| g.modifiers).unwrap_or_default(),
    });
    let recorded_points = Rc::new(RefCell::new(drawing.points));
    let pts_clone = Rc::clone(&recorded_points);
    canvas.set_draw_func(move |_, cr, width, height| {
        let pts = pts_clone.borrow();
        if has_drawing(&pts) {
            draw_gesture_path(cr, &pts, width as f64, height as f64, true, true);
        }
    });
    let modifier_toggles = ModifierKeys::new(drawing.modifiers);

    // The apps the gesture works in, which are part of its trigger, like its modifier keys.
    let gesture_apps = Rc::new(RefCell::new(
        target_gesture.as_ref().map_or_else(|| preset_apps.to_vec(), |g| g.apps.clone()),
    ));

    // Drawing it again opens the window it was drawn in, and only a drawing set there replaces
    // this one. A button of its own below the drawing, which then shows nothing but the gesture.
    let redraw = gtk::Button::with_mnemonic("_Record a New Gesture…");
    redraw.set_halign(gtk::Align::Start);
    // A shape close to another gesture's that can run instead of it.
    let (similar_box, similar_label) = warning_box();
    let show_similar: Rc<dyn Fn()> = {
        let state_rc = Rc::clone(state_rc);
        let own_id = target_gesture.as_ref().map(|g| g.id.clone());
        let apps = Rc::clone(&gesture_apps);
        let points = Rc::clone(&recorded_points);
        let toggles = modifier_toggles.clone();
        let similar_box = similar_box.clone();
        Rc::new(move || {
            let drawing = Drawing { points: points.borrow().clone(), modifiers: toggles.get() };
            let similar =
                similar_gesture(&state_rc.borrow(), &drawing, &apps.borrow(), own_id.as_deref());
            if let Some(name) = &similar {
                similar_label.set_text(&format!("Very similar to \u{201c}{}\u{201d}", name));
            }
            similar_box.set_visible(similar.is_some());
        })
    };
    show_similar();
    {
        let state_rc = Rc::clone(state_rc);
        let dialog = dialog.clone();
        let name_entry = name_entry.clone();
        let apps = Rc::clone(&gesture_apps);
        let points = Rc::clone(&recorded_points);
        let toggles = modifier_toggles.clone();
        let canvas = canvas.clone();
        let check = check.clone();
        let show_similar = Rc::clone(&show_similar);
        redraw.connect_clicked(move |_| {
            let points = Rc::clone(&points);
            let toggles = toggles.clone();
            let canvas = canvas.clone();
            let check = check.clone();
            let show_similar = Rc::clone(&show_similar);
            let on_set: Rc<dyn Fn(Drawing)> = Rc::new(move |drawing| {
                *points.borrow_mut() = drawing.points;
                toggles.set(drawing.modifiers);
                canvas.queue_draw();
                show_similar();
                check();
            });
            // A name being typed is only shown once there's one.
            let name = name_entry.text();
            let name = if name.trim().is_empty() { dialog.title().unwrap_or_default() } else { name };
            open_gesture_recorder(&dialog, &state_rc, &name, &apps.borrow(), on_set);
        });
    }

    // The keys to hold and the shape to draw are one gesture, so the keys show on the drawing.
    let gesture_list = gtk::ListBox::new();
    gesture_list.add_css_class("boxed-list");
    gesture_list.set_selection_mode(gtk::SelectionMode::None);
    let canvas_row = gtk::ListBoxRow::new();
    canvas_row.set_activatable(false);
    let canvas_overlay = gtk::Overlay::new();
    canvas_overlay.set_child(Some(&canvas));
    canvas_overlay.add_overlay(&drawing_corner(&gesture_apps.borrow(), &modifier_toggles));
    canvas_row.set_child(Some(&canvas_overlay));
    gesture_list.append(&canvas_row);
    scroll_content.append(&gesture_list);
    scroll_content.append(&redraw);
    scroll_content.append(&similar_box);
    // The window fits what's in it again as the warning comes and goes, rather than the warning
    // taking room from the rest, which would then scroll out of sight.
    let dialog_fit = dialog.downgrade();
    similar_box.connect_visible_notify(move |_| {
        if let Some(dialog) = dialog_fit.upgrade() {
            dialog.set_default_size(dialog.width(), -1);
        }
    });

    // For actions that need the GNOME extension, which isn't always there.
    let extension_label = gtk::Label::new(None);
    extension_label.add_css_class("dim-label");
    extension_label.set_halign(gtk::Align::Start);
    extension_label.set_xalign(0.0);
    extension_label.set_wrap(true);
    extension_label.set_visible(false);
    scroll_content.append(&extension_label);

    // --- ACTION SETTINGS SECTION ---
    let settings_section_label = gtk::Label::new(Some("Action Details"));
    settings_section_label.set_halign(gtk::Align::Start);
    settings_section_label.add_css_class("section-header");
    scroll_content.append(&settings_section_label);

    let settings_list = gtk::ListBox::new();
    settings_list.add_css_class("boxed-list");
    settings_list.set_selection_mode(gtk::SelectionMode::None);
    scroll_content.append(&settings_list);

    // A custom action can't say what it's for, so it can have a name of its own, in a box above
    // its steps, as it names the whole gesture. Other actions are named after what they do.
    let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    name_row.add_css_class("settings-row");
    let name_label = gtk::Label::with_mnemonic("_Name");
    name_label.set_mnemonic_widget(Some(&name_entry));
    name_row.append(&name_label);
    name_entry.set_hexpand(true);
    name_row.append(&name_entry);
    let name_list_row = gtk::ListBoxRow::new();
    name_list_row.set_activatable(false);
    name_list_row.set_child(Some(&name_row));
    let name_list = gtk::ListBox::new();
    name_list.add_css_class("boxed-list");
    name_list.set_selection_mode(gtk::SelectionMode::None);
    name_list.append(&name_list_row);
    name_list.set_visible(false);
    scroll_content.append(&name_list);

    // What the gesture does comes first, with its note about the extension, and then what
    // triggers it.
    scroll_content.reorder_child_after(&settings_section_label, None::<&gtk::Widget>);
    scroll_content.reorder_child_after(&name_list, Some(&settings_section_label));
    scroll_content.reorder_child_after(&settings_list, Some(&name_list));
    scroll_content.reorder_child_after(&extension_label, Some(&settings_list));

    // Each part is as wide as the main window's lists, centered, with the same margins. Text is
    // wrapped to that width, and starts at its left.
    extension_label.set_max_width_chars(1);
    let mut part = scroll_content.first_child();
    while let Some(widget) = part {
        widget.set_width_request(EDITOR_CONTENT_WIDTH);
        widget.set_halign(gtk::Align::Center);
        widget.set_margin_start(EDITOR_SIDE_MARGIN);
        widget.set_margin_end(EDITOR_SIDE_MARGIN);
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            label.set_xalign(0.0);
        }
        part = widget.next_sibling();
    }




    // Row 4: Action Details Parameter
    let action_details_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    action_details_row.add_css_class("settings-row");

    let action_details_label = gtk::Label::new(None);
    action_details_label.set_halign(gtk::Align::Start);
    action_details_row.append(&action_details_label);

    let action_details_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    action_details_spacer.set_hexpand(true);
    action_details_row.append(&action_details_spacer);

    let entry_container = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    entry_container.set_halign(gtk::Align::End);

    let action_details_entry = gtk::Entry::new();
    action_details_entry.set_size_request(160, -1);
    entry_container.append(&action_details_entry);

    // Add shortcut display box for keycaps representation
    let shortcut_display_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    shortcut_display_box.set_halign(gtk::Align::End);
    shortcut_display_box.set_valign(gtk::Align::Center);
    shortcut_display_box.set_visible(false);
    entry_container.append(&shortcut_display_box);

    let record_btn = gtk::Button::from_icon_name("media-record-symbolic");
    record_btn.set_tooltip_text(Some("Record keys"));
    entry_container.append(&record_btn);

    // A command shows on one line, with a button to the window it's written in. Whether it gets
    // the variables is kept here, as the entry keeps the command.
    let variables_check = gtk::CheckButton::new();
    let command_display = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    command_display.set_hexpand(true);
    command_display.set_visible(false);
    let command_shown = crate::ui::command_editor::command_preview();
    command_display.append(&command_shown);
    let edit_command = crate::ui::command_editor::edit_command_button();
    command_display.append(&edit_command);
    entry_container.append(&command_display);
    {
        let dialog = dialog.clone();
        let entry = action_details_entry.clone();
        let variables = variables_check.clone();
        edit_command.connect_clicked(move |_| {
            let command = gestos::command_variables::ShellCommand::new(entry.text(), variables.is_active());
            let entry = entry.clone();
            let variables = variables.clone();
            let on_set: Rc<dyn Fn(gestos::command_variables::ShellCommand)> = Rc::new(move |command| {
                variables.set_active(command.variables);
                entry.set_text(&command.text);
            });
            crate::ui::command_editor::open_command_editor(&dialog, &command, on_set);
        });
    }
    let shown_from = command_shown.clone();
    action_details_entry.connect_changed(move |entry| {
        crate::ui::command_editor::show_command(&shown_from, &entry.text());
    });
    crate::ui::command_editor::show_command(&command_shown, "");

    action_details_row.append(&entry_container);
    settings_list.append(&action_details_row);

    // Edits the details entry, which holds a custom action's steps as the gesture saves them.
    let (show_steps, load_steps) =
        append_steps_rows(&settings_list, &dialog, &action_details_entry);
    show_steps(false);
    // An app's own action can do more, as a custom action. That's optional, so it's a quiet button
    // at the end, rather than a part of its own.
    let customize_btn = gtk::Button::with_mnemonic("_Customize Action…");
    customize_btn.add_css_class("flat");
    customize_btn.set_halign(gtk::Align::Center);
    customize_btn.set_visible(false);
    scroll_content.append(&customize_btn);

    let selected_action: Rc<RefCell<Option<EditorActionOption>>> = Rc::new(RefCell::new(None));

    let update_default_name = Rc::new({
        let name_entry = name_entry.clone();
        let action_details_entry = action_details_entry.clone();
        let selected_action = Rc::clone(&selected_action);
        let is_name_customized = Rc::clone(&is_name_customized);
        let is_updating_programmatically = Rc::clone(&is_updating_programmatically);
        let apps = Rc::clone(&gesture_apps);

        move || {
            if *is_name_customized.borrow() {
                return;
            }
            if let Some(opt) = selected_action.borrow().as_ref() {
                let detail = action_details_entry.text().to_string();
                // A shortcut for one app is named as that app names it, if it lists those keys.
                let app_title = match apps.borrow().as_slice() {
                    [app] if crate::suggestions::is_custom_shortcut(opt) && !detail.trim().is_empty() => {
                        gestos::app_shortcuts::title_for(app, detail.trim())
                    }
                    _ => None,
                };
                let default_name =
                    app_title.unwrap_or_else(|| get_default_gesture_name(opt, &detail));
                
                *is_updating_programmatically.borrow_mut() = true;
                name_entry.set_text(&default_name);
                *is_updating_programmatically.borrow_mut() = false;
            }
        }
    });

    // Setup record button to open the "Set Shortcut" modal dialog
    let entry_click = action_details_entry.clone();
    let dialog_parent = dialog.clone();
    let name_entry_click = name_entry.clone();
    let udn_click = Rc::clone(&update_default_name);

    record_btn.connect_clicked(move |_| {
        let gesture_name = name_entry_click.text().to_string();
        open_shortcut_recorder(
            &dialog_parent,
            if gesture_name.trim().is_empty() {
                "Gesture"
            } else {
                &gesture_name
            },
            &entry_click,
            Rc::clone(&udn_click) as Rc<dyn Fn()>,
        );
    });

    // Helper to update shortcut display box with keycaps
    let update_shortcut_display = {
        let entry = action_details_entry.clone();
        let display_box = shortcut_display_box.clone();
        Rc::new(move || {
            // Clear previous children
            while let Some(child) = display_box.first_child() {
                display_box.remove(&child);
            }

            let text = entry.text().to_string();
            if text.trim().is_empty() {
                let label = gtk::Label::new(Some("None"));
                label.set_opacity(0.5);
                display_box.append(&label);
            } else {
                let parts: Vec<&str> = text.split('+').filter(|s| !s.trim().is_empty()).collect();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        let plus = gtk::Label::new(Some("+"));
                        plus.set_opacity(0.6);
                        display_box.append(&plus);
                    }
                    // As the recorder shows it.
                    let part = if *part == "Control" { "Ctrl" } else { part };
                    let label = gtk::Label::new(Some(part));
                    label.add_css_class("keycap");
                    display_box.append(&label);
                }
            }
        })
    };

    let udn_clone4 = Rc::clone(&update_default_name);
    let usd_clone_change = Rc::clone(&update_shortcut_display);
    let details_loaded = Rc::new(Cell::new(false));
    let details_loaded_changed = Rc::clone(&details_loaded);
    let name_follows_details = Rc::clone(&is_name_customized);
    let name_entry_details = name_entry.clone();
    let action_details = Rc::clone(&selected_action);
    let check_variables = check.clone();
    variables_check.connect_toggled(move |_| check_variables());
    let check_details = check.clone();
    action_details_entry.connect_changed(move |_| {
        check_details();
        // Loading a gesture isn't a change, but editing its keys or command changes its action,
        // and a name made from them follows. A custom action's own name stays, as it has a field
        // for it: only one still made from the action follows its steps.
        if details_loaded_changed.get() {
            let own_name = action_details.borrow().as_ref().is_some_and(|opt| {
                matches!(opt.action_type, ActionType::Keyboard(_))
                    && !is_automatic_name(&name_entry_details.text(), &opt.name)
            });
            *name_follows_details.borrow_mut() = own_name;
        }
        udn_clone4();
        usd_clone_change();
    });

    let initial_opt = match &target_gesture {
        Some(gesture) => {
            let option = option_for_gesture(gesture, &state_rc.borrow().action_options);
            match gesture.actions.first() {
                // A shortcut that isn't a standard one opens as a custom action of one step.
                Some(ActionType::Keypress(combo)) => {
                    let as_steps = option
                        .as_ref()
                        .is_some_and(|o| matches!(o.action_type, ActionType::Keyboard(_)));
                    if as_steps {
                        let steps = [KeyStep::Press(combo.clone())];
                        action_details_entry.set_text(&format_steps(&steps));
                    } else {
                        action_details_entry.set_text(combo);
                    }
                }
                Some(ActionType::Keyboard(steps)) => {
                    action_details_entry.set_text(&format_steps(steps));
                }
                Some(ActionType::Execute(command))
                    if option.as_ref().is_some_and(action_needs_parameter) =>
                {
                    action_details_entry.set_text(command);
                    variables_check.set_active(command.variables);
                }
                Some(ActionType::Click(button)) => {
                    let button = button.map(|b| b.to_string()).unwrap_or_default();
                    action_details_entry.set_text(&button);
                }
                _ => {}
            }
            option
        }
        None => {
            if let Some(keys) = preset_keys {
                action_details_entry.set_text(keys);
            }
            preset_action.clone()
        }
    };

    // Nothing to fill in until an action is known, such as one that's no longer available
    if let Some(list_row) = action_details_row.parent() {
        list_row.set_visible(false);
    }
    if let Some(opt) = initial_opt {
        *selected_action.borrow_mut() = Some(opt.clone());
        name_list.set_visible(has_own_name(&opt));
        show_action_parameter(
            &opt,
            &action_details_row,
            &action_details_label,
            &action_details_entry,
            &shortcut_display_box,
            &record_btn,
            &command_display,
            &*show_steps,
        );
        // A chosen action is all a new gesture's default name needs.
        if preset_action.is_some() {
            update_default_name();
        }
        // A new custom action is named after what was searched for, as that's what it's for:
        // searching "new tab" and creating one names it "New tab". A search that found the action
        // by its own name says nothing about what it's for.
        let query = state_rc.borrow().search_entry.text();
        if let Some(name) = preset_action
            .as_ref()
            .filter(|preset| has_own_name(preset))
            .and_then(|preset| name_from_search(&query, &preset.name))
        {
            name_entry.set_text(&name);
        }
        // A shortcut is named with its keys, which it can't be told from otherwise.
        let title = match &opt.action_type {
            ActionType::Keypress(_) => get_default_gesture_name(&opt, &action_details_entry.text()),
            _ => opt.name.clone(),
        };
        dialog.set_title(Some(&title));
        dialog_title.set_text(&title);
        let status = state_rc.borrow().extension_status.clone();
        // An app's gesture also needs the app in use to be known.
        let note = crate::ui::main_window::extension_note(&opt.action_type, &status).or_else(|| {
            let for_app = !gesture_apps.borrow().is_empty();
            for_app
                .then(|| {
                    crate::ui::app_picker::apps_note(
                        &status,
                        gestos::active_app::can_tell_without_extension(),
                    )
                })
                .flatten()
        });
        extension_label.set_text(note.unwrap_or_default());
        extension_label.set_visible(note.is_some());
        // Most actions have nothing to fill in, which leaves this section empty. An app's own
        // action can become a custom action, to do more once the app is open, and so can a
        // template's, to change its steps.
        customize_btn.set_visible(customizable_steps(&opt.action_type).is_some());
        customize_btn.set_tooltip_text(
            matches!(opt.action_type, ActionType::Template(_))
                .then_some("Change its steps. The gesture will no longer get updates to this action."),
        );
        let has_details =
            action_needs_parameter(&opt) && !matches!(opt.action_type, ActionType::Keypress(_));
        settings_section_label.set_visible(has_details);
        settings_list.set_visible(has_details);
    }

    // Turns an app's own action into a custom action with its one step, which more are added to,
    // keeping the gesture's name.
    {
        let custom = state_rc
            .borrow()
            .action_options
            .iter()
            .find(|option| is_custom_action(option))
            .cloned();
        let selected_action = Rc::clone(&selected_action);
        let action_details_row = action_details_row.clone();
        let action_details_label = action_details_label.clone();
        let action_details_entry = action_details_entry.clone();
        let shortcut_display_box = shortcut_display_box.clone();
        let record_btn = record_btn.clone();
        let show_steps = Rc::clone(&show_steps);
        let load_steps = Rc::clone(&load_steps);
        let name_list = name_list.clone();
        let customize_btn_click = customize_btn.clone();
        let settings_section_label = settings_section_label.clone();
        let settings_list = settings_list.clone();
        let dialog = dialog.clone();
        let dialog_title = dialog_title.clone();
        let check = check.clone();
        customize_btn.connect_clicked(move |_| {
            let Some(custom) = custom.clone() else { return };
            let steps = selected_action.borrow().as_ref().and_then(|opt| customizable_steps(&opt.action_type));
            let Some(steps) = steps else { return };
            *selected_action.borrow_mut() = Some(custom.clone());
            action_details_entry.set_text(&format_steps(&steps));
            load_steps();
            customize_btn_click.set_visible(false);
            settings_section_label.set_visible(true);
            settings_list.set_visible(true);
            name_list.set_visible(true);
            show_action_parameter(
                &custom,
                &action_details_row,
                &action_details_label,
                &action_details_entry,
                &shortcut_display_box,
                &record_btn,
                &command_display,
                &*show_steps,
            );
            dialog.set_title(Some(&custom.name));
            dialog_title.set_text(&custom.name);
            check();
        });
    }

    load_steps();
    details_loaded.set(true);

    let usd_init = Rc::clone(&update_shortcut_display);
    usd_init(); // Set initial keycaps if keys exist

    // Save and Cancel buttons
    let cancel_btn = gtk::Button::with_mnemonic("_Cancel");
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.destroy();
    });
    dialog_header.pack_start(&cancel_btn);

    let save_btn = gtk::Button::with_mnemonic("_Save");
    save_btn.add_css_class("suggested-action");
    save_btn.set_receives_default(true);

    // What the gesture still needs before it can be saved, if anything.
    let missing: Rc<dyn Fn() -> Option<&'static str>> = {
        let recorded_points = Rc::clone(&recorded_points);
        let selected_action = Rc::clone(&selected_action);
        let action_details_entry = action_details_entry.clone();
        let name_entry = name_entry.clone();
        let is_name_customized = Rc::clone(&is_name_customized);
        let state_rc = Rc::clone(state_rc);
        let own_id = target_gesture.as_ref().map(|g| g.id.clone());
        Rc::new(move || {
            if recorded_points.borrow().len() < 2 {
                return Some("Draw the gesture first");
            }
            let selected = selected_action.borrow();
            let Some(opt) = selected.as_ref() else { return Some("Choose an action first") };
            let detail = action_details_entry.text();
            match &opt.action_type {
                ActionType::Keyboard(steps) if steps.is_empty() && parse_steps(&detail).is_empty() => {
                    return Some("Add a step to the custom action first");
                }
                ActionType::Execute(_) if opt.category == 7 && detail.trim().is_empty() => {
                    return Some("Type the command to run first");
                }
                ActionType::Keypress(keys) if keys.is_empty() && detail.trim().is_empty() => {
                    return Some("Record the shortcut first");
                }
                _ => {}
            }
            let name = name_entry.text();
            let taken = *is_name_customized.borrow()
                && state_rc.borrow().config.gestures.iter().any(|g| {
                    g.name == name.as_str() && Some(&g.id) != own_id.as_ref()
                });
            taken.then_some("Another gesture already has this name")
        })
    };
    {
        let save_btn = save_btn.clone();
        let name_entry = name_entry.clone();
        let missing = Rc::clone(&missing);
        *check_complete.borrow_mut() = Rc::new(move || {
            let missing = missing();
            save_btn.set_sensitive(missing.is_none());
            save_btn.set_tooltip_text(missing);
            if missing == Some("Another gesture already has this name") {
                name_entry.add_css_class("error");
            } else {
                name_entry.remove_css_class("error");
            }
        });
    }
    check();
    // Pressing Enter in a field saves, which an incomplete gesture can't: the system's alert sound
    // says so, as the user set it up.
    for entry in [&name_entry, &action_details_entry] {
        entry.set_activates_default(true);
        let missing = Rc::clone(&missing);
        entry.connect_activate(move |entry| {
            if missing().is_some() {
                entry.error_bell();
            }
        });
    }

    let state_clone = Rc::clone(state_rc);
    let is_edit = target_gesture.is_some();
    let target_id = target_gesture.as_ref().map(|g| g.id.clone());
    let dialog_clone2 = dialog.clone();
    let selected_action_save = Rc::clone(&selected_action);

    let toggles_save = modifier_toggles.clone();
    let apps_save = Rc::clone(&gesture_apps);
    let is_name_customized_save = Rc::clone(&is_name_customized);
    save_btn.connect_clicked(move |_| {
        let modifiers = toggles_save.get();
        let name = name_entry.text().to_string();

        let pts = recorded_points.borrow();
        if pts.len() < 2 {
            println!("Gesture save failed: Not enough points drawn.");
            return;
        }

        // Convert recorded points back to string coords representation
        let raw_movement = pts
            .iter()
            .map(|p| format!("{},{}", p.x as i32, p.y as i32))
            .collect::<Vec<_>>()
            .join(" ");

        let opt = if let Some(opt) = selected_action_save.borrow().as_ref() {
            opt.clone()
        } else {
            println!("Gesture save failed: No action selected.");
            return;
        };
        let detail = action_details_entry.text().to_string();
        // A name left empty goes back to the one made from the action.
        let name = if name.trim().is_empty() {
            *is_name_customized_save.borrow_mut() = false;
            get_default_gesture_name(&opt, &detail)
        } else {
            name
        };

        let action = match &opt.action_type {
            // One shortcut is saved as one, as standard shortcuts and older gestures are. An app's
            // own action has its one step already, and is saved as it is.
            ActionType::Keyboard(steps) if steps.is_empty() => match parse_steps(&detail)
                .as_slice()
            {
                [] => {
                    println!("Gesture save failed: The custom action has no steps.");
                    return;
                }
                [KeyStep::Press(keys)] => ActionType::Keypress(keys.clone()),
                steps => ActionType::Keyboard(steps.to_vec()),
            },
            ActionType::Execute(_) if opt.category == 7 => {
                ActionType::Execute(gestos::command_variables::ShellCommand::new(detail, variables_check.is_active()))
            }
            // The keys recorded, or a standard shortcut's own, as a new gesture for one has none
            // recorded.
            ActionType::Keypress(keys) => {
                let keys = if detail.trim().is_empty() { keys.trim() } else { detail.trim() };
                if keys.is_empty() {
                    println!("Gesture save failed: The shortcut has no keys.");
                    return;
                }
                ActionType::Keypress(keys.to_string())
            }
            ActionType::Click(_) => {
                let btn = if detail.trim().is_empty() {
                    None
                } else {
                    detail.trim().parse::<i32>().ok()
                };
                ActionType::Click(btn)
            }
            other => other.clone(),
        };

        let mut state = state_clone.borrow_mut();
        // A generated name yields to one that's taken. Only a hand-written name is an error.
        let name = if *is_name_customized_save.borrow() {
            name
        } else {
            unique_gesture_name(&name, &state.config.gestures, target_id.as_deref())
        };
        if is_edit {
            let lookup_id = target_id.as_ref().unwrap();
            if let Some(pos) = state
                .config
                .gestures
                .iter()
                .position(|g| g.id == *lookup_id)
            {
                let lookup_name = state.config.gestures[pos].name.clone();
                if name != lookup_name {
                    if state.config.gestures.iter().any(|g| g.name == name) {
                        println!(
                            "Gesture save failed: A gesture with the name '{}' already exists.",
                            name
                        );
                        return;
                    }
                    state.config.gestures[pos].modifiers = modifiers;
                    state.config.gestures[pos].apps = apps_save.borrow().clone();
                    state.config.gestures[pos].name = name.clone();
                    state.config.gestures[pos].raw_movement = raw_movement;
                    state.config.gestures[pos].points = pts.clone();
                    state.config.gestures[pos].actions = vec![action];
                    println!("Gesture renamed successfully: {} -> {}", lookup_name, name);
                } else {
                    state.config.gestures[pos].modifiers = modifiers;
                    state.config.gestures[pos].apps = apps_save.borrow().clone();
                    state.config.gestures[pos].raw_movement = raw_movement;
                    state.config.gestures[pos].points = pts.clone();
                    state.config.gestures[pos].actions = vec![action];
                    println!("Gesture modified successfully: {}", name);
                }
            } else {
                println!(
                    "Gesture modification failed: Could not find gesture with ID '{}'",
                    lookup_id
                );
            }
        } else {
            // Check if name conflict
            if state.config.gestures.iter().any(|g| g.name == name) {
                println!(
                    "Gesture save failed: A gesture with the name '{}' already exists.",
                    name
                );
                return;
            }
            println!("Gesture added successfully: {}", name);
            let new_id = generate_unique_id();
            state.config.gestures.push(Gesture {
                id: new_id,
                name: name.clone(),
                raw_movement,
                points: pts.clone(),
                actions: vec![action],
                is_custom: true,
                is_modified: false,
                is_deleted: false,
                modifiers,
                apps: apps_save.borrow().clone(),
            });
            state.newly_added_gestures.push(name.clone());
        }

        if let Err(e) = state.config.save_to_file() {
            println!("Failed to save configuration to file: {}", e);
        }
        reload_daemon(state.dbus_conn.as_ref(), Some(&dialog_clone2));
        drop(state);

        dialog_clone2.destroy();
        // A gesture made from the main page is shown where it's kept, as the search that found
        // its action may hide it. One made in a category's window shows there already.
        let made_on_main_page = !is_edit && state_clone.borrow().open_category.is_none();
        if made_on_main_page {
            crate::ui::main_window::show_new_gesture(&state_clone, &name);
        } else {
            refresh_gesture_list(&state_clone, Some(&name));
        }
    });
    dialog_header.pack_end(&save_btn);

    if let Some(ref gest) = target_gesture {
        // It doesn't ask first, as a toast offers to undo it. It comes last in the content, as
        // GNOME's own apps place their destructive buttons, with room above it to set it apart.
        let delete_btn = gtk::Button::with_mnemonic("_Delete Gesture");
        delete_btn.add_css_class("destructive-action");
        delete_btn.set_halign(gtk::Align::Center);
        delete_btn.set_margin_top(18);

        let state_clone = Rc::clone(state_rc);
        let target_id = gest.id.clone();
        let dialog_clone3 = dialog.clone();
        delete_btn.connect_clicked(move |_| {
            delete_gesture(&state_clone, &target_id);
            dialog_clone3.destroy();
        });
        scroll_content.append(&delete_btn);
    }

    dialog.set_default_widget(Some(&save_btn));
    dialog.present();
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_custom_actions_own_name_is_kept() {
        let action = "Advanced Action";
        assert!(is_automatic_name("Advanced Action", action));
        assert!(is_automatic_name("Advanced Action (Control+T)", action));
        assert!(is_automatic_name("Custom Action (Control+T)", action));
        assert!(is_automatic_name("Custom Keyboard Action (Alt+F4)", action));
        assert!(is_automatic_name("  ", action));
        assert!(!is_automatic_name("New tab", action));
    }

    #[test]
    fn a_new_custom_action_is_named_after_the_search() {
        let action = "Advanced Action";
        assert_eq!(name_from_search("  new tab ", action).as_deref(), Some("New tab"));
        assert_eq!(name_from_search("advanced", action), None);
        assert_eq!(name_from_search("advanced act", action), None);
        assert_eq!(name_from_search("", action), None);
    }
    use super::*;
    use gestos::modifiers::Modifiers;

    fn gesture(id: &str, name: &str) -> Gesture {
        Gesture {
            id: id.to_string(),
            name: name.to_string(),
            raw_movement: String::new(),
            points: Vec::new(),
            actions: Vec::new(),
            is_custom: true,
            is_modified: false,
            is_deleted: false,
            modifiers: Modifiers::default(),
            apps: Vec::new(),
        }
    }

    fn option(category: usize, action_type: ActionType, name: &str) -> EditorActionOption {
        EditorActionOption {
            category,
            action_type,
            name: name.to_string(),
            tooltip: String::new(),
        }
    }

    #[test]
    fn a_gesture_finds_its_action_in_the_list() {
        let mut close = gesture("a", "My name");
        close.actions = vec![ActionType::Gnome("close".to_string())];
        let options = [option(6, ActionType::Gnome("close".to_string()), "Close window")];
        assert_eq!(option_for_gesture(&close, &options).unwrap().name, "Close window");
    }

    #[test]
    fn an_unlisted_action_is_still_shown_under_the_gesture_name() {
        let mut gone = gesture("a", "My name");
        gone.actions = vec![ActionType::Gnome("gone".to_string())];
        let found = option_for_gesture(&gone, &[]).unwrap();
        assert_eq!(found.name, "My name");
        assert_eq!(found.action_type, gone.actions[0]);
        assert!(!action_needs_parameter(&found));

        let mut command = gesture("b", "Editor");
        command.actions = vec![ActionType::Execute("gedit".into())];
        assert!(action_needs_parameter(&option_for_gesture(&command, &[]).unwrap()));
    }

    #[test]
    fn a_custom_shortcut_with_predefined_keys_stays_custom() {
        let options = crate::actions::get_static_action_options();
        let mut undo = gesture("a", "Undo (Ctrl+Z)");
        undo.actions = vec![ActionType::Keypress("Control+Z".to_string())];
        assert_eq!(option_for_gesture(&undo, &options).unwrap().name, "Undo (Ctrl+Z)");
        let mut own = gesture("b", "Take it back");
        own.actions = undo.actions.clone();
        assert_eq!(option_for_gesture(&own, &options).unwrap().name, "Custom Keyboard Shortcut");
    }

    #[test]
    fn a_gesture_without_actions_has_nothing_to_show() {
        assert!(option_for_gesture(&gesture("a", "Empty"), &[]).is_none());
    }

    #[test]
    fn a_free_name_is_kept() {
        let gestures = [gesture("a", "Back")];
        assert_eq!(unique_gesture_name("Close", &gestures, None), "Close");
    }

    #[test]
    fn a_taken_name_gets_the_first_free_number() {
        let gestures = [gesture("a", "Close"), gesture("b", "Close (2)")];
        assert_eq!(unique_gesture_name("Close", &gestures, None), "Close (3)");
    }

    #[test]
    fn a_gesture_may_keep_its_own_name() {
        let gestures = [gesture("a", "Close"), gesture("b", "Back")];
        assert_eq!(unique_gesture_name("Close", &gestures, Some("a")), "Close");
        assert_eq!(unique_gesture_name("Back", &gestures, Some("a")), "Back (2)");
    }
}
