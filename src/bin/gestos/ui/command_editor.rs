//! The window a shell command is written in, for the action "Run Command in Shell" and an advanced
//! action's command steps: room for a long command or a script of several lines, and its
//! variables, which the rows it's for don't have.

use adw::prelude::*;
use gestos::command_variables::ShellCommand;
use gestos::shell_syntax::{spans, Kind};
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

    // Several lines, as a script has, so Enter starts a new line and Ctrl+Enter sets it. Tab
    // moves on to the next control, as everywhere else, rather than being typed.
    let view = gtk::TextView::new();
    view.buffer().set_text(command);
    view.add_css_class("monospace");
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_accepts_tab(false);
    view.set_top_margin(8);
    view.set_bottom_margin(8);
    view.set_left_margin(8);
    view.set_right_margin(8);
    view.update_property(&[gtk::accessible::Property::Label("Command")]);
    highlight_shell(&view.buffer(), &dialog);
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_child(Some(&view));
    scrolled.set_has_frame(true);
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_min_content_height(160);
    scrolled.set_max_content_height(400);
    scrolled.set_propagate_natural_height(true);
    // Made taller with the window, for a long script.
    scrolled.set_vexpand(true);
    // An example while it's empty, as a text field has.
    let placeholder = gtk::Label::new(Some("e.g. firefox"));
    placeholder.add_css_class("monospace");
    placeholder.add_css_class("dim-label");
    placeholder.set_halign(gtk::Align::Start);
    placeholder.set_valign(gtk::Align::Start);
    placeholder.set_margin_start(9);
    placeholder.set_margin_top(9);
    placeholder.set_can_target(false);
    placeholder.set_visible(command.is_empty());
    let field = gtk::Overlay::new();
    field.set_child(Some(&scrolled));
    field.add_overlay(&placeholder);
    content.append(&field);

    let (variables, check) = crate::ui::variables_menu::variables_controls(&view, command.variables);
    variables.set_halign(gtk::Align::Start);
    content.append(&variables);

    // Only once there's a command to run.
    set_btn.set_sensitive(!command.trim().is_empty());
    let set_shown = set_btn.clone();
    view.buffer().connect_changed(move |buffer| {
        let empty = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).trim().is_empty();
        set_shown.set_sensitive(!empty);
        placeholder.set_visible(buffer.char_count() == 0);
    });
    let shortcuts = gtk::ShortcutController::new();
    let set_with_keys = set_btn.clone();
    shortcuts.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("<Control>Return"),
        Some(gtk::CallbackAction::new(move |_, _| {
            set_with_keys.activate();
            gtk::glib::Propagation::Stop
        })),
    ));
    view.add_controller(shortcuts);
    // Whether it shows what it prints isn't edited here, and stays as it was.
    let shows_output = command.shows_output;
    let dialog_set = dialog.clone();
    let buffer = view.buffer();
    set_btn.connect_clicked(move |_| {
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).trim().to_string();
        if text.is_empty() {
            return;
        }
        dialog_set.destroy();
        on_set(ShellCommand { text, variables: check.is_active(), shows_output });
    });

    dialog.set_default_widget(Some(&set_btn));
    dialog.present();
    view.grab_focus();
}

/// The colors of a command's parts, for a light style and a dark one, from GNOME's palette.
fn color_of(kind: Kind, dark: bool) -> &'static str {
    match (kind, dark) {
        (Kind::Keyword, false) => "#813d9c",
        (Kind::Keyword, true) => "#dc8add",
        (Kind::Quoted, false) => "#26a269",
        (Kind::Quoted, true) => "#8ff0a4",
        (Kind::Variable, false) => "#1c71d8",
        (Kind::Variable, true) => "#99c1f1",
        (Kind::Comment, false) => "#77767b",
        (Kind::Comment, true) => "#9a9996",
        (Kind::Operator, false) => "#c64600",
        (Kind::Operator, true) => "#ffbe6f",
    }
}

/// Colors a command's keywords, quoted text, variables, comments and operators as it's written,
/// in colors for the style in use, light or dark, which change with it while `window` is open.
fn highlight_shell(buffer: &gtk::TextBuffer, window: &gtk::Window) {
    // Made in this order, so a variable inside quotes has its own color over theirs.
    let kinds = [Kind::Quoted, Kind::Keyword, Kind::Operator, Kind::Comment, Kind::Variable];
    let tags: Vec<(Kind, gtk::TextTag)> = kinds
        .into_iter()
        .map(|kind| (kind, buffer.create_tag(None, &[]).expect("a tag without a name")))
        .collect();
    let style = adw::StyleManager::default();
    let color = {
        let tags = tags.clone();
        move |style: &adw::StyleManager| {
            for (kind, tag) in &tags {
                tag.set_foreground(Some(color_of(*kind, style.is_dark())));
                if *kind == Kind::Comment {
                    tag.set_style(gtk::pango::Style::Italic);
                }
            }
        }
    };
    color(&style);
    let recolor = std::cell::Cell::new(Some(style.connect_dark_notify(color)));
    window.connect_destroy(move |_| {
        if let Some(recolor) = recolor.take() {
            style.disconnect(recolor);
        }
    });

    let paint = move |buffer: &gtk::TextBuffer| {
        let (start, end) = buffer.bounds();
        for (_, tag) in &tags {
            buffer.remove_tag(tag, &start, &end);
        }
        for span in spans(&buffer.text(&start, &end, false)) {
            let Some((_, tag)) = tags.iter().find(|(kind, _)| *kind == span.kind) else { continue };
            let from = buffer.iter_at_offset(span.start as i32);
            let to = buffer.iter_at_offset(span.end as i32);
            buffer.apply_tag(tag, &from, &to);
        }
    };
    paint(buffer);
    buffer.connect_changed(paint);
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
