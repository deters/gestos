use gtk::prelude::*;
use gtk::glib;
use gestos::protractor::match_gesture;
use gtk4 as gtk;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use gestos::config::{ActionType, Gesture};
use gestos::modifiers::Modifiers;
use gestos::config::generate_unique_id;
use crate::state::{AppState, EditorActionOption};
use crate::actions::{action_matches, action_needs_parameter};
use crate::ui::shortcut_recorder::open_shortcut_recorder;
use crate::ui::keyboard_steps::build_steps_editor;
use crate::ui::app_picker::append_apps_rows;
use gestos::key_steps::{format_steps, parse_steps, KeyStep};
use crate::ui::components::{draw_gesture_path, show_confirm_dialog};
use crate::daemon::reload_daemon;
use crate::ui::main_window::refresh_gesture_list;
use gestos::protractor::Point2D;

// The width of the editor's content, as the main window's lists have, and the margins around it.
const EDITOR_CONTENT_WIDTH: i32 = 500;
const EDITOR_SIDE_MARGIN: i32 = 24;
// The smallest the gesture editor gets: its content and margins wide, with room for the drawing
// and a few rows below it.
const EDITOR_MIN_WIDTH: i32 = EDITOR_CONTENT_WIDTH + 2 * EDITOR_SIDE_MARGIN;
const EDITOR_MIN_HEIGHT: i32 = 480;

/// Whether a custom action's name is one made from the action, rather than one the user gave it.
/// Names from before the action was renamed count too.
fn is_automatic_name(name: &str, action_name: &str) -> bool {
    let name = name.trim();
    name.is_empty() || name.starts_with(action_name) || name.starts_with("Custom Keyboard Action")
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
                format!("{} ({})", opt.name, detail.trim())
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
                format!("{} ({})", opt.name, detail.trim())
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
    options
        .iter()
        .find(|option| action_matches(action, option))
        .cloned()
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

/// The modifier keys a gesture needs. Holding them while drawing sets them too.
#[derive(Clone)]
struct ModifierToggles {
    ctrl: gtk::ToggleButton,
    alt: gtk::ToggleButton,
    shift: gtk::ToggleButton,
    super_key: gtk::ToggleButton,
}

impl ModifierToggles {
    fn new(initial: Modifiers) -> Self {
        let toggles = ModifierToggles {
            ctrl: gtk::ToggleButton::with_label("Ctrl"),
            alt: gtk::ToggleButton::with_label("Alt"),
            shift: gtk::ToggleButton::with_label("Shift"),
            super_key: gtk::ToggleButton::with_label("Super"),
        };
        toggles.set(initial);
        toggles
    }

    fn set(&self, modifiers: Modifiers) {
        self.ctrl.set_active(modifiers.ctrl);
        self.alt.set_active(modifiers.alt);
        self.shift.set_active(modifiers.shift);
        self.super_key.set_active(modifiers.super_key);
    }

    fn get(&self) -> Modifiers {
        Modifiers {
            ctrl: self.ctrl.is_active(),
            alt: self.alt.is_active(),
            shift: self.shift.is_active(),
            super_key: self.super_key.is_active(),
        }
    }

    fn buttons(&self) -> [&gtk::ToggleButton; 4] {
        [&self.ctrl, &self.alt, &self.shift, &self.super_key]
    }
}

fn held_modifiers(state: gtk::gdk::ModifierType) -> Modifiers {
    Modifiers {
        ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
        alt: state.contains(gtk::gdk::ModifierType::ALT_MASK),
        shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
        super_key: state.contains(gtk::gdk::ModifierType::SUPER_MASK),
    }
}

/// Shows the parameter row only for an action that takes one, in the form that action uses.
fn show_action_parameter(
    opt: &EditorActionOption,
    row: &gtk::Box,
    label: &gtk::Label,
    entry: &gtk::Entry,
    shortcut_display: &gtk::Box,
    record_button: &gtk::Button,
    steps_editor: &gtk::Widget,
) {
    // A custom action has its list of steps instead of the one value other actions take.
    let has_steps = matches!(&opt.action_type, ActionType::Keyboard(_));
    // The list's own rows, because hiding only their content leaves an empty strip.
    if let Some(list_row) = row.parent() {
        list_row.set_visible(action_needs_parameter(opt) && !has_steps);
    }
    if let Some(list_row) = steps_editor.parent() {
        list_row.set_visible(has_steps);
    }

    let is_keypress = matches!(&opt.action_type, ActionType::Keypress(_));
    entry.set_visible(!is_keypress);
    shortcut_display.set_visible(is_keypress);
    record_button.set_visible(is_keypress);

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
    open_editor(state_rc, Some(gesture), None);
}

/// Opens the editor for a new gesture. Its action comes from the main window's search, and the
/// editor never changes it.
pub fn open_gesture_editor_for_action(state_rc: &Rc<RefCell<AppState>>, action: EditorActionOption) {
    open_editor(state_rc, None, Some(action));
}

fn open_editor(
    state_rc: &Rc<RefCell<AppState>>,
    target_gesture: Option<Gesture>,
    preset_action: Option<EditorActionOption>,
) {
    let state = state_rc.borrow();
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(&state.window));
    dialog.set_modal(true);
    dialog.set_destroy_with_parent(true);
    let title_text = if target_gesture.is_some() { "Edit Gesture" } else { "Add Gesture" };
    dialog.set_title(Some(title_text));
    // A little narrower than the main window, so that it reads as being on top of it, and as tall
    // as what's in it, up to a fifth taller than that. It doesn't grow along with a maximized
    // window, nor shrink below what the trigger and its details need.
    dialog.set_size_request(EDITOR_MIN_WIDTH, EDITOR_MIN_HEIGHT);
    dialog.set_default_size((state.window.width() - 60).clamp(EDITOR_MIN_WIDTH, 640), -1);
    let max_height = ((state.window.height() - 60) * 6 / 5).clamp(EDITOR_MIN_HEIGHT, 672);

    let esc_controller = gtk::EventControllerKey::new();
    let dialog_esc = dialog.clone();
    esc_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            dialog_esc.destroy();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    dialog.add_controller(esc_controller);

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
    scrolled.set_child(Some(&scroll_content));

    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some("e.g. Back Action"));
    if let Some(ref g) = target_gesture {
        name_entry.set_text(&g.name);
    }

    let is_name_customized = Rc::new(RefCell::new(target_gesture.is_some()));
    let is_updating_programmatically = Rc::new(RefCell::new(false));

    let cust_clone = Rc::clone(&is_name_customized);
    let prog_clone = Rc::clone(&is_updating_programmatically);
    name_entry.connect_changed(move |entry| {
        if *prog_clone.borrow() {
            return;
        }
        *cust_clone.borrow_mut() = !entry.text().trim().is_empty();
    });

    // --- GESTURE PATH SECTION ---
    let path_section_label = gtk::Label::new(Some("Trigger"));
    path_section_label.set_halign(gtk::Align::Start);
    path_section_label.add_css_class("section-header");
    scroll_content.append(&path_section_label);

    // As wide as the rows below, and as tall as half of that.
    let canvas = crate::ui::gesture_canvas::GestureCanvas::new();
    let recorded_points: Rc<RefCell<Vec<Point2D>>> = Rc::new(RefCell::new(
        target_gesture
            .as_ref()
            .map(|g| g.points.clone())
            .unwrap_or_default(),
    ));
    let is_recording = Rc::new(RefCell::new(false));

    let pts_clone = Rc::clone(&recorded_points);
    let rec_clone = Rc::clone(&is_recording);
    canvas.set_draw_func(move |_, cr, width, height| {
        let pts = pts_clone.borrow();
        let active = *rec_clone.borrow();
        draw_gesture_path(cr, &pts, width as f64, height as f64, true, !active);
    });

    // GestureDrag controller for recording drawing
    let drag = gtk::GestureDrag::new();

    let conflict_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    conflict_box.add_css_class("warning-box");
    conflict_box.set_visible(false);
    conflict_box.set_margin_top(4);
    conflict_box.set_margin_bottom(4);

    let conflict_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
    conflict_icon.set_icon_size(gtk::IconSize::Normal);
    conflict_box.append(&conflict_icon);

    let conflict_label = gtk::Label::new(None);
    conflict_label.set_halign(gtk::Align::Start);
    conflict_label.add_css_class("warning-label");
    conflict_label.set_wrap(true);
    // Wrapped to the box's width, rather than asking for room for all of it on one line.
    conflict_label.set_max_width_chars(1);
    conflict_box.append(&conflict_label);

    let modifier_toggles = ModifierToggles::new(
        target_gesture
            .as_ref()
            .map(|gesture| gesture.modifiers)
            .unwrap_or_default(),
    );

    // Shown over the drawing area while there's no gesture in it.
    let draw_hint = gtk::Label::new(Some("Draw the gesture here"));
    draw_hint.add_css_class("dim-label");
    draw_hint.set_can_target(false);
    draw_hint.set_visible(recorded_points.borrow().len() < 2);

    let canvas_clone = canvas.clone();
    let pts_drag = Rc::clone(&recorded_points);
    let rec_drag = Rc::clone(&is_recording);
    let conflict_box_begin = conflict_box.clone();
    let toggles_begin = modifier_toggles.clone();
    let hint_begin = draw_hint.clone();
    drag.connect_drag_begin(move |gesture, start_x, start_y| {
        *rec_drag.borrow_mut() = true;
        hint_begin.set_visible(false);
        // Nothing held keeps what the toggles say, so they can be set before drawing.
        let held = held_modifiers(gesture.current_event_state());
        if !held.is_empty() {
            toggles_begin.set(held);
        }
        let mut pts = pts_drag.borrow_mut();
        pts.clear();
        pts.push(Point2D {
            x: start_x,
            y: start_y,
        });
        println!(
            "GUI: Gesture drawing started at ({:.1}, {:.1})",
            start_x, start_y
        );
        canvas_clone.queue_draw();
        conflict_box_begin.set_visible(false);
    });

    let canvas_clone2 = canvas.clone();
    let pts_drag2 = Rc::clone(&recorded_points);
    let rec_drag2 = Rc::clone(&is_recording);
    drag.connect_drag_update(move |gesture, offset_x, offset_y| {
        if !*rec_drag2.borrow() {
            return;
        }
        if let Some((start_x, start_y)) = gesture.start_point() {
            let cx = start_x + offset_x;
            let cy = start_y + offset_y;
            let mut pts = pts_drag2.borrow_mut();

            // Filter jitter
            let add = match pts.last() {
                Some(lp) => {
                    let dx = cx - lp.x;
                    let dy = cy - lp.y;
                    dx * dx + dy * dy >= 9.0
                }
                None => true,
            };
            if add {
                pts.push(Point2D { x: cx, y: cy });
                canvas_clone2.queue_draw();
            }
        }
    });

    // The apps the gesture works in, which are part of its trigger, like its modifier keys.
    let gesture_apps = Rc::new(RefCell::new(
        target_gesture.as_ref().map(|g| g.apps.clone()).unwrap_or_default(),
    ));

    // A shape close to another gesture's conflicts when both can fire at once: with the same
    // modifier keys, in the same app. One for some apps and one for all of them don't, as the
    // app's own wins there.
    let check_conflict: Rc<dyn Fn()> = {
        let pts_check = Rc::clone(&recorded_points);
        let state_check = Rc::clone(state_rc);
        let edited_gesture_name = target_gesture.as_ref().map(|g| g.name.clone());
        let conflict_box = conflict_box.clone();
        let conflict_label = conflict_label.clone();
        let toggles = modifier_toggles.clone();
        let apps = Rc::clone(&gesture_apps);
        Rc::new(move || {
            let pts = pts_check.borrow();
            if pts.len() < 2 {
                conflict_box.set_visible(false);
                return;
            }
            let state = state_check.borrow();
            let drawn_modifiers = toggles.get();
            let apps = apps.borrow();
            let same_apps = |other: &[String]| {
                other.is_empty() == apps.is_empty()
                    && (apps.is_empty() || other.iter().any(|app| apps.contains(app)))
            };
            let templates: Vec<(String, Vec<Point2D>)> = state
                .config
                .gestures
                .iter()
                .filter(|g| !g.is_deleted)
                .filter(|g| g.modifiers == drawn_modifiers && same_apps(&g.apps))
                .filter(|g| edited_gesture_name.as_ref() != Some(&g.name))
                .map(|g| (g.name.clone(), g.points.clone()))
                .collect();

            if let Some(matched_name) = match_gesture(&pts[..], &templates) {
                conflict_label.set_text(&format!(
                    "Warning: This path is very similar to the existing gesture '{}'.",
                    matched_name
                ));
                conflict_box.set_visible(true);
            } else {
                conflict_box.set_visible(false);
            }
        })
    };

    let canvas_clone3 = canvas.clone();
    let rec_drag3 = Rc::clone(&is_recording);
    let check_drag_end = Rc::clone(&check_conflict);
    let pts_hint = Rc::clone(&recorded_points);
    let hint_end = draw_hint.clone();
    drag.connect_drag_end(move |_, _, _| {
        *rec_drag3.borrow_mut() = false;
        hint_end.set_visible(pts_hint.borrow().len() < 2);
        canvas_clone3.queue_draw();
        println!("GUI: Gesture drawing finished.");
        check_drag_end();
    });

    let modifiers_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    modifiers_row.add_css_class("settings-row");
    let modifiers_label = gtk::Label::new(Some("Modifier Keys"));
    modifiers_label.set_halign(gtk::Align::Start);
    modifiers_label.set_hexpand(true);
    modifiers_row.append(&modifiers_label);
    for button in modifier_toggles.buttons() {
        button.set_tooltip_text(Some("Hold this key while drawing the gesture to use it"));
        modifiers_row.append(button);
    }
    // The keys to hold and the shape to draw are one gesture, so they share a list: the keys on
    // top, the shape right below them.
    let gesture_list = gtk::ListBox::new();
    gesture_list.add_css_class("boxed-list");
    gesture_list.set_selection_mode(gtk::SelectionMode::None);
    let modifiers_list_row = gtk::ListBoxRow::new();
    modifiers_list_row.set_activatable(false);
    modifiers_list_row.set_child(Some(&modifiers_row));
    gesture_list.append(&modifiers_list_row);
    canvas.add_controller(drag);
    let canvas_row = gtk::ListBoxRow::new();
    canvas_row.set_activatable(false);
    let canvas_overlay = gtk::Overlay::new();
    canvas_overlay.set_child(Some(&canvas));
    canvas_overlay.add_overlay(&draw_hint);
    canvas_row.set_child(Some(&canvas_overlay));
    gesture_list.append(&canvas_row);
    scroll_content.append(&gesture_list);
    scroll_content.append(&conflict_box);

    let apps_list = gtk::ListBox::new();
    apps_list.add_css_class("boxed-list");
    apps_list.set_selection_mode(gtk::SelectionMode::None);
    append_apps_rows(&apps_list, &dialog, Rc::clone(&gesture_apps), Rc::clone(&check_conflict));
    scroll_content.append(&apps_list);

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

    // A custom action can't say what it's for, so it can have a name of its own. Other actions
    // are named after what they do.
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
    name_list_row.set_visible(false);
    settings_list.append(&name_list_row);

    // What the gesture does comes first, with its note about the extension, and then what
    // triggers it.
    scroll_content.reorder_child_after(&settings_section_label, None::<&gtk::Widget>);
    scroll_content.reorder_child_after(&settings_list, Some(&settings_section_label));
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

    action_details_row.append(&entry_container);
    settings_list.append(&action_details_row);

    // Edits the details entry, which holds a custom action's steps as the gesture saves them.
    let (steps_box, load_steps) = build_steps_editor(&dialog, &action_details_entry);
    // Its buttons and text would ask for room for all of them on one line, making Action Details
    // wider than the parts around it. Held this way, it asks only for its smallest width, takes
    // the width the others have, and wraps to it, with all of its height.
    let steps_editor = gtk::ScrolledWindow::new();
    steps_editor.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
    steps_editor.set_propagate_natural_height(true);
    steps_editor.set_child(Some(&steps_box));
    let steps_editor: gtk::Widget = steps_editor.upcast();
    settings_list.append(&steps_editor);
    if let Some(list_row) = steps_editor.parent() {
        list_row.set_visible(false);
    }

    let selected_action: Rc<RefCell<Option<EditorActionOption>>> = Rc::new(RefCell::new(None));

    let update_default_name = Rc::new({
        let name_entry = name_entry.clone();
        let action_details_entry = action_details_entry.clone();
        let selected_action = Rc::clone(&selected_action);
        let is_name_customized = Rc::clone(&is_name_customized);
        let is_updating_programmatically = Rc::clone(&is_updating_programmatically);

        move || {
            if *is_name_customized.borrow() {
                return;
            }
            if let Some(opt) = selected_action.borrow().as_ref() {
                let detail = action_details_entry.text().to_string();
                let default_name = get_default_gesture_name(opt, &detail);
                
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
    action_details_entry.connect_changed(move |_| {
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
                }
                Some(ActionType::Click(button)) => {
                    let button = button.map(|b| b.to_string()).unwrap_or_default();
                    action_details_entry.set_text(&button);
                }
                _ => {}
            }
            option
        }
        None => preset_action.clone(),
    };

    // Nothing to fill in until an action is known, such as one that's no longer available
    if let Some(list_row) = action_details_row.parent() {
        list_row.set_visible(false);
    }
    if let Some(opt) = initial_opt {
        *selected_action.borrow_mut() = Some(opt.clone());
        name_list_row.set_visible(matches!(opt.action_type, ActionType::Keyboard(_)));
        show_action_parameter(
            &opt,
            &action_details_row,
            &action_details_label,
            &action_details_entry,
            &shortcut_display_box,
            &record_btn,
            &steps_editor,
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
            .filter(|preset| matches!(preset.action_type, ActionType::Keyboard(_)))
            .and_then(|preset| name_from_search(&query, &preset.name))
        {
            name_entry.set_text(&name);
        }
        dialog.set_title(Some(&opt.name));
        dialog_title.set_text(&opt.name);
        let note = crate::ui::main_window::extension_note(
            &opt.action_type,
            &state_rc.borrow().extension_status,
        );
        extension_label.set_text(note.unwrap_or_default());
        extension_label.set_visible(note.is_some());
        // Most actions have nothing to fill in, which leaves this section empty.
        let has_details = action_needs_parameter(&opt);
        settings_section_label.set_visible(has_details);
        settings_list.set_visible(has_details);
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
            // One shortcut is saved as one, as standard shortcuts and older gestures are.
            ActionType::Keyboard(_) => match parse_steps(&detail).as_slice() {
                [] => {
                    println!("Gesture save failed: The custom action has no steps.");
                    return;
                }
                [KeyStep::Press(keys)] => ActionType::Keypress(keys.clone()),
                steps => ActionType::Keyboard(steps.to_vec()),
            },
            ActionType::Execute(_) if opt.category == 7 => ActionType::Execute(detail),
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

        refresh_gesture_list(&state_clone, Some(&name));
        dialog_clone2.destroy();
    });
    dialog_header.pack_end(&save_btn);

    if let Some(ref gest) = target_gesture {
        // It asks first, so its label ends in "…". It comes last in the content, as GNOME's own
        // apps place their destructive buttons, with room above it to set it apart.
        let delete_btn = gtk::Button::with_mnemonic("_Delete Gesture…");
        delete_btn.add_css_class("destructive-action");
        delete_btn.set_halign(gtk::Align::Center);
        delete_btn.set_margin_top(18);

        let state_clone = Rc::clone(state_rc);
        let target_id = gest.id.clone();
        let name_clone = gest.name.clone();
        let dialog_clone3 = dialog.clone();
        delete_btn.connect_clicked(move |_| {
            let state_clone = Rc::clone(&state_clone);
            let target_id = target_id.clone();
            let name_clone = name_clone.clone();
            let dialog_for_delete = dialog_clone3.clone();
            show_confirm_dialog(
                &dialog_clone3,
                "Delete Gesture?",
                &format!("\u{201c}{}\u{201d} will be removed. This can't be undone.", name_clone),
                "Delete",
                true,
                move || {
                    let mut state = state_clone.borrow_mut();
                    if let Some(pos) = state.config.gestures.iter().position(|g| g.id == target_id) {
                        state.config.gestures.remove(pos);
                        if let Err(e) = state.config.save_to_file() {
                            println!("Failed to save config to file on delete: {}", e);
                        }
                        println!("Gesture deleted successfully: {}", name_clone);
                        reload_daemon(state.dbus_conn.as_ref(), Some(&dialog_for_delete));
                        drop(state);
                        refresh_gesture_list(&state_clone, None);
                        dialog_for_delete.destroy();
                    }
                },
            );
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
        let action = "Custom Action";
        assert!(is_automatic_name("Custom Action", action));
        assert!(is_automatic_name("Custom Action (Control+T)", action));
        assert!(is_automatic_name("Custom Keyboard Action (Alt+F4)", action));
        assert!(is_automatic_name("  ", action));
        assert!(!is_automatic_name("New tab", action));
    }

    #[test]
    fn a_new_custom_action_is_named_after_the_search() {
        let action = "Custom Action";
        assert_eq!(name_from_search("  new tab ", action).as_deref(), Some("New tab"));
        assert_eq!(name_from_search("custom", action), None);
        assert_eq!(name_from_search("custom act", action), None);
        assert_eq!(name_from_search("", action), None);
    }
    use super::*;

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
        command.actions = vec![ActionType::Execute("gedit".to_string())];
        assert!(action_needs_parameter(&option_for_gesture(&command, &[]).unwrap()));
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
