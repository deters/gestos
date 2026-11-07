//! The steps of a Custom Keyboard Action: shortcuts to press, text to type and pauses, in a list
//! that steps are added to, moved in and removed from. They're kept as text in the editor's
//! details entry, which is what the gesture saves.

use crate::ui::shortcut_recorder::open_shortcut_recorder;
use gestos::key_steps::{format_steps, parse_steps, KeyStep, MAX_WAIT_MILLIS};
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::rc::Rc;

// How much the pause changes with each click of its buttons.
const WAIT_STEP_MILLIS: f64 = 50.0;
// The pause a new step starts with: about what an app takes to open a dialog.
const NEW_WAIT_MILLIS: u32 = 200;

struct StepsEditor {
    parent: gtk::Window,
    details: gtk::Entry,
    rows: gtk::Box,
    steps: RefCell<Vec<KeyStep>>,
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
    });

    let add_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    add_buttons.set_halign(gtk::Align::Start);
    for (label, step) in [
        ("Add _Keys", KeyStep::Press(String::new())),
        ("Add _Text", KeyStep::Type(String::new())),
        ("Add _Pause", KeyStep::Wait(NEW_WAIT_MILLIS)),
    ] {
        let button = gtk::Button::with_mnemonic(label);
        let editor = Rc::clone(&editor);
        button.connect_clicked(move |_| {
            let records = matches!(step, KeyStep::Press(_));
            editor.steps.borrow_mut().push(step.clone());
            editor.changed();
            editor.rebuild();
            // Keys are only known once they're pressed, so a new keys step asks for them.
            if records {
                editor.record(editor.steps.borrow().len() - 1);
            }
        });
        add_buttons.append(&button);
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
        if steps.is_empty() {
            let empty =
                gtk::Label::new(Some("Add the keys to press, text to type and pauses, in order"));
            empty.add_css_class("dim-label");
            empty.set_halign(gtk::Align::Start);
            empty.set_wrap(true);
            empty.set_xalign(0.0);
            self.rows.append(&empty);
        }
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
        }));
        kind.add_css_class("dim-label");
        kind.set_width_chars(6);
        kind.set_xalign(0.0);
        row.append(&kind);

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        content.set_hexpand(true);
        content.set_valign(gtk::Align::Center);
        match step {
            KeyStep::Press(keys) => {
                content.append(&keycaps(keys));
                let record = gtk::Button::from_icon_name("media-record-symbolic");
                record.set_tooltip_text(Some("Record keys"));
                record.add_css_class("flat");
                let editor = Rc::clone(self);
                record.connect_clicked(move |_| editor.record(i));
                content.append(&record);
            }
            KeyStep::Type(text) => {
                let entry = gtk::Entry::new();
                entry.set_text(text);
                entry.set_placeholder_text(Some("Text to type"));
                entry.set_hexpand(true);
                let editor = Rc::clone(self);
                entry.connect_changed(move |entry| {
                    editor.steps.borrow_mut()[i] = KeyStep::Type(entry.text().to_string());
                    editor.changed();
                });
                content.append(&entry);
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
