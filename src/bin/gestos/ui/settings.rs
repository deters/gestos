use gtk::prelude::*;
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

/// A row with a title, a subtitle under it, and room for a control at its end.
fn settings_row(title: &str, subtitle: &str) -> (gtk::ListBoxRow, gtk::Box, gtk::Label) {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_halign(gtk::Align::Start);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("status-label");
    title_label.set_halign(gtk::Align::Start);
    text.append(&title_label);
    let subtitle_label = gtk::Label::new(Some(subtitle));
    subtitle_label.add_css_class("action-label");
    subtitle_label.set_halign(gtk::Align::Start);
    subtitle_label.set_xalign(0.0);
    subtitle_label.set_wrap(true);
    text.append(&subtitle_label);
    row_box.append(&text);

    row.set_child(Some(&row_box));
    (row, row_box, subtitle_label)
}

/// A titled group of rows, as GNOME's settings pages have them.
fn append_section(main_box: &gtk::Box, title: &str) -> gtk::ListBox {
    let header = gtk::Label::new(Some(title));
    header.add_css_class("section-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_bottom(4);
    if main_box.first_child().is_some() {
        header.set_margin_top(12);
    }
    main_box.append(&header);

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    main_box.append(&list);
    list
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

fn append_autostart_row(list: &gtk::ListBox) {
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
            config_line.set_halign(gtk::Align::Start);
            config_line.set_selectable(true);
            if let Some(text) = subtitle.parent().and_downcast::<gtk::Box>() {
                text.append(&config_line);
            }
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
    list.append(&row);
}

/// The mouse buttons gestures can be drawn with, numbered as in `gestosd --button`.
const DRAW_BUTTONS: [(&str, i32); 4] = [("Right", 3), ("Middle", 2), ("Back", 8), ("Forward", 9)];
const DEFAULT_DRAW_BUTTON: i32 = 3;

/// Which mouse button draws gestures. The daemon only picks it as it starts, so a change
/// restarts gestures if they're on.
fn append_button_row(state_rc: &Rc<RefCell<AppState>>, list: &gtk::ListBox, dialog: &gtk::Window) {
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
        toggle.connect_toggled(move |toggle| {
            if !toggle.is_active() {
                return;
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
    list.append(&row);
}


/// The extension's status, with a switch for it. The rows for what it does follow it, as they
/// can only be changed while it runs.
fn append_extension_row(
    state_rc: &Rc<RefCell<AppState>>,
    list: &gtk::ListBox,
    needs_extension: Vec<gtk::ListBoxRow>,
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
    list.append(&status_row);
}

pub fn open_settings_window(state_rc: &Rc<RefCell<AppState>>) {
    let parent = state_rc.borrow().window.clone();
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(&parent));
    dialog.set_modal(true);
    dialog.set_title(Some("Preferences"));
    dialog.set_default_size(380, -1);

    let esc_controller = gtk::EventControllerKey::new();
    let dialog_esc = dialog.clone();
    esc_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            dialog_esc.destroy();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    dialog.add_controller(esc_controller);

    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(true);
    let title_label = gtk::Label::new(Some("Preferences"));
    title_label.add_css_class("title");
    header.set_title_widget(Some(&title_label));
    dialog.set_titlebar(Some(&header));

    // As tall as what's in it, and scrolling once that's more than the main window.
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_propagate_natural_height(true);
    scrolled.set_max_content_height(parent.height().max(480));
    dialog.set_child(Some(&scrolled));

    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    main_box.add_css_class("dialog-content");
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);
    scrolled.set_child(Some(&main_box));

    let on_gnome = state_rc.borrow().extension_status != ExtensionStatus::Unavailable;
    // The extension keeps its settings with it, so they can only be bound while it's installed.
    let extension_settings = extension::settings();
    let mut needs_extension = Vec::new();

    // --- Gestures ---
    let gestures_list = append_section(&main_box, "Gestures");
    append_autostart_row(&gestures_list);
    append_button_row(state_rc, &gestures_list, &dialog);
    // The extension draws it, so it's only on GNOME.
    if on_gnome {
        let (path_row, path_box, _) =
            settings_row("Show Gesture Path", "Draw each gesture on screen as you draw it");
        let path = settings_switch("Show Gesture Path");
        if let Some(settings) = &extension_settings {
            settings.bind("show-gesture-path", &path, "active").build();
        }
        path_box.append(&path);
        gestures_list.append(&path_row);
        needs_extension.push(path_row);
    }

    if on_gnome {
        // --- Window Tiling ---
        let tiling_list = append_section(&main_box, "Window Tiling");
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
        tiling_list.append(&auto_row);
        gap_box.append(&gap);
        tiling_list.append(&gap_row);
        needs_extension.push(auto_row);
        needs_extension.push(gap_row);

        // --- GNOME Extension ---
        let extension_list = append_section(&main_box, "GNOME Extension");
        append_extension_row(state_rc, &extension_list, needs_extension);
    }

    // --- Your Gestures ---
    let config_list = append_section(&main_box, "Your Gestures");

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
    config_list.append(&export_row);

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
    config_list.append(&import_row);

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
    config_list.append(&reset_row);

    // --- Troubleshooting ---
    // For bug reports: the user's setup and what Gestos logged, opened in their text editor.
    let troubleshooting_list = append_section(&main_box, "Troubleshooting");
    let (debug_row, debug_box, _) =
        settings_row("Debugging Information", "Details about your setup for a bug report");
    let debug_btn = gtk::Button::with_mnemonic("_Create Report");
    debug_btn.set_valign(gtk::Align::Center);
    let debug_parent = dialog.clone();
    debug_btn.connect_clicked(move |_| crate::debug_info::open(&debug_parent));
    debug_box.append(&debug_btn);
    troubleshooting_list.append(&debug_row);

    let about_btn = gtk::Button::with_label("About Gestos");
    about_btn.add_css_class("flat");
    about_btn.set_halign(gtk::Align::Center);
    about_btn.set_margin_top(12);
    let about_parent = dialog.clone();
    about_btn.connect_clicked(move |_| {
        crate::ui::main_window::show_about_dialog(&about_parent);
    });
    main_box.append(&about_btn);

    dialog.present();
}
