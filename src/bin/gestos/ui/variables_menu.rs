//! Giving a command the variables, such as what's copied, and a menu of them, which puts the one
//! picked into the command where the cursor is.

use gestos::command_variables::OFFERED;
use gtk::prelude::*;
use gtk4 as gtk;

/// What goes by a command's entry: a check box that gives it the variables, which it doesn't get
/// otherwise, and a menu of them, which checks it when one is picked.
pub fn variables_controls(entry: &gtk::TextView, variables: bool) -> (gtk::Box, gtk::CheckButton) {
    let check = gtk::CheckButton::with_label("Use Variables");
    check.set_active(variables);
    check.set_valign(gtk::Align::Center);
    let menu = variables_button(entry, &check);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    controls.append(&check);
    controls.append(&menu);
    (controls, check)
}

/// A button by a command's entry, whose menu lists the variables a command is given.
fn variables_button(entry: &gtk::TextView, check: &gtk::CheckButton) -> gtk::MenuButton {
    let items = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let popover = gtk::Popover::new();
    popover.set_child(Some(&items));
    // Outside GNOME, what's copied is read with a program that may not be installed.
    if let Some(package) = gestos::command_variables::missing_clipboard_reader() {
        let note = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        note.set_margin_start(6);
        note.set_margin_end(6);
        note.set_margin_top(6);
        note.set_margin_bottom(6);
        note.append(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
        let label = gtk::Label::new(Some(&format!("Install {package} to use what's copied")));
        label.set_xalign(0.0);
        note.append(&label);
        items.append(&note);
    }
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
        let check = check.clone();
        item.connect_clicked(move |_| {
            check.set_active(true);
            insert_at_cursor(&entry, written);
            popover.popdown();
            entry.grab_focus();
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

/// How a variable is written where it goes, so that it stays quoted: as it is, unless it goes
/// inside quotes already open before it, where its own quotes would end them instead.
pub fn quoted_for(written: &str, before: &str) -> String {
    let bare = written.trim_matches('"');
    let (mut double, mut single, mut escaped) = (false, false, false);
    for c in before.chars() {
        match c {
            _ if escaped => escaped = false,
            '\\' if !single => escaped = true,
            '"' if !single => double = !double,
            '\'' if !double => single = !single,
            _ => {}
        }
    }
    if double {
        bare.to_string()
    } else if single {
        // Single quotes keep it from being read at all, so they're closed around it.
        format!("'{written}'")
    } else {
        written.to_string()
    }
}

/// Puts text where the cursor is, apart from the words around it.
fn insert_at_cursor(view: &gtk::TextView, written: &str) {
    let buffer = view.buffer();
    let cursor = buffer.iter_at_mark(&buffer.get_insert());
    // Quotes are open or closed by the line the cursor is on, as a script's lines are each read so.
    let mut line_start = cursor;
    line_start.set_line_offset(0);
    let before_cursor = buffer.text(&line_start, &cursor, false).to_string();
    let placed = quoted_for(written, &before_cursor);
    // Inside quotes, it's part of their text, which may need no spaces.
    let quoted = placed != written;
    let written = placed;
    let before = before_cursor.chars().last();
    let after = cursor.char();
    let mut inserted = String::new();
    if !quoted && before.is_some_and(|c| !c.is_whitespace()) {
        inserted.push(' ');
    }
    inserted.push_str(&written);
    if !quoted && after != '\0' && !after.is_whitespace() {
        inserted.push(' ');
    }
    buffer.insert_at_cursor(&inserted);
}

#[cfg(test)]
mod tests {
    use super::quoted_for;

    #[test]
    fn a_variable_stays_quoted_wherever_it_goes() {
        let clipboard = "\"$GESTOS_CLIPBOARD\"";
        assert_eq!(quoted_for(clipboard, "notify-send "), clipboard);
        assert_eq!(quoted_for(clipboard, "notify-send \"Copied: "), "$GESTOS_CLIPBOARD");
        assert_eq!(quoted_for(clipboard, "echo 'Copied: "), "'\"$GESTOS_CLIPBOARD\"'");
        assert_eq!(quoted_for(clipboard, "echo \"it's\" "), clipboard);
        assert_eq!(quoted_for(clipboard, "echo \\\" "), clipboard);
    }
}
