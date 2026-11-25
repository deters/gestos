use adw::prelude::*;
use gtk::{gio, glib};
use gestos::config::Configuration;
use crate::ui::main_window::refresh_gesture_list;
use crate::ui::components::{show_error_dialog, show_confirm_dialog};
use gtk4 as gtk;
use std::rc::Rc;
use std::cell::RefCell;
use crate::state::AppState;
use crate::daemon::reload_daemon;
use crate::extension::{self, ExtensionStatus};
use crate::system::{get_autostart_file_path, mark_autostart_decided, set_autostart_enabled};

/// A row's subtitle, which also says when a setting couldn't be changed.
#[derive(Clone)]
struct Subtitle(adw::ActionRow);

impl Subtitle {
    fn set_text(&self, text: &str) {
        self.0.set_subtitle(text);
    }
}

/// A row with a title, a subtitle under it, and a box at its end for its controls.
fn settings_row(title: &str, subtitle: &str) -> (adw::ActionRow, gtk::Box, Subtitle) {
    let row = adw::ActionRow::builder().title(title).subtitle(subtitle).build();
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    controls.set_valign(gtk::Align::Center);
    row.add_suffix(&controls);
    let subtitle = Subtitle(row.clone());
    (row, controls, subtitle)
}

/// A titled group of rows, as GNOME's settings pages have them.
fn append_section(page: &adw::PreferencesPage, title: &str) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(title).build();
    page.add(&group);
    group
}

fn settings_switch(label: &str) -> gtk::Switch {
    let switch = gtk::Switch::new();
    switch.set_valign(gtk::Align::Center);
    switch.update_property(&[gtk::accessible::Property::Label(label)]);
    switch
}

/// Whether what the extension does can be set: it's there to do it.
fn extension_settings_usable(status: &ExtensionStatus) -> bool {
    matches!(status, ExtensionStatus::Running | ExtensionStatus::Outdated(_))
}

fn append_autostart_row(list: &adw::PreferencesGroup) {
    let manual_autostart =
        gestos::config::manual_autostart_for(gestos::config::get_environment_suffix());
    let description = "Start gestures automatically";
    let (row, row_box, subtitle) = settings_row("Start When I Log In", description);
    match manual_autostart {
        Some((desktop, config_file, line)) => {
            subtitle.set_text(&format!(
                "{desktop} doesn't use this setting. Add this line to {config_file}:"
            ));
            let config_line = gtk::Label::new(Some(line));
            config_line.add_css_class("monospace");
            config_line.set_selectable(true);
            row_box.append(&config_line);
        }
        None => {
            let switch = settings_switch("Start When I Log In");
            switch.set_active(get_autostart_file_path().is_some_and(|path| path.exists()));
            switch.connect_state_set(move |_, enabled| {
                let changed = mark_autostart_decided().and_then(|_| set_autostart_enabled(enabled));
                match changed {
                    Ok(()) => subtitle.set_text(description),
                    Err(e) => subtitle.set_text(&format!("Could not change this setting: {e}")),
                }
                glib::Propagation::Proceed
            });
            row_box.append(&switch);
        }
    }
    list.add(&row);
}

/// The mouse buttons gestures can be drawn with, numbered as in `gestosd --button`.
pub const DRAW_BUTTONS: [(&str, i32); 4] = [("Right", 3), ("Middle", 2), ("Back", 8), ("Forward", 9)];
pub const DEFAULT_DRAW_BUTTON: i32 = 3;

/// Which mouse button draws gestures. The daemon only picks it as it starts, so a change
/// restarts gestures if they're on.
fn append_button_row(
    state_rc: &Rc<RefCell<AppState>>,
    list: &adw::PreferencesGroup,
    dialog: &gtk::Window,
    right_only: Vec<gtk::Widget>,
) {
    let (row, row_box, subtitle) =
        settings_row("Draw With", "The mouse button that draws gestures");
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    buttons.add_css_class("linked");
    buttons.set_valign(gtk::Align::Center);
    let current = state_rc.borrow().config.button.unwrap_or(DEFAULT_DRAW_BUTTON);
    let mut first: Option<gtk::ToggleButton> = None;
    for (label, button) in DRAW_BUTTONS {
        let toggle = gtk::ToggleButton::with_label(label);
        toggle.set_group(first.as_ref());
        toggle.set_active(button == current);
        let state_rc = Rc::clone(state_rc);
        let dialog = dialog.clone();
        let subtitle = subtitle.clone();
        let right_only = right_only.clone();
        toggle.connect_toggled(move |toggle| {
            if !toggle.is_active() {
                return;
            }
            for widget in &right_only {
                widget.set_sensitive(button == DEFAULT_DRAW_BUTTON);
            }
            let mut state = state_rc.borrow_mut();
            let picked = (button != DEFAULT_DRAW_BUTTON).then_some(button);
            if state.config.button == picked {
                return;
            }
            state.config.button = picked;
            if let Err(e) = state.config.save_to_file() {
                subtitle.set_text(&format!("Could not change this setting: {e}"));
                return;
            }
            let conn = state.dbus_conn.clone();
            drop(state);
            if crate::daemon::is_daemon_running(conn.as_ref()) {
                crate::daemon::stop_daemon(conn.as_ref());
                if let Err(e) = crate::daemon::start_daemon(conn.as_ref()) {
                    show_error_dialog(&dialog, &e);
                }
            }
        });
        first.get_or_insert(toggle.clone());
        buttons.append(&toggle);
    }
    row_box.append(&buttons);
    list.add(&row);
}


/// Clicking left while holding right shows the overview, as on a Mac. Only desktops with an
/// overview have it, and it only works while the right button draws.
fn overview_click_row(state_rc: &Rc<RefCell<AppState>>) -> Option<(adw::ActionRow, gtk::Switch)> {
    if !matches!(gestos::config::get_environment_suffix(), Some("gnome" | "kde")) {
        return None;
    }
    Some(right_button_switch_row(
        state_rc,
        "Show Overview With Both Buttons",
        "Hold right, then click left",
        |config| &mut config.overview_click,
    ))
}

/// Dragging with the middle button held moves the window under the pointer, and scrolling resizes
/// it. The extension moves and resizes it, and the middle button is only held back from apps while
/// the right button draws, as the mouse is only grabbed then.
fn middle_button_windows_row(state_rc: &Rc<RefCell<AppState>>) -> (adw::ActionRow, gtk::Switch) {
    right_button_switch_row(
        state_rc,
        "Control Windows With the Middle Button",
        "Hold it and drag to move, or scroll to resize",
        |config| &mut config.middle_button_windows,
    )
}

/// GNOME's own setting for pasting the selected text with a middle click, as GNOME Tweaks has it.
fn middle_click_paste_row() -> Option<adw::ActionRow> {
    const SCHEMA: &str = "org.gnome.desktop.interface";
    const KEY: &str = "gtk-enable-primary-paste";
    gio::SettingsSchemaSource::default()?.lookup(SCHEMA, true).filter(|schema| schema.has_key(KEY))?;
    let settings = gio::Settings::new(SCHEMA);
    let title = "Allow Middle Click Paste";
    let description = "Paste the selected text with a middle click";
    let (row, row_box, _) = settings_row(title, description);
    let switch = settings_switch(title);
    settings.bind(KEY, &switch, "active").build();
    // The binding needs the settings for as long as the switch is there.
    switch.connect_destroy(move |_| {
        let _ = &settings;
    });
    row_box.append(&switch);
    Some(row)
}

/// A switch for a setting of the daemon's that only works while the right button draws.
fn right_button_switch_row(
    state_rc: &Rc<RefCell<AppState>>,
    title: &'static str,
    description: &'static str,
    setting: fn(&mut gestos::config::Configuration) -> &mut bool,
) -> (adw::ActionRow, gtk::Switch) {
    let (row, row_box, subtitle) = settings_row(title, description);
    let switch = settings_switch(title);
    {
        let mut state = state_rc.borrow_mut();
        switch.set_active(*setting(&mut state.config));
        switch.set_sensitive(state.config.button.unwrap_or(DEFAULT_DRAW_BUTTON) == DEFAULT_DRAW_BUTTON);
    }
    let state_rc = Rc::clone(state_rc);
    switch.connect_state_set(move |switch, enabled| {
        let mut state = state_rc.borrow_mut();
        *setting(&mut state.config) = enabled;
        if let Err(e) = state.config.save_to_file() {
            *setting(&mut state.config) = !enabled;
            subtitle.set_text(&format!("Could not change this setting: {e}"));
            return glib::Propagation::Stop;
        }
        subtitle.set_text(description);
        let parent = switch.root().and_downcast::<gtk::Window>();
        reload_daemon(state.dbus_conn.as_ref(), parent.as_ref());
        glib::Propagation::Proceed
    });
    row_box.append(&switch);
    (row, switch)
}

/// Whether the main page leaves out the list of every gesture, above the categories. It only
/// changes what Gestos shows, so the daemon isn't told.
fn hide_gestures_row(state_rc: &Rc<RefCell<AppState>>) -> adw::ActionRow {
    let title = "Hide Gesture List in the Main Page";
    let description = "Show only the categories";
    let (row, row_box, subtitle) = settings_row(title, description);
    let switch = settings_switch(title);
    switch.set_active(state_rc.borrow().config.hide_gestures);
    let state_rc = Rc::clone(state_rc);
    switch.connect_state_set(move |_, enabled| {
        {
            let mut state = state_rc.borrow_mut();
            state.config.hide_gestures = enabled;
            if let Err(e) = state.config.save_to_file() {
                state.config.hide_gestures = !enabled;
                subtitle.set_text(&format!("Could not change this setting: {e}"));
                return glib::Propagation::Stop;
            }
        }
        subtitle.set_text(description);
        refresh_gesture_list(&state_rc, None);
        glib::Propagation::Proceed
    });
    row_box.append(&switch);
    row
}

/// On X11 and KDE, Gestos draws gestures' paths itself, as its configuration says.
fn daemon_path_row(state_rc: &Rc<RefCell<AppState>>) -> adw::ActionRow {
    let title = "Show Gesture Path";
    let description = "Draw each gesture on screen as you draw it";
    let (row, row_box, subtitle) = settings_row(title, description);
    let switch = settings_switch(title);
    switch.set_active(state_rc.borrow().config.show_path);
    let state_rc = Rc::clone(state_rc);
    switch.connect_state_set(move |switch, enabled| {
        let mut state = state_rc.borrow_mut();
        state.config.show_path = enabled;
        if let Err(e) = state.config.save_to_file() {
            state.config.show_path = !enabled;
            subtitle.set_text(&format!("Could not change this setting: {e}"));
            return glib::Propagation::Stop;
        }
        subtitle.set_text(description);
        let parent = switch.root().and_downcast::<gtk::Window>();
        reload_daemon(state.dbus_conn.as_ref(), parent.as_ref());
        glib::Propagation::Proceed
    });
    row_box.append(&switch);
    row
}

/// The extension's status, with a switch for it. The rows for what it does follow it, as they
/// can only be changed while it runs.
fn append_extension_row(
    state_rc: &Rc<RefCell<AppState>>,
    list: &adw::PreferencesGroup,
    needs_extension: Vec<adw::ActionRow>,
) {
    let status = state_rc.borrow().extension_status.clone();
    for row in &needs_extension {
        row.set_sensitive(extension_settings_usable(&status));
    }
    let (status_row, status_box, status_subtitle) =
        // Named as GNOME's Extensions app lists it.
        settings_row("Gestos", &status.description());
    if status.can_switch() {
        let switch = settings_switch("Gestos GNOME extension");
        switch.set_active(status != ExtensionStatus::Off);
        let state_switch = Rc::clone(state_rc);
        switch.connect_state_set(move |_, enabled| {
            let conn = state_switch.borrow().dbus_conn.clone();
            let Some(conn) = conn else {
                status_subtitle.set_text("Could not change this setting: no session bus");
                return glib::Propagation::Stop;
            };
            if let Err(e) = extension::set_enabled(&conn, enabled) {
                status_subtitle.set_text(&format!("Could not change this setting: {e}"));
                return glib::Propagation::Stop;
            }
            // GNOME takes a moment to start or stop it.
            let state_after = Rc::clone(&state_switch);
            let subtitle = status_subtitle.clone();
            let needs_extension = needs_extension.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
                crate::ui::main_window::refresh_extension_status(&state_after);
                let status = state_after.borrow().extension_status.clone();
                subtitle.set_text(&status.description());
                for row in &needs_extension {
                    row.set_sensitive(extension_settings_usable(&status));
                }
            });
            glib::Propagation::Proceed
        });
        status_box.append(&switch);
    }
    list.add(&status_row);
}

/// The actions made of templates, each with a switch to offer it or not, and a way to save it to
/// a file. Imported ones are the user's own, and can be removed; one with the ID of an action that
/// came with Gestos takes its place.
fn actions_page(state_rc: &Rc<RefCell<AppState>>, dialog: &gtk::Window) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder().title("Actions").icon_name("system-run-symbolic").build();
    let group = adw::PreferencesGroup::builder().title("Actions").build();
    let import = gtk::Button::with_mnemonic("_Import…");
    import.set_valign(gtk::Align::Center);
    group.set_header_suffix(Some(&import));
    page.add(&group);
    let rows: Rc<RefCell<Vec<adw::ActionRow>>> = Rc::default();
    let fill: Rc<dyn Fn()> = {
        let state_rc = Rc::downgrade(state_rc);
        let group = group.clone();
        let dialog = dialog.clone();
        let rows = Rc::clone(&rows);
        // Filled again after each import or removal, as the list changes.
        // Weak, as the rows it makes hold it, and the import button keeps it.
        let fill_cell: Rc<RefCell<std::rc::Weak<dyn Fn()>>> = Rc::new(RefCell::new(std::rc::Weak::<Box<dyn Fn()>>::new()));
        let fill_again = Rc::clone(&fill_cell);
        let fill: Rc<dyn Fn()> = Rc::new(move || {
            let Some(state_rc) = state_rc.upgrade() else { return };
            for row in rows.borrow_mut().drain(..) {
                group.remove(&row);
            }
            let refill = fill_again.borrow().upgrade();
            for listed in gestos::action_templates::list() {
                let row = action_template_row(&state_rc, &dialog, &listed, refill.clone());
                group.add(&row);
                rows.borrow_mut().push(row);
            }
        });
        *fill_cell.borrow_mut() = Rc::downgrade(&fill);
        fill
    };
    fill();
    let state_import = Rc::downgrade(state_rc);
    let dialog_import = dialog.clone();
    import.connect_clicked(move |_| {
        if let Some(state_rc) = state_import.upgrade() {
            import_action_template(&state_rc, &dialog_import, Rc::clone(&fill));
        }
    });
    page
}

/// An action's row: its name and description, a button to export it, one to remove it if it's
/// the user's own, and a switch to offer it or not.
fn action_template_row(
    state_rc: &Rc<RefCell<AppState>>,
    dialog: &gtk::Window,
    listed: &gestos::action_templates::Listed,
    refill: Option<Rc<dyn Fn()>>,
) -> adw::ActionRow {
    let template = &listed.template;
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&template.name).as_str())
        .subtitle(glib::markup_escape_text(&template.description).as_str())
        .build();
    if let Some(icon) = &template.icon {
        row.add_prefix(&gtk::Image::from_icon_name(icon));
    }

    let export = gtk::Button::from_icon_name("document-save-symbolic");
    export.set_tooltip_text(Some("Export…"));
    export.update_property(&[gtk::accessible::Property::Label("Export…")]);
    export.add_css_class("flat");
    export.set_valign(gtk::Align::Center);
    let text = listed.text.clone();
    let id = template.id.clone();
    let dialog_export = dialog.clone();
    export.connect_clicked(move |_| {
        let chooser = gtk::FileChooserNative::new(
            Some("Export Action"),
            Some(&dialog_export),
            gtk::FileChooserAction::Save,
            Some("_Export"),
            Some("_Cancel"),
        );
        chooser.set_current_name(&format!("{id}.yaml"));
        let text = text.clone();
        let parent = dialog_export.clone();
        chooser.connect_response(move |chooser, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(to) = chooser.file().and_then(|file| file.path()) {
                    match std::fs::write(&to, &text) {
                        Ok(()) => open_in_text_editor(&parent, &to),
                        Err(e) => show_error_dialog(&parent, &format!("Couldn't export the action: {e}")),
                    }
                }
            }
            chooser.destroy();
        });
        show_until_answered(chooser);
    });
    row.add_suffix(&export);

    if listed.own {
        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.set_tooltip_text(Some("Remove"));
        remove.update_property(&[gtk::accessible::Property::Label("Remove")]);
        remove.add_css_class("flat");
        remove.set_valign(gtk::Align::Center);
        let (id, name, replaces) = (template.id.clone(), template.name.clone(), listed.replaces);
        let state_remove = Rc::downgrade(state_rc);
        let dialog_remove = dialog.clone();
        remove.connect_clicked(move |_| {
            let message = if replaces {
                "The action that came with Gestos is offered again in its place."
            } else {
                "Gestures made with it stop working."
            };
            let (id, state_remove, parent, refill) =
                (id.clone(), state_remove.clone(), dialog_remove.clone(), refill.clone());
            show_confirm_dialog(&dialog_remove, &format!("Remove \u{201c}{name}\u{201d}?"), message, "Remove", true, move || {
                let Some(state_rc) = state_remove.upgrade() else { return };
                if change_own_templates(&state_rc, &parent, |own| {
                    own.remove(&id);
                }) {
                    if let Some(refill) = &refill {
                        refill();
                    }
                }
            });
        });
        row.add_suffix(&remove);
    }

    let switch = settings_switch(&template.name);
    switch.set_active(!state_rc.borrow().config.disabled_templates.contains(&template.id));
    let id = template.id.clone();
    let state_switch = Rc::downgrade(state_rc);
    switch.connect_state_set(move |_, enabled| {
        let Some(state_rc) = state_switch.upgrade() else { return glib::Propagation::Proceed };
        {
            let mut state = state_rc.borrow_mut();
            let before = state.config.disabled_templates.clone();
            state.config.disabled_templates.retain(|disabled| *disabled != id);
            if !enabled {
                state.config.disabled_templates.push(id.clone());
            }
            if state.config.save_to_file().is_err() {
                state.config.disabled_templates = before;
                return glib::Propagation::Stop;
            }
        }
        crate::ui::main_window::load_action_options(&state_rc);
        glib::Propagation::Proceed
    });
    row.add_suffix(&switch);
    row.set_activatable_widget(Some(&switch));
    row
}

/// Opens a file in the default text editor: the app set to open plain text, whatever the file's
/// own type, as a .yaml file often has no app of its own. Without one, it's said where the file is.
fn open_in_text_editor(parent: &gtk::Window, path: &std::path::Path) {
    // Started with this, so that it comes to the front, on Wayland too.
    let context = WidgetExt::display(parent).app_launch_context();
    let opened = match gio::AppInfo::default_for_type("text/plain", false) {
        Some(editor) => editor.launch(&[gio::File::for_path(path)], Some(&context)).map_err(|e| e.to_string()),
        None => Err("No app is set to open text files".to_string()),
    };
    if let Err(why) = opened {
        show_error_dialog(parent, &format!("Couldn't open {} in a text editor: {why}", path.display()));
    }
}

/// Shows a file chooser and holds it until it's answered. GTK doesn't hold it: through the portal,
/// the portal's request does, but GTK's own window, used without a portal, closes as soon as
/// nothing holds the chooser.
fn show_until_answered(chooser: gtk::FileChooserNative) {
    let held = RefCell::new(Some(chooser.clone()));
    chooser.connect_response(move |_, _| {
        held.borrow_mut().take();
    });
    chooser.show();
}

/// Changes the user's own action templates in their configuration, and has the daemon and the
/// lists of actions follow. Whether it was saved; if not, it's said why, and nothing changed.
fn change_own_templates(
    state_rc: &Rc<RefCell<AppState>>,
    parent: &gtk::Window,
    change: impl FnOnce(&mut std::collections::BTreeMap<String, String>),
) -> bool {
    let mut state = state_rc.borrow_mut();
    let before = state.config.action_templates.clone();
    change(&mut state.config.action_templates);
    if let Err(e) = state.config.save_to_file() {
        state.config.action_templates = before;
        drop(state);
        show_error_dialog(parent, &format!("Couldn't save your actions: {e}"));
        return false;
    }
    let conn = state.dbus_conn.clone();
    drop(state);
    reload_daemon(conn.as_ref(), Some(parent));
    crate::ui::main_window::load_action_options(state_rc);
    true
}

/// Asks for a template's file, and makes it the user's own, in their configuration. What it runs
/// is said first, and one of their own with the same ID is only replaced once they say so.
fn import_action_template(state_rc: &Rc<RefCell<AppState>>, dialog: &gtk::Window, refill: Rc<dyn Fn()>) {
    let chooser = gtk::FileChooserNative::new(
        Some("Import Action"),
        Some(dialog),
        gtk::FileChooserAction::Open,
        Some("_Import"),
        Some("_Cancel"),
    );
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Actions"));
    filter.add_pattern("*.yaml");
    chooser.add_filter(&filter);
    let state_import = Rc::downgrade(state_rc);
    let parent = dialog.clone();
    chooser.connect_response(move |chooser, response| {
        let path = (response == gtk::ResponseType::Accept).then(|| chooser.file().and_then(|file| file.path())).flatten();
        chooser.destroy();
        let Some(path) = path else { return };
        let (template, text) = match gestos::action_templates::read_to_import(&path) {
            Ok(read) => read,
            Err(why) => {
                show_error_dialog(&parent, &format!("Couldn't import the action. {why}."));
                return;
            }
        };
        let Some(state_rc) = state_import.upgrade() else { return };
        let replaces = state_rc.borrow().config.action_templates.contains_key(&template.id);
        // It runs what's in it only once a gesture is made with it, but that's said first, as
        // importing a configuration does.
        let commands: Vec<String> = template
            .steps
            .iter()
            .filter_map(|step| match step {
                gestos::key_steps::KeyStep::Run(command) => Some(command.text.clone()),
                _ => None,
            })
            .collect();
        let mut message = String::new();
        if replaces {
            message.push_str("An action you imported before has the same ID. It's replaced by this one.");
        }
        if !commands.is_empty() {
            if !message.is_empty() {
                message.push_str("\n\n");
            }
            message.push_str("Its gestures run these commands:\n");
            for command in &commands {
                message.push_str(&format!("\n{command}"));
            }
        }
        let (parent_import, refill, id) = (parent.clone(), Rc::clone(&refill), template.id.clone());
        let import = move || {
            if change_own_templates(&state_rc, &parent_import, |own| {
                own.insert(id, text);
            }) {
                refill();
            }
        };
        if message.is_empty() {
            import();
        } else {
            show_confirm_dialog(
                &parent,
                &format!("Import \u{201c}{}\u{201d}?", template.name),
                &message,
                if replaces { "Replace" } else { "Import" },
                replaces,
                import,
            );
        }
    });
    show_until_answered(chooser);
}

pub fn open_settings_window(state_rc: &Rc<RefCell<AppState>>) {
    let parent = state_rc.borrow().window.clone();
    let preferences = adw::PreferencesWindow::builder()
        .title("Preferences")
        .transient_for(&parent)
        .modal(true)
        .search_enabled(false)
        .default_width(560)
        .build();
    let dialog: gtk::Window = preferences.clone().upcast();
    crate::ui::components::close_with_ctrl_w(&dialog);
    let page = adw::PreferencesPage::builder()
        .title("General")
        .icon_name("preferences-system-symbolic")
        .build();
    preferences.add(&page);
    preferences.add(&actions_page(state_rc, &dialog));

    let on_gnome = state_rc.borrow().extension_status != ExtensionStatus::Unavailable;
    // The extension keeps its settings with it, so they can only be bound while it's installed.
    let extension_settings = extension::settings();
    let mut needs_extension = Vec::new();

    // --- Gestures ---
    let gestures_list = append_section(&page, "Gestures");
    append_autostart_row(&gestures_list);
    let overview_row = overview_click_row(state_rc);
    let middle_button_windows = on_gnome.then(|| middle_button_windows_row(state_rc));
    let right_only = overview_row.iter().chain(&middle_button_windows)
        .map(|(_, switch)| switch.clone().upcast())
        .collect();
    append_button_row(state_rc, &gestures_list, &dialog, right_only);
    if let Some((row, _)) = &overview_row {
        gestures_list.add(row);
    }
    if let Some((row, _)) = &middle_button_windows {
        gestures_list.add(row);
        needs_extension.push(row.clone());
    }
    if on_gnome {
        if let Some(row) = middle_click_paste_row() {
            gestures_list.add(&row);
        }
    }
    // On GNOME the extension draws it, with its own setting; on X11 and KDE, Gestos does.
    if on_gnome {
        let (path_row, path_box, _) =
            settings_row("Show Gesture Path", "Draw each gesture on screen as you draw it");
        let path = settings_switch("Show Gesture Path");
        if let Some(settings) = &extension_settings {
            settings.bind("show-gesture-path", &path, "active").build();
        }
        path_box.append(&path);
        gestures_list.add(&path_row);
        needs_extension.push(path_row);
    } else if gestos::active_app::is_x11_session() || gestos::active_app::is_kde() {
        gestures_list.add(&daemon_path_row(state_rc));
    }
    gestures_list.add(&hide_gestures_row(state_rc));

    if on_gnome {
        // --- Window Tiling ---
        let tiling_list = append_section(&page, "Window Tiling");
        let (auto_row, auto_box, _) =
            settings_row("Tile Automatically", "Tile every workspace, also new ones");
        let auto = settings_switch("Tile Automatically");
        let (gap_row, gap_box, _) = settings_row("Window Gap", "Space around tiled windows");
        let gap = gtk::SpinButton::with_range(0.0, 100.0, 1.0);
        gap.set_valign(gtk::Align::Center);
        gap.update_property(&[gtk::accessible::Property::Label("Window Gap")]);
        if let Some(settings) = extension_settings.clone() {
            settings.bind("tile-automatically", &auto, "active").build();
            settings.bind("window-gap", &gap, "value").build();
            // The bindings need the settings for as long as the controls are there.
            gap.connect_destroy(move |_| {
                let _ = &settings;
            });
        }
        auto_box.append(&auto);
        tiling_list.add(&auto_row);
        gap_box.append(&gap);
        tiling_list.add(&gap_row);
        needs_extension.push(auto_row);
        needs_extension.push(gap_row);

        // --- GNOME Extension ---
        let extension_list = append_section(&page, "GNOME Extension");
        append_extension_row(state_rc, &extension_list, needs_extension);
    }

    // --- Your Gestures ---
    let config_list = append_section(&page, "Your Gestures");

    let (export_row, export_box, _) =
        settings_row("Export Configuration", "Save your gestures to a YAML file");
    let export_btn = gtk::Button::with_mnemonic("_Export…");
    export_btn.set_valign(gtk::Align::Center);
    let parent_clone = dialog.clone();
    export_btn.connect_clicked(move |_| {
        let chooser = gtk::FileChooserNative::new(
            Some("Export Configuration"),
            Some(&parent_clone),
            gtk::FileChooserAction::Save,
            Some("_Export"),
            Some("_Cancel"),
        );
        if let Ok(cwd) = std::env::current_dir() {
            let file = gio::File::for_path(cwd);
            let _ = chooser.set_current_folder(Some(&file));
        } else if let Ok(home) = std::env::var("HOME") {
            let file = gio::File::for_path(home);
            let _ = chooser.set_current_folder(Some(&file));
        }
        // Dated, so that exports made on different days don't replace each other.
        let today = glib::DateTime::now_local().ok().and_then(|now| now.format("%F").ok());
        chooser.set_current_name(&match today {
            Some(today) => format!("Gestos {today}.yaml"),
            None => "Gestos.yaml".to_string(),
        });
        
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("YAML Files"));
        filter.add_pattern("*.yaml");
        filter.add_pattern("*.yml");
        chooser.add_filter(&filter);

        let parent_err_clone = parent_clone.clone();
        chooser.connect_response(move |c, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(file) = c.file() {
                    if let Some(path) = file.path() {
                        let config_path = gestos::config::get_default_config_path();
                        let saved = if config_path.exists() {
                            std::fs::copy(&config_path, &path).map(|_| ()).map_err(|e| e.to_string())
                        } else {
                            let mut temp_cfg = Configuration::load_from_defaults();
                            temp_cfg.user_config_path = path.clone();
                            temp_cfg.save_to_file().map_err(|e| e.to_string())
                        };
                        match saved {
                            Ok(()) => open_in_text_editor(&parent_err_clone, &path),
                            Err(e) => show_error_dialog(&parent_err_clone, &format!("Failed to export config: {}", e)),
                        }
                    }
                }
            }
            c.destroy();
        });
        show_until_answered(chooser);
    });
    export_box.append(&export_btn);
    config_list.add(&export_row);

    // A file from someone else can run anything as this user, so what it runs is shown first.
    fn import_message(commands: &[String]) -> String {
        const SHOWN: usize = 10;
        const LONGEST: usize = 200;
        let mut message = "Are you sure you want to import this configuration? This will overwrite your existing gestures.".to_string();
        if commands.is_empty() {
            return message;
        }
        message.push_str("\n\nIts gestures run these commands:\n");
        for command in commands.iter().take(SHOWN) {
            let shown: String = command.chars().take(LONGEST).collect();
            let more = if shown.len() < command.len() { "…" } else { "" };
            message.push_str(&format!("\n{shown}{more}"));
        }
        if commands.len() > SHOWN {
            message.push_str(&format!("\nand {} more", commands.len() - SHOWN));
        }
        message
    }
    let (import_row, import_box, _) =
        settings_row("Import Configuration", "Load gestures from a YAML file");
    let import_btn = gtk::Button::with_mnemonic("_Import…");
    import_btn.set_valign(gtk::Align::Center);
    let parent_clone = dialog.clone();
    let state_rc_clone = Rc::clone(state_rc);
    import_btn.connect_clicked(move |_| {
        let chooser = gtk::FileChooserNative::new(
            Some("Import Configuration"),
            Some(&parent_clone),
            gtk::FileChooserAction::Open,
            Some("_Import"),
            Some("_Cancel"),
        );
        if let Ok(cwd) = std::env::current_dir() {
            let file = gio::File::for_path(cwd);
            let _ = chooser.set_current_folder(Some(&file));
        } else if let Ok(home) = std::env::var("HOME") {
            let file = gio::File::for_path(home);
            let _ = chooser.set_current_folder(Some(&file));
        }
        
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("YAML Files"));
        filter.add_pattern("*.yaml");
        filter.add_pattern("*.yml");
        chooser.add_filter(&filter);

        let parent_err_clone = parent_clone.clone();
        let state_rc_inner = Rc::clone(&state_rc_clone);
        chooser.connect_response(move |c, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(file) = c.file() {
                    if let Some(path) = file.path() {
                        if let Some(mut new_config) = Configuration::load_from_file(&path) {
                            let parent_confirm = parent_err_clone.clone();
                            let state_rc_confirm = Rc::clone(&state_rc_inner);
                            let message = import_message(&new_config.commands());
                            show_confirm_dialog(
                                &parent_err_clone,
                                "Confirm Import",
                                &message,
                                "Import",
                                false,
                                move || {
                                    let default_path = gestos::config::get_default_config_path();
                                    let imported = std::fs::copy(&path, &default_path).and_then(|_| {
                                        gestos::config::remove_write_access_for_others(&default_path)
                                    });
                                    if let Err(e) = imported {
                                        show_error_dialog(&parent_confirm, &format!("Failed to copy config file: {}", e));
                                    } else {
                                        new_config.user_config_path = default_path;
                                        let mut state = state_rc_confirm.borrow_mut();
                                        state.config = new_config;
                                        drop(state);
                                        refresh_gesture_list(&state_rc_confirm, None);
                                        let state = state_rc_confirm.borrow();
                                        reload_daemon(state.dbus_conn.as_ref(), Some(&parent_confirm));
                                    }
                                }
                            );
                        } else {
                            show_error_dialog(&parent_err_clone, "Invalid configuration file. Please select a valid YAML gestos configuration.");
                        }
                    }
                }
            }
            c.destroy();
        });
        show_until_answered(chooser);
    });
    import_box.append(&import_btn);
    config_list.add(&import_row);

    let (reset_row, reset_box, _) =
        settings_row("Remove All Gestures", "Delete every gesture and start over");
    let reset_btn = gtk::Button::with_mnemonic("_Remove All…");
    reset_btn.set_valign(gtk::Align::Center);
    reset_btn.add_css_class("destructive-action");
    let parent_clone = dialog.clone();
    let state_rc_clone2 = Rc::clone(state_rc);
    reset_btn.connect_clicked(move |_| {
        let parent_confirm = parent_clone.clone();
        let state_rc_inner = Rc::clone(&state_rc_clone2);
        show_confirm_dialog(
            &parent_clone,
            "Remove All Gestures?",
            "Every gesture will be deleted. This can't be undone.",
            "Remove All",
            true,
            move || {
                let default_path = gestos::config::get_default_config_path();
                if default_path.exists() {
                    let _ = std::fs::remove_file(&default_path);
                }
                if let Err(e) = gestos::config::initialize_user_config_if_missing() {
                    show_error_dialog(&parent_confirm, &format!("Failed to reset config: {}", e));
                    return;
                }
                let new_config = Configuration::load_from_defaults();
                let mut state = state_rc_inner.borrow_mut();
                state.config = new_config;
                drop(state);
                refresh_gesture_list(&state_rc_inner, None);
                let state = state_rc_inner.borrow();
                reload_daemon(state.dbus_conn.as_ref(), Some(&parent_confirm));
            }
        );
    });
    reset_box.append(&reset_btn);
    config_list.add(&reset_row);

    // --- Troubleshooting ---
    // For bug reports: the user's setup and what Gestos logged, opened in their text editor.
    let troubleshooting_list = append_section(&page, "Troubleshooting");
    let (debug_row, debug_box, _) =
        settings_row("Debugging Information", "Details about your setup for a bug report");
    let debug_btn = gtk::Button::with_mnemonic("_Create Report");
    debug_btn.set_valign(gtk::Align::Center);
    let debug_parent = dialog.clone();
    debug_btn.connect_clicked(move |_| crate::debug_info::open(&debug_parent));
    debug_box.append(&debug_btn);
    troubleshooting_list.add(&debug_row);

    let about_btn = gtk::Button::with_label("About Gestos");
    about_btn.add_css_class("flat");
    about_btn.set_halign(gtk::Align::Center);
    let about_parent = dialog.clone();
    about_btn.connect_clicked(move |_| {
        crate::ui::main_window::show_about_dialog(&about_parent);
    });
    let about_group = adw::PreferencesGroup::new();
    about_group.add(&about_btn);
    page.add(&about_group);

    preferences.present();
}
