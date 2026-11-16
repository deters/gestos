//! A menu of what a command can use, such as what's copied, which puts the one picked into the
//! command where the cursor is.

use gestos::command_variables::OFFERED;
use gtk::prelude::*;
use gtk4 as gtk;

/// A button by a command's entry, whose menu lists the variables a command is given.
pub fn variables_button(entry: &gtk::Entry) -> gtk::MenuButton {
    let items = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let popover = gtk::Popover::new();
    popover.set_child(Some(&items));
    for (name, written) in OFFERED {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let label = gtk::Label::new(Some(name));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        content.append(&label);
        let code = gtk::Label::new(Some(written));
        code.add_css_class("monospace");
        code.add_css_class("dim-label");
        content.append(&code);
        let item = gtk::Button::new();
        item.set_child(Some(&content));
        item.add_css_class("flat");
        let entry = entry.clone();
        let popover = popover.clone();
        item.connect_clicked(move |_| {
            insert_at_cursor(&entry, written);
            popover.popdown();
            entry.grab_focus_without_selecting();
        });
        items.append(&item);
    }
    let button = gtk::MenuButton::new();
    button.set_icon_name("insert-text-symbolic");
    button.set_tooltip_text(Some("Insert a Variable"));
    button.set_valign(gtk::Align::Center);
    button.set_popover(Some(&popover));
    button
}

/// Puts text where the cursor is, apart from the words around it.
fn insert_at_cursor(entry: &gtk::Entry, written: &str) {
    let mut position = entry.position();
    let text = entry.text();
    let before = text.chars().nth((position as usize).wrapping_sub(1));
    let after = text.chars().nth(position as usize);
    let mut inserted = String::new();
    if before.is_some_and(|c| !c.is_whitespace()) {
        inserted.push(' ');
    }
    inserted.push_str(written);
    if after.is_some_and(|c| !c.is_whitespace()) {
        inserted.push(' ');
    }
    entry.insert_text(&inserted, &mut position);
    entry.set_position(position);
}
