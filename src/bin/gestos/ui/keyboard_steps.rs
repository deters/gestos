//! The steps of a Custom Action: apps to open, shortcuts to press, text to type and pauses. They
//! are rows of the action's list: a title row with a button to add each kind of step, and a row
//! for each step, which can be moved and removed. They're kept as text in the editor's details
//! entry, which is what the gesture saves.

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

// The icon for each kind of step, on its rows.
const OPEN_ICON: &str = "view-app-grid-symbolic";
const PRESS_ICON: &str = "input-keyboard-symbolic";
const TYPE_ICON: &str = "insert-text-symbolic";
const WAIT_ICON: &str = "alarm-symbolic";

// How many characters wide the name of each step is, to fit the longest.
const STEP_NAME_CHARS: i32 = 9;

fn step_name(step: &KeyStep) -> &'static str {
    match step {
        KeyStep::Open(_) => "Open App",
        KeyStep::Press(_) => "Shortcut",
        KeyStep::Type(_) => "Type",
        KeyStep::Wait(_) => "Pause",
    }
}

fn step_icon(step: &KeyStep) -> &'static str {
    match step {
        KeyStep::Open(_) => OPEN_ICON,
        KeyStep::Press(_) => PRESS_ICON,
        KeyStep::Type(_) => TYPE_ICON,
        KeyStep::Wait(_) => WAIT_ICON,
    }
}

struct StepsEditor {
    parent: gtk::Window,
    details: gtk::Entry,
    list: gtk::ListBox,
    title_row: gtk::ListBoxRow,
    rows: RefCell<Vec<gtk::ListBoxRow>>,
    shown: Cell<bool>,
    steps: RefCell<Vec<KeyStep>>,
    // Whether the text box of a text step just added takes the focus, to type in right away.
    focus_new_text: Cell<bool>,
}

/// Adds the steps to the end of `list`, as GNOME's lists add items: a title row with a button for
/// each kind of step, and a row for each. Returns what shows or hides them, and what fills them
/// from the details entry once that holds the gesture's steps.
pub fn append_steps_rows(
    list: &gtk::ListBox,
    parent: &gtk::Window,
    details: &gtk::Entry,
) -> (Rc<dyn Fn(bool)>, Rc<dyn Fn()>) {
    let title = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    title.add_css_class("settings-row");
    let label = gtk::Label::new(Some("Steps"));
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    title.append(&label);
    let title_row = gtk::ListBoxRow::new();
    title_row.set_activatable(false);
    title_row.set_child(Some(&title));
    list.append(&title_row);

    let editor = Rc::new(StepsEditor {
        parent: parent.clone(),
        details: details.clone(),
        list: list.clone(),
        title_row,
        rows: RefCell::new(Vec::new()),
        shown: Cell::new(true),
        steps: RefCell::new(Vec::new()),
        focus_new_text: Cell::new(false),
    });

    // Each opens what it needs, so those that open a dialog say so with "…". An app comes up
    // first, so that the steps after it go to it.
    // A "+" and a word, so it's clear they add a step, and which.
    let add_button = |word: &str, tooltip: &str| {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
        content.append(&gtk::Label::new(Some(word)));
        let button = gtk::Button::new();
        button.set_child(Some(&content));
        button.set_tooltip_text(Some(tooltip));
        button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
        button.add_css_class("flat");
        button.set_valign(gtk::Align::Center);
        title.append(&button);
        button
    };
    let opening = Rc::clone(&editor);
    add_button("App", "Open an App…").connect_clicked(move |_| {
        let picked = Rc::clone(&opening);
        crate::ui::app_picker::open_app_picker(&opening.parent, &[], move |app| {
            picked.add(KeyStep::Open(app));
        });
    });
    let pressing = Rc::clone(&editor);
    add_button("Shortcut", "Press a Shortcut…").connect_clicked(move |_| {
        pressing.add(KeyStep::Press(String::new()));
        // Keys are only known once they're pressed, so a new keys step asks for them.
        pressing.record(pressing.steps.borrow().len() - 1);
    });
    let typing = Rc::clone(&editor);
    add_button("Type", "Type a Text").connect_clicked(move |_| {
        typing.focus_new_text.set(true);
        typing.add(KeyStep::Type(String::new()));
    });
    let waiting = Rc::clone(&editor);
    add_button("Pause", "Add a Pause").connect_clicked(move |_| {
        waiting.add(KeyStep::Wait(NEW_WAIT_MILLIS));
    });

    let show: Rc<dyn Fn(bool)> = {
        let editor = Rc::clone(&editor);
        Rc::new(move |shown| editor.show(shown))
    };
    let load: Rc<dyn Fn()> = {
        let editor = Rc::clone(&editor);
        Rc::new(move || {
            *editor.steps.borrow_mut() = parse_steps(&editor.details.text());
            editor.rebuild();
        })
    };
    load();
    (show, load)
}

impl StepsEditor {
    /// Saves the steps in the details entry, which also renames a gesture named after them.
    fn changed(&self) {
        self.details.set_text(&format_steps(&self.steps.borrow()));
    }

    fn add(self: &Rc<Self>, step: KeyStep) {
        self.steps.borrow_mut().push(step);
        self.changed();
        self.rebuild();
    }

    fn show(&self, shown: bool) {
        self.shown.set(shown);
        self.title_row.set_visible(shown);
        for row in self.rows.borrow().iter() {
            row.set_visible(shown);
        }
    }

    // The step rows are the last of the list, after its title row.
    fn rebuild(self: &Rc<Self>) {
        for row in self.rows.borrow_mut().drain(..) {
            self.list.remove(&row);
        }
        let steps = self.steps.borrow().clone();
        for (i, step) in steps.iter().enumerate() {
            let row = gtk::ListBoxRow::new();
            row.set_activatable(false);
            row.set_visible(self.shown.get());
            row.set_child(Some(&self.step_row(i, step, steps.len())));
            self.list.append(&row);
            self.rows.borrow_mut().push(row);
        }
    }

    fn step_row(self: &Rc<Self>, i: usize, step: &KeyStep, count: usize) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.add_css_class("settings-row");

        // Its icon and name, the same width on every row, so the steps line up.
        let icon = gtk::Image::from_icon_name(step_icon(step));
        icon.add_css_class("dim-label");
        icon.set_valign(gtk::Align::Center);
        row.append(&icon);
        let kind = gtk::Label::new(Some(step_name(step)));
        kind.add_css_class("dim-label");
        kind.set_width_chars(STEP_NAME_CHARS);
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
