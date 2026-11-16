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
fn insert_at_cursor(entry: &gtk::Entry, written: &str) {
    let mut position = entry.position();
    let text = entry.text();
    let before_cursor: String = text.chars().take(position as usize).collect();
    let placed = quoted_for(written, &before_cursor);
    // Inside quotes, it's part of their text, which may need no spaces.
    let quoted = placed != written;
    let written = placed;
    let before = text.chars().nth((position as usize).wrapping_sub(1));
    let after = text.chars().nth(position as usize);
    let mut inserted = String::new();
    if !quoted && before.is_some_and(|c| !c.is_whitespace()) {
        inserted.push(' ');
    }
    inserted.push_str(&written);
    if !quoted && after.is_some_and(|c| !c.is_whitespace()) {
        inserted.push(' ');
    }
    entry.insert_text(&inserted, &mut position);
    entry.set_position(position);
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
