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
use crate::ui::keyboard_steps::append_steps_rows;
use crate::ui::app_picker::append_apps_rows;
use gestos::key_steps::{format_steps, parse_steps, KeyStep};
use crate::ui::components::draw_gesture_path;
use crate::daemon::reload_daemon;
use crate::ui::main_window::{get_visible_gestures, refresh_gesture_list};
use gestos::protractor::Point2D;

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

/// Whether a gesture with this action is named by the user, as a custom action or a custom
/// keyboard shortcut are, since the action alone doesn't say what they're for.
fn has_own_name(option: &EditorActionOption) -> bool {
    is_custom_action(option) || crate::suggestions::is_custom_shortcut(option)
}

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
    found
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

/// The modifier keys a gesture needs, which are those held while it's drawn. They're shown as the
/// keyboard's keys name them, as on a Mac's, or as there being none. While keys are held in the
/// editor, they show instead, lit up, so that the key each name means can be found by pressing it.
#[derive(Clone)]
struct ModifierKeys {
    recorded: Rc<Cell<Modifiers>>,
    held: Rc<Cell<Modifiers>>,
    label: gtk::Label,
    keys: gtk::Box,
}

impl ModifierKeys {
    fn new(initial: Modifiers) -> Self {
        let label = gtk::Label::new(None);
        label.set_halign(gtk::Align::Start);
        label.set_hexpand(true);
        let keys = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        keys.set_valign(gtk::Align::Center);
        let modifier_keys = ModifierKeys {
            recorded: Rc::new(Cell::new(initial)),
            held: Rc::new(Cell::new(Modifiers::default())),
            label,
            keys,
        };
        modifier_keys.show();
        modifier_keys
    }

    fn set(&self, modifiers: Modifiers) {
        self.recorded.set(modifiers);
        self.show();
    }

    fn get(&self) -> Modifiers {
        self.recorded.get()
    }

    fn show(&self) {
        let held = self.held.get();
        let shown = if held.is_empty() { self.recorded.get() } else { held };
        self.label.set_text(if shown.is_empty() { "No Modifier Keys" } else { "Modifier Keys" });
        while let Some(child) = self.keys.first_child() {
            self.keys.remove(&child);
        }
        let names = gestos::keyboards::modifier_names();
        for (i, name) in shown.shown_labels(&names).into_iter().enumerate() {
            if i > 0 {
                let plus = gtk::Label::new(Some("+"));
                plus.add_css_class("dim-label");
                self.keys.append(&plus);
            }
            let key = gtk::Label::new(Some(name));
            key.add_css_class("keycap");
            if !held.is_empty() {
                key.add_css_class("key-held");
            }
            self.keys.append(&key);
        }
    }

    /// The modifier a key is, by its key.
    fn modifier_of(key: gtk::gdk::Key) -> Option<Modifiers> {
        use gtk::gdk::Key;
        let mut modifier = Modifiers::default();
        match key {
            Key::Control_L | Key::Control_R => modifier.ctrl = true,
            Key::Alt_L | Key::Alt_R | Key::ISO_Level3_Shift => modifier.alt = true,
            Key::Shift_L | Key::Shift_R => modifier.shift = true,
            // Some keyboards' Super key comes as Meta or Hyper.
            Key::Super_L | Key::Super_R | Key::Meta_L | Key::Meta_R | Key::Hyper_L
            | Key::Hyper_R => modifier.super_key = true,
            _ => return None,
        }
        Some(modifier)
    }

    /// Shows the keys held in the window, while they are.
    fn light_up_held_keys(&self, window: &gtk::Window) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let pressed = self.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if let Some(modifier) = Self::modifier_of(key) {
                let held = pressed.held.get();
                pressed.held.set(Modifiers {
                    ctrl: held.ctrl || modifier.ctrl,
                    alt: held.alt || modifier.alt,
                    shift: held.shift || modifier.shift,
                    super_key: held.super_key || modifier.super_key,
                });
                pressed.show();
            }
            glib::Propagation::Proceed
        });
        let released = self.clone();
        keys.connect_key_released(move |_, key, _, _| {
            if let Some(modifier) = Self::modifier_of(key) {
                let held = released.held.get();
                released.held.set(Modifiers {
                    ctrl: held.ctrl && !modifier.ctrl,
                    alt: held.alt && !modifier.alt,
                    shift: held.shift && !modifier.shift,
                    super_key: held.super_key && !modifier.super_key,
                });
                released.show();
            }
        });
        window.add_controller(keys);
        // A key let go in another window would stay lit.
        let left = self.clone();
        window.connect_is_active_notify(move |window| {
            if !window.is_active() {
                left.held.set(Modifiers::default());
                left.show();
            }
        });
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
    open_editor(state_rc, Some(gesture), None, None);
}

/// Deletes a gesture right away, with a toast to bring it back. The focus moves to the gesture
/// after it in the list, or before it at the end.
fn delete_gesture(state_rc: &Rc<RefCell<AppState>>, id: &str) {
    let mut state = state_rc.borrow_mut();
    let Some(pos) = state.config.gestures.iter().position(|g| g.id == id) else {
        return;
    };
    let visible = get_visible_gestures(&state);
    let next = visible.iter().position(|g| g.id == id).and_then(|i| {
        visible.get(i + 1).or_else(|| i.checked_sub(1).and_then(|i| visible.get(i)))
    });
    let next_name = next.map(|g| g.name.clone());
    let removed = state.config.gestures.remove(pos);
    if let Err(e) = state.config.save_to_file() {
        println!("Failed to save config to file on delete: {}", e);
    }
    println!("Gesture deleted successfully: {}", removed.name);
    reload_daemon(state.dbus_conn.as_ref(), Some(&state.window));

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
    reload_daemon(state.dbus_conn.as_ref(), Some(&state.window));
    drop(state);
    refresh_gesture_list(state_rc, Some(&name));
}

/// Opens the editor for a new gesture. Its action comes from the main window's search, and the
/// editor never changes it. A custom keyboard shortcut is recorded first, as its keys are all
/// there is to it, and the editor only opens once they're set.
pub fn open_gesture_editor_for_action(state_rc: &Rc<RefCell<AppState>>, action: EditorActionOption) {
    if !crate::suggestions::is_custom_shortcut(&action) {
        open_editor(state_rc, None, Some(action), None);
        return;
    }
    let keys = gtk::Entry::new();
    let window = state_rc.borrow().window.clone().upcast::<gtk::Window>();
    let state_set = Rc::downgrade(state_rc);
    let keys_set = keys.clone();
    let on_set: Rc<dyn Fn()> = Rc::new(move || {
        let keys = keys_set.text().to_string();
        if let (Some(state_rc), false) = (state_set.upgrade(), keys.is_empty()) {
            open_editor(&state_rc, None, Some(action.clone()), Some(&keys));
        }
    });
    open_shortcut_recorder(&window, "", &keys, on_set);
}

fn open_editor(
    state_rc: &Rc<RefCell<AppState>>,
    target_gesture: Option<Gesture>,
    preset_action: Option<EditorActionOption>,
    preset_keys: Option<&str>,
) {
    let state = state_rc.borrow();
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(&state.window));
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

    // As wide as the rows below, and as tall as half of that.
    let canvas = crate::ui::gesture_canvas::GestureCanvas::new();
    let recorded_points: Rc<RefCell<Vec<Point2D>>> = Rc::new(RefCell::new(
        target_gesture
            .as_ref()
            .map(|g| g.points.clone())
            .unwrap_or_default(),
    ));
    let is_recording = Rc::new(RefCell::new(false));
    // Drawn with Gestos's own button, anywhere on screen, rather than in the drawing area.
    let drawn_on_screen = Rc::new(std::cell::Cell::new(false));

    let pts_clone = Rc::clone(&recorded_points);
    let rec_clone = Rc::clone(&is_recording);
    let on_screen_clone = Rc::clone(&drawn_on_screen);
    canvas.set_draw_func(move |_, cr, width, height| {
        let pts = pts_clone.borrow();
        let active = *rec_clone.borrow() && !on_screen_clone.get();
        draw_gesture_path(cr, &pts, width as f64, height as f64, true, !active);
    });

    // GestureDrag controller for recording drawing. Any button draws, as people try the one
    // gestures are drawn with.
    let drag = gtk::GestureDrag::new();
    drag.set_button(0);

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

    let modifier_toggles = ModifierKeys::new(
        target_gesture
            .as_ref()
            .map(|gesture| gesture.modifiers)
            .unwrap_or_default(),
    );
    modifier_toggles.light_up_held_keys(&dialog);

    // Shown over the drawing area while there's no gesture in it.
    // Named after the button gestures are drawn with, which is the one to draw with here too.
    let draw_button = state_rc.borrow().config.button.unwrap_or(crate::ui::settings::DEFAULT_DRAW_BUTTON);
    let button_name = crate::ui::settings::DRAW_BUTTONS
        .iter()
        .find(|(_, button)| *button == draw_button)
        .map_or("right", |(name, _)| name)
        .to_lowercase();
    let draw_hint = gtk::Label::new(Some(&format!(
        "Draw your gesture with the {button_name} mouse button. Hold a modifier key while drawing \
         to make it part of the trigger."
    )));
    draw_hint.set_wrap(true);
    draw_hint.set_justify(gtk::Justification::Center);
    draw_hint.set_max_width_chars(36);
    draw_hint.set_margin_start(24);
    draw_hint.set_margin_end(24);
    draw_hint.add_css_class("dim-label");
    draw_hint.set_can_target(false);
    draw_hint.set_visible(recorded_points.borrow().len() < 2);

    let canvas_clone = canvas.clone();
    let pts_drag = Rc::clone(&recorded_points);
    let rec_drag = Rc::clone(&is_recording);
    let conflict_box_begin = conflict_box.clone();
    let toggles_begin = modifier_toggles.clone();
    let hint_begin = draw_hint.clone();
    let on_screen_begin = Rc::clone(&drawn_on_screen);
    drag.connect_drag_begin(move |gesture, start_x, start_y| {
        *rec_drag.borrow_mut() = true;
        on_screen_begin.set(false);
        hint_begin.set_visible(false);
        // The keys held as it's drawn are the ones it needs, and none held is none.
        toggles_begin.set(held_modifiers(gesture.current_event_state()));
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
    let check_drawn = check.clone();
    let finish_drawing: Rc<dyn Fn()> = Rc::new(move || {
        *rec_drag3.borrow_mut() = false;
        hint_end.set_visible(pts_hint.borrow().len() < 2);
        canvas_clone3.queue_draw();
        println!("GUI: Gesture drawing finished.");
        check_drag_end();
        check_drawn();
    });
    let finish_drag = Rc::clone(&finish_drawing);
    drag.connect_drag_end(move |_, _, _| finish_drag());

    // While Gestos runs, the button it draws with doesn't reach any window: the gesture is drawn
    // on screen. Its path then comes from the daemon as it's drawn, and isn't run meanwhile.
    let editor_lock = gestos::ipc::EditorLock::acquire().ok();
    let bus = editor_lock
        .as_ref()
        .and_then(|_| gtk::gio::bus_get_sync(gtk::gio::BusType::Session, gtk::gio::Cancellable::NONE).ok());
    let mut subscriptions = Vec::new();
    if let Some(bus) = &bus {
        let subscribe = |signal: &str, handler: Box<dyn Fn(&glib::Variant)>| {
            bus.signal_subscribe(
                None,
                Some("io.github.deters.Gestos.Daemon"),
                Some(signal),
                Some("/io/github/deters/Gestos/Daemon"),
                None,
                gtk::gio::DBusSignalFlags::NONE,
                move |_, _, _, _, _, parameters| handler(parameters),
            )
        };
        // The path so far, which only takes the place of the one drawn once it moves, as a
        // click with the button starts one too.
        let new_path: Rc<RefCell<Vec<Point2D>>> = Rc::default();
        let held_at_start = Rc::new(std::cell::Cell::new(Modifiers::default()));
        let path = Rc::clone(&new_path);
        let held = Rc::clone(&held_at_start);
        let canvas_started = canvas.clone();
        subscriptions.push(subscribe("GestureStarted", Box::new(move |parameters| {
            let Some((x, y)) = parameters.get::<(f64, f64)>() else { return };
            *path.borrow_mut() = vec![Point2D { x, y }];
            held.set(
                canvas_started
                    .display()
                    .default_seat()
                    .and_then(|seat| seat.keyboard())
                    .map(|keyboard| held_modifiers(keyboard.modifier_state()))
                    .unwrap_or_default(),
            );
        })));
        let path = Rc::clone(&new_path);
        let held = Rc::clone(&held_at_start);
        let pts = Rc::clone(&recorded_points);
        let recording = Rc::clone(&is_recording);
        let on_screen = Rc::clone(&drawn_on_screen);
        let hint = draw_hint.clone();
        let conflict = conflict_box.clone();
        let toggles = modifier_toggles.clone();
        let canvas_updated = canvas.clone();
        subscriptions.push(subscribe("GestureUpdated", Box::new(move |parameters| {
            let Some((x, y)) = parameters.get::<(f64, f64)>() else { return };
            let mut path = path.borrow_mut();
            if path.is_empty() {
                return;
            }
            path.push(Point2D { x, y });
            if !*recording.borrow() {
                *recording.borrow_mut() = true;
                on_screen.set(true);
                hint.set_visible(false);
                conflict.set_visible(false);
                // As when drawing in the window, the keys held then are the ones it needs.
                toggles.set(held.get());
            }
            *pts.borrow_mut() = path.clone();
            canvas_updated.queue_draw();
        })));
        let path = Rc::clone(&new_path);
        let recording = Rc::clone(&is_recording);
        let on_screen = Rc::clone(&drawn_on_screen);
        let finish_on_screen = Rc::clone(&finish_drawing);
        subscriptions.push(subscribe("GestureEnded", Box::new(move |_| {
            path.borrow_mut().clear();
            if *recording.borrow() && on_screen.get() {
                finish_on_screen();
            }
        })));
    }
    let recording_from_screen = RefCell::new(Some((editor_lock, subscriptions)));
    // When the window goes from the screen, rather than when it's destroyed: GTK only destroys a
    // window once nothing holds it, and the editor's own handlers do. Closing it doesn't say
    // it's hidden either, only that it's no longer realized.
    dialog.connect_unrealize(move |_| {
        if let Some((lock, subscriptions)) = recording_from_screen.take() {
            if let Some(bus) = &bus {
                for id in subscriptions {
                    bus.signal_unsubscribe(id);
                }
            }
            drop(lock);
        }
    });

    let modifiers_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    modifiers_row.add_css_class("settings-row");
    modifiers_row.append(&modifier_toggles.label);
    modifiers_row.append(&modifier_toggles.keys);
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
    append_apps_rows(
        &apps_list,
        &dialog,
        Rc::clone(&gesture_apps),
        Rc::clone(&check_conflict),
        &state_rc.borrow().extension_status,
    );
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
        let note = crate::ui::main_window::extension_note(
            &opt.action_type,
            &state_rc.borrow().extension_status,
        );
        extension_label.set_text(note.unwrap_or_default());
        extension_label.set_visible(note.is_some());
        // Most actions have nothing to fill in, which leaves this section empty. An app's own
        // action can become a custom action, to do more once the app is open.
        customize_btn.set_visible(crate::actions::only_opens_an_app(&opt.action_type));
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
            let steps = match selected_action.borrow().as_ref().map(|opt| &opt.action_type) {
                Some(ActionType::Keyboard(steps)) => steps.clone(),
                _ => return,
            };
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
            ActionType::Execute(_) if opt.category == 7 => ActionType::Execute(detail),
            // The keys recorded, which a standard shortcut's are too.
            ActionType::Keypress(_) => {
                if detail.trim().is_empty() {
                    println!("Gesture save failed: The shortcut has no keys.");
                    return;
                }
                ActionType::Keypress(detail.trim().to_string())
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

        refresh_gesture_list(&state_clone, Some(&name));
        dialog_clone2.destroy();
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
