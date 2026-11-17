//! The window a shell command is written in, for the action "Run Command in Shell" and an advanced
//! action's command steps: room for a long command, and its variables, which the rows it's for
//! don't have.

use gestos::command_variables::ShellCommand;
use gtk::prelude::*;
use gtk4 as gtk;
use std::rc::Rc;

/// Opens the window over `parent`, with `command` to edit. Set gives the command written, which
/// can't be empty. Cancel leaves it as it was.
pub fn open_command_editor(parent: &gtk::Window, command: &ShellCommand, on_set: Rc<dyn Fn(ShellCommand)>) {
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_destroy_with_parent(true);
    dialog.set_default_size(560, -1);
    dialog.set_title(Some("Edit Command"));
    crate::ui::components::close_with_keys(&dialog);

    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);
    let title_label = gtk::Label::new(Some("Edit Command"));
    title_label.add_css_class("title");
    header.set_title_widget(Some(&title_label));
    dialog.set_titlebar(Some(&header));

    let cancel_btn = gtk::Button::with_mnemonic("_Cancel");
    let dialog_cancel = dialog.clone();
    cancel_btn.connect_clicked(move |_| dialog_cancel.destroy());
    header.pack_start(&cancel_btn);

    let set_btn = gtk::Button::with_mnemonic("_Set");
    set_btn.add_css_class("suggested-action");
    header.pack_end(&set_btn);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.add_css_class("dialog-content");
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    dialog.set_child(Some(&content));

    let entry = gtk::Entry::new();
    entry.set_text(command);
    entry.add_css_class("monospace");
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("e.g. firefox"));
    entry.set_activates_default(true);
    content.append(&entry);

    let (variables, check) = crate::ui::variables_menu::variables_controls(&entry, command.variables);
    variables.set_halign(gtk::Align::Start);
    content.append(&variables);

    // Only once there's a command to run.
    set_btn.set_sensitive(!command.trim().is_empty());
    let set_shown = set_btn.clone();
    entry.connect_changed(move |entry| set_shown.set_sensitive(!entry.text().trim().is_empty()));
    // Whether it shows what it prints isn't edited here, and stays as it was.
    let shows_output = command.shows_output;
    let dialog_set = dialog.clone();
    let entry_set = entry.clone();
    set_btn.connect_clicked(move |_| {
        let text = entry_set.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        dialog_set.destroy();
        on_set(ShellCommand { text, variables: check.is_active(), shows_output });
    });

    dialog.set_default_widget(Some(&set_btn));
    dialog.present();
    entry.grab_focus();
}

/// A command as a row shows it: on one line, cut short at the end, or a dimmed "None" before
/// there's one.
pub fn command_preview() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class("monospace");
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label
}

pub fn show_command(label: &gtk::Label, command: &str) {
    let command = command.trim();
    if command.is_empty() {
        label.set_text("None");
        label.add_css_class("dim-label");
    } else {
        // Only its first line, if it has more, as the rest is cut off.
        label.set_text(command.lines().next().unwrap_or_default());
        label.remove_css_class("dim-label");
    }
    label.set_tooltip_text((!command.is_empty()).then_some(command));
}

/// The button on a command's row that opens the window to edit it.
pub fn edit_command_button() -> gtk::Button {
    let button = gtk::Button::from_icon_name("document-edit-symbolic");
    button.set_tooltip_text(Some("Edit Command"));
    button.update_property(&[gtk::accessible::Property::Label("Edit Command")]);
    button.add_css_class("flat");
    button.set_valign(gtk::Align::Center);
    button
}
