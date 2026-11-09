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
const DRAW_BUTTONS: [(&str, i32); 4] = [("Right", 3), ("Middle", 2), ("Back", 8), ("Forward", 9)];
const DEFAULT_DRAW_BUTTON: i32 = 3;

/// Which mouse button draws gestures. The daemon only picks it as it starts, so a change
/// restarts gestures if they're on.
fn append_button_row(
    state_rc: &Rc<RefCell<AppState>>,
    list: &adw::PreferencesGroup,
    dialog: &gtk::Window,
    overview_row: Option<adw::ActionRow>,
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
        let overview_row = overview_row.clone();
        toggle.connect_toggled(move |toggle| {
            if !toggle.is_active() {
                return;
            }
            if let Some(row) = &overview_row {
                row.set_sensitive(button == DEFAULT_DRAW_BUTTON);
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
fn overview_click_row(state_rc: &Rc<RefCell<AppState>>) -> Option<adw::ActionRow> {
    if !matches!(gestos::config::get_environment_suffix(), Some("gnome" | "kde")) {
        return None;
    }
    let title = "Show Overview With Both Buttons";
    let description = "Hold right, then click left";
    let (row, row_box, subtitle) = settings_row(title, description);
    let switch = settings_switch(title);
    {
        let state = state_rc.borrow();
        switch.set_active(state.config.overview_click);
        row.set_sensitive(state.config.button.unwrap_or(DEFAULT_DRAW_BUTTON) == DEFAULT_DRAW_BUTTON);
    }
    let state_rc = Rc::clone(state_rc);
    switch.connect_state_set(move |switch, enabled| {
        let mut state = state_rc.borrow_mut();
        state.config.overview_click = enabled;
        if let Err(e) = state.config.save_to_file() {
            state.config.overview_click = !enabled;
            subtitle.set_text(&format!("Could not change this setting: {e}"));
            return glib::Propagation::Stop;
        }
        subtitle.set_text(description);
        let parent = switch.root().and_downcast::<gtk::Window>();
        reload_daemon(state.dbus_conn.as_ref(), parent.as_ref());
        glib::Propagation::Proceed
    });
    row_box.append(&switch);
    Some(row)
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
    let page = adw::PreferencesPage::new();
    preferences.add(&page);

    let on_gnome = state_rc.borrow().extension_status != ExtensionStatus::Unavailable;
    // The extension keeps its settings with it, so they can only be bound while it's installed.
    let extension_settings = extension::settings();
    let mut needs_extension = Vec::new();

    // --- Gestures ---
    let gestures_list = append_section(&page, "Gestures");
    append_autostart_row(&gestures_list);
    let overview_row = overview_click_row(state_rc);
    append_button_row(state_rc, &gestures_list, &dialog, overview_row.clone());
    if let Some(row) = &overview_row {
        gestures_list.add(row);
    }
    // The extension draws it, so it's only on GNOME.
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
    }

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
    let parent_clone = parent.clone();
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
        chooser.set_current_name("gestos.yaml");
        
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
                        if config_path.exists() {
                            if let Err(e) = std::fs::copy(&config_path, &path) {
                                show_error_dialog(&parent_err_clone, &format!("Failed to export config: {}", e));
                            }
                        } else {
                            let default_cfg = Configuration::load_from_defaults();
                            let mut temp_cfg = default_cfg;
                            temp_cfg.user_config_path = path;
                            if let Err(e) = temp_cfg.save_to_file() {
                                show_error_dialog(&parent_err_clone, &format!("Failed to export config: {}", e));
                            }
                        }
                    }
                }
            }
            c.destroy();
        });
        chooser.show();
    });
    export_box.append(&export_btn);
    config_list.add(&export_row);

    let (import_row, import_box, _) =
        settings_row("Import Configuration", "Load gestures from a YAML file");
    let import_btn = gtk::Button::with_mnemonic("_Import…");
    import_btn.set_valign(gtk::Align::Center);
    let parent_clone = parent.clone();
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
                            show_confirm_dialog(
                                &parent_err_clone,
                                "Confirm Import",
                                "Are you sure you want to import this configuration? This will overwrite your existing gestures.",
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
        chooser.show();
    });
    import_box.append(&import_btn);
    config_list.add(&import_row);

    let (reset_row, reset_box, _) =
        settings_row("Remove All Gestures", "Delete every gesture and start over");
    let reset_btn = gtk::Button::with_mnemonic("_Remove All…");
    reset_btn.set_valign(gtk::Align::Center);
    reset_btn.add_css_class("destructive-action");
    let parent_clone = parent.clone();
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
