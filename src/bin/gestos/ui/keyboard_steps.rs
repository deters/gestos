//! The steps of a Custom Action: apps to open, shortcuts to press, text to type and pauses, in a
//! list that steps are added to, moved in and removed from. They're kept as text in the editor's
//! details entry, which is what the gesture saves.

use crate::ui::shortcut_recorder::open_shortcut_recorder;
use gestos::key_steps::{format_steps, parse_steps, KeyStep, MAX_WAIT_MILLIS};
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// How much the pause changes with each click of its buttons.
const WAIT_STEP_MILLIS: f64 = 50.0;
// The pause a new step starts with: about what an app takes to open a dialog.
const NEW_WAIT_MILLIS: u32 = 200;
// How tall the box for text to type is before it holds more than about three lines.
const TYPE_BOX_HEIGHT: i32 = 72;
// The size of the icon of an app to open.
const OPEN_ICON_SIZE: i32 = 24;

struct StepsEditor {
    parent: gtk::Window,
    details: gtk::Entry,
    rows: gtk::Box,
    steps: RefCell<Vec<KeyStep>>,
    // Whether the text box of a text step just added takes the focus, to type in right away.
    focus_new_text: Cell<bool>,
}

/// The list of steps, and what fills it from the details entry once that holds the gesture's
/// steps.
pub fn build_steps_editor(parent: &gtk::Window, details: &gtk::Entry) -> (gtk::Box, Rc<dyn Fn()>) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
    container.add_css_class("settings-row");

    let rows = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.append(&rows);

    let editor = Rc::new(StepsEditor {
        parent: parent.clone(),
        details: details.clone(),
        rows,
        steps: RefCell::new(Vec::new()),
        focus_new_text: Cell::new(false),
    });

    // Four buttons don't always fit on one line, so they wrap.
    let add_buttons = gtk::FlowBox::new();
    add_buttons.set_selection_mode(gtk::SelectionMode::None);
    add_buttons.set_column_spacing(6);
    add_buttons.set_row_spacing(6);
    add_buttons.set_max_children_per_line(4);
    add_buttons.set_halign(gtk::Align::Start);

    // An app comes up first, so that the steps after it go to it.
    let open = gtk::Button::with_mnemonic("Open an _App");
    let opening = Rc::clone(&editor);
    open.connect_clicked(move |_| {
        let picked = Rc::clone(&opening);
        crate::ui::app_picker::open_app_picker(&opening.parent, &[], move |app| {
            picked.steps.borrow_mut().push(KeyStep::Open(app));
            picked.changed();
            picked.rebuild();
        });
    });
    add_buttons.insert(&open, -1);

    for (label, step) in [
        ("Press a _Shortcut", KeyStep::Press(String::new())),
        ("_Type a Text", KeyStep::Type(String::new())),
        ("Add a _Pause", KeyStep::Wait(NEW_WAIT_MILLIS)),
    ] {
        let button = gtk::Button::with_mnemonic(label);
        let editor = Rc::clone(&editor);
        button.connect_clicked(move |_| {
            let records = matches!(step, KeyStep::Press(_));
            editor.steps.borrow_mut().push(step.clone());
            editor.changed();
            editor.focus_new_text.set(matches!(step, KeyStep::Type(_)));
            editor.rebuild();
            // Keys are only known once they're pressed, so a new keys step asks for them.
            if records {
                editor.record(editor.steps.borrow().len() - 1);
            }
        });
        add_buttons.insert(&button, -1);
    }
    container.append(&add_buttons);

    let load = {
        let editor = Rc::clone(&editor);
        Rc::new(move || {
            *editor.steps.borrow_mut() = parse_steps(&editor.details.text());
            editor.rebuild();
        })
    };
    load();
    (container, load)
}

impl StepsEditor {
    /// Saves the steps in the details entry, which also renames a gesture named after them.
    fn changed(&self) {
        self.details.set_text(&format_steps(&self.steps.borrow()));
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.rows.first_child() {
            self.rows.remove(&child);
        }
        let steps = self.steps.borrow().clone();
        // With no steps, only the buttons to add them show, without space kept for the list.
        self.rows.set_visible(!steps.is_empty());
        for (i, step) in steps.iter().enumerate() {
            self.rows.append(&self.step_row(i, step, steps.len()));
        }
    }

    fn step_row(self: &Rc<Self>, i: usize, step: &KeyStep, count: usize) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);

        let kind = gtk::Label::new(Some(match step {
            KeyStep::Press(_) => "Press",
            KeyStep::Type(_) => "Type",
            KeyStep::Wait(_) => "Wait",
            KeyStep::Open(_) => "Open",
        }));
        kind.add_css_class("dim-label");
        kind.set_width_chars(6);
        kind.set_xalign(0.0);
        row.append(&kind);

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        content.set_hexpand(true);
        content.set_valign(gtk::Align::Center);
        match step {
            KeyStep::Open(app) => {
                content.append(&crate::ui::app_picker::app_icon(app, OPEN_ICON_SIZE));
                let name = gtk::Label::new(Some(&crate::ui::app_picker::app_name(app)));
                name.set_xalign(0.0);
                content.append(&name);
            }
            KeyStep::Press(keys) => {
                content.append(&keycaps(keys));
                let record = gtk::Button::from_icon_name("media-record-symbolic");
                record.set_tooltip_text(Some("Record keys"));
                record.add_css_class("flat");
                let editor = Rc::clone(self);
                record.connect_clicked(move |_| editor.record(i));
                content.append(&record);
            }
            // Several lines, for text like a signature. Each line break is typed as Enter.
            KeyStep::Type(text) => {
                let view = gtk::TextView::new();
                view.set_wrap_mode(gtk::WrapMode::WordChar);
                view.set_accepts_tab(true);
                view.set_left_margin(8);
                view.set_right_margin(8);
                view.set_top_margin(6);
                view.set_bottom_margin(6);
                // About three lines tall, so it reads as a box for several, and growing with them.
                view.set_size_request(-1, TYPE_BOX_HEIGHT);
                view.buffer().set_text(text);
                let editor = Rc::clone(self);
                view.buffer().connect_changed(move |buffer| {
                    let (start, end) = buffer.bounds();
                    let text = buffer.text(&start, &end, false).to_string();
                    editor.steps.borrow_mut()[i] = KeyStep::Type(text);
                    editor.changed();
                });
                let frame = gtk::Frame::new(None);
                frame.set_child(Some(&view));
                frame.set_hexpand(true);
                content.append(&frame);
                // Once it's on screen, which is when it can take the focus.
                if i + 1 == count && self.focus_new_text.take() {
                    gtk::glib::idle_add_local_once(move || {
                        view.grab_focus();
                    });
                }
            }
            KeyStep::Wait(millis) => {
                let spin =
                    gtk::SpinButton::with_range(0.0, f64::from(MAX_WAIT_MILLIS), WAIT_STEP_MILLIS);
                spin.set_value(f64::from(*millis));
                let editor = Rc::clone(self);
                spin.connect_value_changed(move |spin| {
                    editor.steps.borrow_mut()[i] = KeyStep::Wait(spin.value_as_int() as u32);
                    editor.changed();
                });
                content.append(&spin);
                content.append(&gtk::Label::new(Some("ms")));
            }
        }
        row.append(&content);

        for (icon, tooltip, enabled, target) in [
            ("go-up-symbolic", "Move up", i > 0, i.wrapping_sub(1)),
            ("go-down-symbolic", "Move down", i + 1 < count, i + 1),
        ] {
            let button = gtk::Button::from_icon_name(icon);
            button.set_tooltip_text(Some(tooltip));
            button.add_css_class("flat");
            button.set_sensitive(enabled);
            let editor = Rc::clone(self);
            button.connect_clicked(move |_| {
                editor.steps.borrow_mut().swap(i, target);
                editor.changed();
                editor.rebuild();
            });
            row.append(&button);
        }

        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.set_tooltip_text(Some("Remove"));
        remove.add_css_class("flat");
        let editor = Rc::clone(self);
        remove.connect_clicked(move |_| {
            editor.steps.borrow_mut().remove(i);
            editor.changed();
            editor.rebuild();
        });
        row.append(&remove);

        row
    }

    /// Asks for the keys of a step, with the recorder the other shortcuts use.
    fn record(self: &Rc<Self>, i: usize) {
        let entry = gtk::Entry::new();
        if let Some(KeyStep::Press(keys)) = self.steps.borrow().get(i) {
            entry.set_text(keys);
        }
        let editor = Rc::clone(self);
        let recorded = entry.clone();
        let done: Rc<dyn Fn()> = Rc::new(move || {
            if let Some(step) = editor.steps.borrow_mut().get_mut(i) {
                *step = KeyStep::Press(recorded.text().to_string());
            }
            editor.changed();
            editor.rebuild();
        });
        open_shortcut_recorder(&self.parent, "this step", &entry, done);
    }
}

/// Keys as keycaps, like `Ctrl` `+` `L`, or a dimmed "None" before any are recorded.
fn keycaps(keys: &str) -> gtk::Box {
    let caps = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    caps.set_valign(gtk::Align::Center);
    let parts: Vec<&str> = keys.split('+').filter(|part| !part.trim().is_empty()).collect();
    if parts.is_empty() {
        let none = gtk::Label::new(Some("None"));
        none.set_opacity(0.5);
        caps.append(&none);
    }
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            let plus = gtk::Label::new(Some("+"));
            plus.set_opacity(0.6);
            caps.append(&plus);
        }
        let cap = gtk::Label::new(Some(part));
        cap.add_css_class("keycap");
        caps.append(&cap);
    }
    caps
}
