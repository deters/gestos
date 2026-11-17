//! The window a gesture is drawn in, as a shortcut's keys are recorded in theirs: before a new
//! gesture's editor opens, and from the editor to draw it again. Each drawing takes the place of
//! the last one, and Set keeps it. Cancel leaves the gesture as it was.

use gtk::prelude::*;
use gtk::glib;
use gestos::protractor::{confused_with, Point2D};
use gestos::modifiers::Modifiers;
use gtk4 as gtk;
use std::rc::Rc;
use std::cell::{Cell, RefCell};
use crate::state::AppState;

/// A gesture as it's drawn: its shape, and the modifier keys held while drawing it.
#[derive(Clone, Default)]
pub struct Drawing {
    pub points: Vec<Point2D>,
    pub modifiers: Modifiers,
}

/// The modifier keys a gesture needs, which are those held while it's drawn. They're shown as the
/// keyboard's keys name them, as on a Mac's, or as there being none. While keys are held in the
/// window, they show instead, lit up, so that the key each name means can be found by pressing it.
#[derive(Clone)]
pub struct ModifierKeys {
    recorded: Rc<Cell<Modifiers>>,
    held: Rc<Cell<Modifiers>>,
    pub keys: gtk::Box,
}

impl ModifierKeys {
    pub fn new(initial: Modifiers) -> Self {
        // In the drawing's corner, as they're part of the gesture drawn there. Without any, the
        // corner is empty.
        let keys = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        keys.set_halign(gtk::Align::Start);
        keys.set_valign(gtk::Align::Start);
        keys.set_can_target(false);
        let modifier_keys = ModifierKeys {
            recorded: Rc::new(Cell::new(initial)),
            held: Rc::new(Cell::new(Modifiers::default())),
            keys,
        };
        modifier_keys.show();
        modifier_keys
    }

    pub fn set(&self, modifiers: Modifiers) {
        self.recorded.set(modifiers);
        self.show();
    }

    pub fn get(&self) -> Modifiers {
        self.recorded.get()
    }

    fn show(&self) {
        let held = self.held.get();
        let shown = if held.is_empty() { self.recorded.get() } else { held };
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

/// The modifiers in a state GTK gives. On X11, Alt can come as Meta, and Super as Hyper, or only as
/// X's own Mod4, which GTK passes on without naming it.
fn held_modifiers(state: gtk::gdk::ModifierType) -> Modifiers {
    use gtk::gdk::ModifierType;
    const X11_MOD4: u32 = 1 << 6;
    Modifiers {
        ctrl: state.contains(ModifierType::CONTROL_MASK),
        alt: state.intersects(ModifierType::ALT_MASK | ModifierType::META_MASK),
        shift: state.contains(ModifierType::SHIFT_MASK),
        super_key: state.intersects(ModifierType::SUPER_MASK | ModifierType::HYPER_MASK)
            || state.bits() & X11_MOD4 != 0,
    }
}

/// What a gesture is drawn with, under the mouse that shows how: the button as Settings names it.
fn draw_hint(button: i32) -> String {
    match crate::ui::settings::DRAW_BUTTONS.iter().find(|(_, number)| *number == button) {
        Some((name, _)) => format!("Use the {} button of the mouse.", name.to_lowercase()),
        None => format!("Use button {button} of the mouse."),
    }
}

/// Whether there's a gesture drawn, rather than a click.
pub fn has_drawing(points: &[Point2D]) -> bool {
    points.len() >= 2
}

/// The app icons and modifier keys in a drawing's corner: the apps the gesture is for, which it
/// was made in and stays in, and the keys to hold, as they're one gesture with its shape. A
/// gesture for every app, without keys, shows none.
pub fn drawing_corner(apps: &[String], keys: &ModifierKeys) -> gtk::Box {
    let corner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    corner.set_halign(gtk::Align::Start);
    corner.set_valign(gtk::Align::Start);
    corner.set_margin_start(12);
    corner.set_margin_top(12);
    corner.set_can_target(false);
    for app in apps {
        let icon = crate::ui::app_picker::app_icon(app, 24);
        let name = crate::ui::app_picker::app_name(app);
        icon.set_tooltip_text(Some(&name));
        icon.update_property(&[gtk::accessible::Property::Label(&name)]);
        corner.append(&icon);
    }
    corner.append(&keys.keys);
    corner
}

/// Opens the window to draw a gesture in, over `parent`. `own_id` is the gesture's being drawn
/// again, which its new drawing can't be confused with, and `apps` the ones it's for.
pub fn open_gesture_recorder(
    parent: &gtk::Window,
    state_rc: &Rc<RefCell<AppState>>,
    own_id: Option<String>,
    apps: &[String],
    on_set: Rc<dyn Fn(Drawing)>,
) {
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_destroy_with_parent(true);
    dialog.set_resizable(false);
    dialog.set_title(Some("Draw Gesture"));
    crate::ui::components::close_with_keys(&dialog);
    // Without a frame, and see-through, so that what the gesture is for stays in sight. The window
    // itself is clear, and the card in it rounded, as a window's corners would be.
    dialog.set_decorated(false);
    dialog.remove_css_class("background");
    dialog.add_css_class("gesture-recorder");

    let cancel_btn = gtk::Button::with_mnemonic("_Cancel");
    let dialog_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| dialog_cancel.destroy());

    // Only once there's a drawing to set.
    let set_btn = gtk::Button::with_mnemonic("_Set");
    set_btn.add_css_class("suggested-action");
    set_btn.set_sensitive(false);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.add_css_class("recorder-card");
    dialog.set_child(Some(&content));

    let canvas = crate::ui::gesture_canvas::GestureCanvas::new();
    let points: Rc<RefCell<Vec<Point2D>>> = Rc::default();
    let is_recording = Rc::new(Cell::new(false));
    // Drawn with Gestos's own button, anywhere on screen, rather than in the drawing area.
    let drawn_on_screen = Rc::new(Cell::new(false));

    // Until there's a gesture, a mouse shows how one is drawn, with the button it's drawn with.
    let draw_button = state_rc
        .borrow()
        .config
        .button
        .unwrap_or(crate::ui::settings::DEFAULT_DRAW_BUTTON);
    let demo_started = Rc::new(Cell::new(None::<i64>));
    let demo_time = Rc::new(Cell::new(0.0));
    {
        let points = Rc::clone(&points);
        let is_recording = Rc::clone(&is_recording);
        let demo_time = Rc::clone(&demo_time);
        canvas.set_draw_func(move |canvas, cr, width, height| {
            if is_recording.get() {
                return;
            }
            let points = points.borrow();
            if has_drawing(&points) {
                crate::ui::components::draw_gesture_path(cr, &points, width as f64, height as f64, true, true);
            } else {
                #[allow(deprecated)]
                let fg = canvas.style_context().color();
                let fg = (f64::from(fg.red()), f64::from(fg.green()), f64::from(fg.blue()));
                crate::ui::gesture_demo::draw(cr, width as f64, height as f64, demo_time.get(), draw_button, fg);
            }
        });
    }
    // Under the mouse while it shows how, as gestures aren't drawn in the window but anywhere.
    let hint = gtk::Label::new(Some(&draw_hint(draw_button)));
    hint.set_justify(gtk::Justification::Center);
    hint.add_css_class("dim-label");
    hint.set_valign(gtk::Align::End);
    hint.set_margin_bottom(16);
    hint.set_can_target(false);
    {
        let points = Rc::clone(&points);
        let is_recording = Rc::clone(&is_recording);
        let hint = hint.clone();
        canvas.add_tick_callback(move |canvas, clock| {
            let showing_how = !has_drawing(&points.borrow()) && !is_recording.get();
            hint.set_visible(showing_how);
            if showing_how {
                let now = clock.frame_time();
                let started = demo_started.get().unwrap_or(now);
                demo_started.set(Some(started));
                demo_time.set((now - started) as f64 / 1_000_000.0);
                canvas.queue_draw();
            } else {
                // It starts over when it shows again, after a click that drew nothing.
                demo_started.set(None);
            }
            glib::ControlFlow::Continue
        });
    }

    // All the drawing shows while a gesture is drawn, which is then shown once it's done.
    let recording_label = gtk::Label::new(Some("Recording"));
    recording_label.add_css_class("title-2");
    recording_label.add_css_class("dim-label");
    recording_label.set_can_target(false);
    recording_label.set_visible(false);

    let conflict_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    conflict_box.add_css_class("warning-box");
    conflict_box.set_visible(false);
    let conflict_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
    conflict_icon.set_icon_size(gtk::IconSize::Normal);
    conflict_box.append(&conflict_icon);
    let conflict_label = gtk::Label::new(None);
    conflict_label.set_xalign(0.0);
    conflict_label.set_hexpand(true);
    conflict_label.add_css_class("warning-label");
    conflict_label.set_wrap(true);
    // Wrapped to the box's width, rather than asking for room for all of it on one line.
    conflict_label.set_max_width_chars(1);
    conflict_box.append(&conflict_label);

    let modifier_keys = ModifierKeys::new(Modifiers::default());
    modifier_keys.light_up_held_keys(&dialog);

    // A shape close to another gesture's conflicts when both can fire at once: with the same
    // modifier keys, in the same app. One for some apps and one for all of them don't, as the
    // app's own wins there.
    let check_conflict = {
        let points = Rc::clone(&points);
        let state_rc = Rc::clone(state_rc);
        let conflict_box = conflict_box.clone();
        let keys = modifier_keys.clone();
        let apps = apps.to_vec();
        move || {
            let points = points.borrow();
            let state = state_rc.borrow();
            let modifiers = keys.get();
            let same_apps = |other: &[String]| {
                other.is_empty() == apps.is_empty()
                    && (apps.is_empty() || other.iter().any(|app| apps.contains(app)))
            };
            let templates: Vec<(String, Vec<Point2D>)> = state
                .config
                .gestures
                .iter()
                .filter(|g| !g.is_deleted && Some(&g.id) != own_id.as_ref())
                .filter(|g| g.modifiers == modifiers && same_apps(&g.apps))
                .map(|g| (g.name.clone(), g.points.clone()))
                .collect();
            let confused = has_drawing(&points).then(|| confused_with(&points[..], &templates)).flatten();
            if let Some(name) = &confused {
                conflict_label.set_text(&format!("Very similar to \u{201c}{}\u{201d}", name));
            }
            conflict_box.set_visible(confused.is_some());
        }
    };

    // Each drawing takes the place of the one before, and the keys held then are the ones it needs.
    let start_drawing = {
        let points = Rc::clone(&points);
        let is_recording = Rc::clone(&is_recording);
        let drawn_on_screen = Rc::clone(&drawn_on_screen);
        let recording_label = recording_label.clone();
        let conflict_box = conflict_box.clone();
        let keys = modifier_keys.clone();
        let canvas = canvas.clone();
        let set_btn = set_btn.clone();
        Rc::new(move |start: Vec<Point2D>, held: Modifiers, on_screen: bool| {
            is_recording.set(true);
            drawn_on_screen.set(on_screen);
            recording_label.set_visible(true);
            conflict_box.set_visible(false);
            set_btn.set_sensitive(false);
            keys.set(held);
            *points.borrow_mut() = start;
            canvas.queue_draw();
        })
    };
    let finish_drawing = {
        let points = Rc::clone(&points);
        let is_recording = Rc::clone(&is_recording);
        let recording_label = recording_label.clone();
        let canvas = canvas.clone();
        let set_btn = set_btn.clone();
        Rc::new(move || {
            is_recording.set(false);
            recording_label.set_visible(false);
            canvas.queue_draw();
            set_btn.set_sensitive(has_drawing(&points.borrow()));
            check_conflict();
        })
    };

    // Only the button gestures are drawn with draws here, for when gestures are off and the
    // daemon doesn't record it from the screen.
    let drag = gtk::GestureDrag::new();
    drag.set_button(draw_button as u32);
    {
        let start_drawing = Rc::clone(&start_drawing);
        drag.connect_drag_begin(move |gesture, x, y| {
            start_drawing(vec![Point2D { x, y }], held_modifiers(gesture.current_event_state()), false);
        });
    }
    {
        let points = Rc::clone(&points);
        let is_recording = Rc::clone(&is_recording);
        let canvas = canvas.clone();
        drag.connect_drag_update(move |gesture, offset_x, offset_y| {
            let Some((start_x, start_y)) = gesture.start_point() else { return };
            if !is_recording.get() {
                return;
            }
            let (x, y) = (start_x + offset_x, start_y + offset_y);
            let mut points = points.borrow_mut();
            // Filter jitter
            let far_enough = points.last().map_or(true, |last| {
                let (dx, dy) = (x - last.x, y - last.y);
                dx * dx + dy * dy >= 9.0
            });
            if far_enough {
                points.push(Point2D { x, y });
                canvas.queue_draw();
            }
        });
    }
    {
        let is_recording = Rc::clone(&is_recording);
        let finish_drawing = Rc::clone(&finish_drawing);
        drag.connect_drag_end(move |_, _, _| {
            if is_recording.get() {
                finish_drawing();
            }
        });
    }
    canvas.add_controller(drag);

    // While Gestos runs, the button it draws with doesn't reach any window: the gesture is drawn
    // on screen, and its path comes from the daemon as it's drawn. This window holds the editor
    // lock while it's open, so that the daemon doesn't run what's drawn. Only one window records
    // at a time.
    let editor_lock = gestos::ipc::EditorLock::acquire().ok();
    let recording_from_screen = editor_lock.is_some();
    let bus = gtk::gio::bus_get_sync(gtk::gio::BusType::Session, gtk::gio::Cancellable::NONE).ok();
    let mut subscriptions = Vec::new();
    if let (Some(bus), true) = (&bus, recording_from_screen) {
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
        let held_at_start = Rc::new(Cell::new(Modifiers::default()));
        {
            let path = Rc::clone(&new_path);
            let held = Rc::clone(&held_at_start);
            let canvas = canvas.clone();
            subscriptions.push(subscribe("GestureStarted", Box::new(move |parameters| {
                let Some((x, y)) = parameters.get::<(f64, f64)>() else { return };
                *path.borrow_mut() = vec![Point2D { x, y }];
                // On X11, GTK only follows the keys for the window with focus, which this one may
                // not be, so the X server is asked.
                let on_x11 = gestos::active_app::is_x11_session()
                    .then(Modifiers::held_on_x11)
                    .flatten();
                held.set(on_x11.unwrap_or_else(|| {
                    canvas
                        .display()
                        .default_seat()
                        .and_then(|seat| seat.keyboard())
                        .map(|keyboard| held_modifiers(keyboard.modifier_state()))
                        .unwrap_or_default()
                }));
            })));
        }
        {
            let path = Rc::clone(&new_path);
            let held = Rc::clone(&held_at_start);
            let points = Rc::clone(&points);
            let is_recording = Rc::clone(&is_recording);
            let start_drawing = Rc::clone(&start_drawing);
            let canvas = canvas.clone();
            subscriptions.push(subscribe("GestureUpdated", Box::new(move |parameters| {
                let Some((x, y)) = parameters.get::<(f64, f64)>() else { return };
                let mut path = path.borrow_mut();
                if path.is_empty() {
                    return;
                }
                path.push(Point2D { x, y });
                if !is_recording.get() {
                    start_drawing(Vec::new(), held.get(), true);
                }
                *points.borrow_mut() = path.clone();
                canvas.queue_draw();
            })));
        }
        {
            let path = Rc::clone(&new_path);
            let is_recording = Rc::clone(&is_recording);
            let drawn_on_screen = Rc::clone(&drawn_on_screen);
            let finish_drawing = Rc::clone(&finish_drawing);
            subscriptions.push(subscribe("GestureEnded", Box::new(move |_| {
                path.borrow_mut().clear();
                if is_recording.get() && drawn_on_screen.get() {
                    finish_drawing();
                }
            })));
        }
    }
    let recording = RefCell::new(Some((editor_lock, subscriptions)));
    // When the window goes from the screen, rather than when it's destroyed: GTK only destroys a
    // window once nothing holds it, and the window's own handlers do.
    dialog.connect_unrealize(move |_| {
        if let Some((lock, subscriptions)) = recording.take() {
            if let Some(bus) = &bus {
                for id in subscriptions {
                    bus.signal_unsubscribe(id);
                }
            }
            drop(lock);
        }
    });

    {
        let points = Rc::clone(&points);
        let keys = modifier_keys.clone();
        let dialog = dialog.clone();
        set_btn.connect_clicked(move |_| {
            let points = points.borrow().clone();
            if has_drawing(&points) {
                dialog.destroy();
                on_set(Drawing { points, modifiers: keys.get() });
            }
        });
    }

    let gesture_list = gtk::ListBox::new();
    gesture_list.add_css_class("boxed-list");
    gesture_list.set_selection_mode(gtk::SelectionMode::None);
    let canvas_row = gtk::ListBoxRow::new();
    canvas_row.set_activatable(false);
    let canvas_overlay = gtk::Overlay::new();
    canvas_overlay.set_child(Some(&canvas));
    canvas_overlay.add_overlay(&recording_label);
    canvas_overlay.add_overlay(&hint);
    canvas_overlay.add_overlay(&drawing_corner(apps, &modifier_keys));
    canvas_row.set_child(Some(&canvas_overlay));
    gesture_list.append(&canvas_row);
    content.append(&gesture_list);
    content.append(&conflict_box);
    // Where a dialog's header bar would have them, below the drawing as there's no header bar.
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_homogeneous(true);
    buttons.append(&cancel_btn);
    buttons.append(&set_btn);
    content.append(&buttons);

    dialog.set_default_widget(Some(&set_btn));
    dialog.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_names_the_button_gestures_are_drawn_with() {
        assert_eq!(draw_hint(3), "Use the right button of the mouse.");
        assert_eq!(draw_hint(8), "Use the back button of the mouse.");
        assert_eq!(draw_hint(12), "Use button 12 of the mouse.");
    }

    #[test]
    fn alt_and_super_are_found_however_x11_reports_them() {
        use gtk::gdk::ModifierType;
        let all = Modifiers { ctrl: true, alt: true, shift: true, super_key: true };
        let named = ModifierType::CONTROL_MASK | ModifierType::ALT_MASK | ModifierType::SHIFT_MASK
            | ModifierType::SUPER_MASK;
        assert_eq!(held_modifiers(named), all);
        let as_meta_and_hyper = ModifierType::CONTROL_MASK | ModifierType::META_MASK
            | ModifierType::SHIFT_MASK | ModifierType::HYPER_MASK;
        assert_eq!(held_modifiers(as_meta_and_hyper), all);
        // Super as X11's Mod4, with Num Lock on, as GTK passed it on in a test.
        let mod4 = ModifierType::from_bits_retain(80);
        assert_eq!(held_modifiers(mod4), Modifiers { super_key: true, ..Default::default() });
    }
}
