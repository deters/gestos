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
use crate::system::{get_autostart_file_path, mark_autostart_decided, set_autostart_enabled, is_osd_enabled, set_osd_enabled, get_overlay_autostart_file_path, set_overlay_enabled};

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

/// The GNOME extension: whether it's running, a switch for it, and its settings.
fn append_extension_section(state_rc: &Rc<RefCell<AppState>>, main_box: &gtk::Box) {
    let header = gtk::Label::new(Some("GNOME Extension"));
    header.add_css_class("section-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_top(12);
    header.set_margin_bottom(4);
    main_box.append(&header);

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    main_box.append(&list);

    let status = state_rc.borrow().extension_status.clone();
    let (status_row, status_box, status_subtitle) =
        // Named as GNOME's Extensions app lists it.
        settings_row("Gestos", &status.description());
    if status.can_switch() {
        let switch = gtk::Switch::new();
        switch.set_valign(gtk::Align::Center);
        switch.set_active(status != ExtensionStatus::Off);
        switch.update_property(&[gtk::accessible::Property::Label("Gestos GNOME extension")]);
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
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
                crate::ui::main_window::refresh_extension_status(&state_after);
                subtitle.set_text(&state_after.borrow().extension_status.description());
            });
            glib::Propagation::Proceed
        });
        status_box.append(&switch);
    }
    list.append(&status_row);

    // The extension keeps its settings with it, so they're only here while it's installed.
    if let Some(settings) = extension::settings() {
        let (auto_row, auto_box, _) =
            settings_row("Tile Automatically", "Tile every workspace, also new ones");
        let auto = gtk::Switch::new();
        auto.set_valign(gtk::Align::Center);
        auto.update_property(&[gtk::accessible::Property::Label("Tile Automatically")]);
        settings.bind("tile-automatically", &auto, "active").build();
        auto_box.append(&auto);
        list.append(&auto_row);

        let (gap_row, gap_box, _) = settings_row(
            "Window Gap",
            "Space around tiled windows",
        );
        let gap = gtk::SpinButton::with_range(0.0, 100.0, 1.0);
        gap.set_valign(gtk::Align::Center);
        gap.update_property(&[gtk::accessible::Property::Label("Window Gap")]);
        settings.bind("window-gap", &gap, "value").build();
        // The bindings need the settings for as long as the controls are there.
        gap.connect_destroy(move |_| {
            let _ = &settings;
        });
        gap_box.append(&gap);
        list.append(&gap_row);
    }
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

    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    main_box.add_css_class("dialog-content");
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);
    dialog.set_child(Some(&main_box));

    // --- General Section ---
    let general_header = gtk::Label::new(Some("General"));
    general_header.add_css_class("section-header");
    general_header.set_halign(gtk::Align::Start);
    general_header.set_margin_bottom(4);
    main_box.append(&general_header);

    let general_list = gtk::ListBox::new();
    general_list.add_css_class("boxed-list");
    general_list.set_selection_mode(gtk::SelectionMode::None);
    main_box.append(&general_list);

    // Start at login row
    let autostart_row = gtk::ListBoxRow::new();
    autostart_row.set_activatable(false);
    let autostart_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    autostart_box.set_margin_start(16);
    autostart_box.set_margin_end(16);
    autostart_box.set_margin_top(12);
    autostart_box.set_margin_bottom(12);

    let autostart_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    autostart_text.set_hexpand(true);
    autostart_text.set_halign(gtk::Align::Start);

    let autostart_title = gtk::Label::new(Some("Start When I Log In"));
    autostart_title.add_css_class("status-label");
    autostart_title.set_halign(gtk::Align::Start);
    autostart_text.append(&autostart_title);

    let manual_autostart =
        gestos::config::manual_autostart_for(gestos::config::get_environment_suffix());
    let autostart_subtitle = gtk::Label::new(None);
    autostart_subtitle.add_css_class("action-label");
    autostart_subtitle.set_halign(gtk::Align::Start);
    autostart_subtitle.set_xalign(0.0);
    autostart_subtitle.set_wrap(true);
    autostart_text.append(&autostart_subtitle);
    autostart_box.append(&autostart_text);

    match manual_autostart {
        Some((desktop, config_file, line)) => {
            autostart_subtitle.set_text(&format!(
                "{desktop} doesn't use this setting. Add this line to {config_file}:"
            ));
            let config_line = gtk::Label::new(Some(line));
            config_line.add_css_class("monospace");
            config_line.set_halign(gtk::Align::Start);
            config_line.set_selectable(true);
            autostart_text.append(&config_line);
        }
        None => {
            autostart_subtitle.set_text("Start gestures automatically");
            let autostart_switch = gtk::Switch::new();
            autostart_switch.set_valign(gtk::Align::Center);
            autostart_switch
                .set_active(get_autostart_file_path().is_some_and(|path| path.exists()));
            let subtitle = autostart_subtitle.clone();
            autostart_switch.connect_state_set(move |_, enabled| {
                let changed = mark_autostart_decided().and_then(|_| set_autostart_enabled(enabled));
                match changed {
                    Ok(()) => subtitle.set_text("Start gestures automatically"),
                    Err(e) => subtitle.set_text(&format!("Could not change this setting: {e}")),
                }
                glib::Propagation::Proceed
            });
            autostart_box.append(&autostart_switch);
        }
    }
    autostart_row.set_child(Some(&autostart_box));
    general_list.append(&autostart_row);

    // Overlay (Gesture Trail) row
    let overlay_row = gtk::ListBoxRow::new();
    let overlay_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    overlay_box.set_margin_start(16);
    overlay_box.set_margin_end(16);
    overlay_box.set_margin_top(12);
    overlay_box.set_margin_bottom(12);

    let overlay_text_vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    overlay_text_vbox.set_hexpand(true);
    overlay_text_vbox.set_halign(gtk::Align::Start);

    let overlay_title = gtk::Label::new(Some("Show Gesture Trail"));
    overlay_title.add_css_class("status-label");
    overlay_title.set_halign(gtk::Align::Start);
    overlay_text_vbox.append(&overlay_title);

    let overlay_subtitle = gtk::Label::new(Some("Show visual feedback trail while drawing"));
    overlay_subtitle.add_css_class("action-label");
    overlay_subtitle.set_halign(gtk::Align::Start);
    overlay_text_vbox.append(&overlay_subtitle);

    overlay_box.append(&overlay_text_vbox);

    let overlay_switch = gtk::Switch::new();
    overlay_switch.set_valign(gtk::Align::Center);

    let overlay_file = get_overlay_autostart_file_path();
    let is_overlay_enabled = overlay_file.map(|p| p.exists()).unwrap_or(false);
    overlay_switch.set_active(is_overlay_enabled);

    overlay_switch.connect_state_set(move |_, state| {
        set_overlay_enabled(state);
        glib::Propagation::Proceed
    });

    overlay_box.append(&overlay_switch);
    overlay_row.set_child(Some(&overlay_box));
    overlay_row.set_visible(false); // HIDE FOR NOW
    general_list.append(&overlay_row);

    // Action Notifications (OSD) row
    let osd_row = gtk::ListBoxRow::new();
    let osd_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    osd_box.set_margin_start(16);
    osd_box.set_margin_end(16);
    osd_box.set_margin_top(12);
    osd_box.set_margin_bottom(12);

    let osd_text_vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    osd_text_vbox.set_hexpand(true);
    osd_text_vbox.set_halign(gtk::Align::Start);

    let osd_title_lbl = gtk::Label::new(Some("Action Notifications"));
    osd_title_lbl.add_css_class("status-label");
    osd_title_lbl.set_halign(gtk::Align::Start);
    osd_text_vbox.append(&osd_title_lbl);

    let osd_subtitle_lbl = gtk::Label::new(Some("Show a brief popup window when an action executes"));
    osd_subtitle_lbl.add_css_class("action-label");
    osd_subtitle_lbl.set_halign(gtk::Align::Start);
    osd_text_vbox.append(&osd_subtitle_lbl);

    osd_box.append(&osd_text_vbox);

    let osd_switch = gtk::Switch::new();
    osd_switch.set_valign(gtk::Align::Center);
    osd_switch.set_active(is_osd_enabled());

    osd_switch.connect_state_set(move |_, state| {
        set_osd_enabled(state);
        glib::Propagation::Proceed
    });

    osd_box.append(&osd_switch);
    osd_row.set_child(Some(&osd_box));
    osd_row.set_visible(false); // HIDE FOR NOW
    general_list.append(&osd_row);

    // Reduce Notifications row
    let reduce_row = gtk::ListBoxRow::new();
    reduce_row.set_activatable(false);
    let reduce_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    reduce_box.set_margin_start(16);
    reduce_box.set_margin_end(16);
    reduce_box.set_margin_top(12);
    reduce_box.set_margin_bottom(12);

    let reduce_text_vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    reduce_text_vbox.set_hexpand(true);
    reduce_text_vbox.set_halign(gtk::Align::Start);

    let reduce_title = gtk::Label::new(Some("Reduce Notifications"));
    reduce_title.add_css_class("status-label");
    reduce_title.set_halign(gtk::Align::Start);
    reduce_text_vbox.append(&reduce_title);

    let reduce_description = "Don't show a notification each time a gesture runs";
    let reduce_subtitle = gtk::Label::new(Some(reduce_description));
    reduce_subtitle.add_css_class("action-label");
    reduce_subtitle.set_halign(gtk::Align::Start);
    reduce_subtitle.set_xalign(0.0);
    reduce_subtitle.set_wrap(true);
    reduce_text_vbox.append(&reduce_subtitle);

    reduce_box.append(&reduce_text_vbox);

    let reduce_switch = gtk::Switch::new();
    reduce_switch.set_valign(gtk::Align::Center);
    reduce_switch.set_active(state_rc.borrow().config.reduce_notifications);

    let state_rc_reduce = Rc::clone(state_rc);
    let dialog_reduce = dialog.clone();
    reduce_switch.connect_state_set(move |_, enabled| {
        let mut state = state_rc_reduce.borrow_mut();
        if state.config.reduce_notifications != enabled {
            state.config.reduce_notifications = enabled;
            match state.config.save_to_file() {
                Ok(()) => {
                    reduce_subtitle.set_text(reduce_description);
                    reload_daemon(state.dbus_conn.as_ref(), Some(&dialog_reduce));
                }
                Err(e) => reduce_subtitle.set_text(&format!("Could not change this setting: {e}")),
            }
        }
        glib::Propagation::Proceed
    });

    reduce_box.append(&reduce_switch);
    reduce_row.set_child(Some(&reduce_box));
    general_list.append(&reduce_row);

    // --- GNOME Extension Section ---
    // Only on GNOME, where the extension can exist. Elsewhere its actions aren't offered at all.
    if state_rc.borrow().extension_status != ExtensionStatus::Unavailable {
        append_extension_section(state_rc, &main_box);
    }

    // --- Backup & Restore Section ---
    let maintenance_header = gtk::Label::new(Some("Backup & Restore"));
    maintenance_header.add_css_class("section-header");
    maintenance_header.set_halign(gtk::Align::Start);
    maintenance_header.set_margin_top(12);
    maintenance_header.set_margin_bottom(4);
    main_box.append(&maintenance_header);

    let maintenance_list = gtk::ListBox::new();
    maintenance_list.add_css_class("boxed-list");
    maintenance_list.set_selection_mode(gtk::SelectionMode::None);
    main_box.append(&maintenance_list);

    // Export Row
    let export_row = gtk::ListBoxRow::new();
    let export_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    export_box.set_margin_start(16);
    export_box.set_margin_end(16);
    export_box.set_margin_top(12);
    export_box.set_margin_bottom(12);

    let export_text_vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    export_text_vbox.set_hexpand(true);
    export_text_vbox.set_halign(gtk::Align::Start);

    let export_title = gtk::Label::new(Some("Export Configuration"));
    export_title.add_css_class("status-label");
    export_title.set_halign(gtk::Align::Start);
    export_text_vbox.append(&export_title);

    let export_subtitle = gtk::Label::new(Some("Save gestures settings to a YAML file"));
    export_subtitle.add_css_class("action-label");
    export_subtitle.set_halign(gtk::Align::Start);
    export_text_vbox.append(&export_subtitle);

    export_box.append(&export_text_vbox);

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
    export_row.set_child(Some(&export_box));
    maintenance_list.append(&export_row);

    // Import Row
    let import_row = gtk::ListBoxRow::new();
    let import_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    import_box.set_margin_start(16);
    import_box.set_margin_end(16);
    import_box.set_margin_top(12);
    import_box.set_margin_bottom(12);

    let import_text_vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    import_text_vbox.set_hexpand(true);
    import_text_vbox.set_halign(gtk::Align::Start);

    let import_title = gtk::Label::new(Some("Import Configuration"));
    import_title.add_css_class("status-label");
    import_title.set_halign(gtk::Align::Start);
    import_text_vbox.append(&import_title);

    let import_subtitle = gtk::Label::new(Some("Load gestures settings from a YAML file"));
    import_subtitle.add_css_class("action-label");
    import_subtitle.set_halign(gtk::Align::Start);
    import_text_vbox.append(&import_subtitle);

    import_box.append(&import_text_vbox);

    let import_btn = gtk::Button::with_mnemonic("_Import…");
    import_btn.set_valign(gtk::Align::Center);

    let parent_clone = parent.clone();
    let state_rc_clone = Rc::clone(state_rc);
    let reduce_switch_import = reduce_switch.clone();
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
        let reduce_switch_response = reduce_switch_import.clone();
        chooser.connect_response(move |c, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(file) = c.file() {
                    if let Some(path) = file.path() {
                        if let Some(mut new_config) = Configuration::load_from_file(&path) {
                            let parent_confirm = parent_err_clone.clone();
                            let state_rc_confirm = Rc::clone(&state_rc_inner);
                            let reduce_switch_confirm = reduce_switch_response.clone();
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
                                        let reduce = state_rc_confirm.borrow().config.reduce_notifications;
                                        reduce_switch_confirm.set_active(reduce);
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
    import_row.set_child(Some(&import_box));
    maintenance_list.append(&import_row);

    // Reset Row
    let reset_row = gtk::ListBoxRow::new();
    let reset_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    reset_box.set_margin_start(16);
    reset_box.set_margin_end(16);
    reset_box.set_margin_top(12);
    reset_box.set_margin_bottom(12);

    let reset_text_vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
    reset_text_vbox.set_hexpand(true);
    reset_text_vbox.set_halign(gtk::Align::Start);

    let reset_title = gtk::Label::new(Some("Remove All Gestures"));
    reset_title.add_css_class("status-label");
    reset_title.set_halign(gtk::Align::Start);
    reset_text_vbox.append(&reset_title);

    let reset_subtitle = gtk::Label::new(Some("Delete every gesture and start over"));
    reset_subtitle.add_css_class("action-label");
    reset_subtitle.set_halign(gtk::Align::Start);
    reset_text_vbox.append(&reset_subtitle);

    reset_box.append(&reset_text_vbox);

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
                let mut new_config = Configuration::load_from_defaults();
                let mut state = state_rc_inner.borrow_mut();
                // This removes gestures, and a preference isn't one of them.
                if state.config.reduce_notifications {
                    new_config.reduce_notifications = true;
                    if let Err(e) = new_config.save_to_file() {
                        show_error_dialog(&parent_confirm, &format!("Failed to reset config: {}", e));
                    }
                }
                state.config = new_config;
                drop(state);
                refresh_gesture_list(&state_rc_inner, None);
                let state = state_rc_inner.borrow();
                reload_daemon(state.dbus_conn.as_ref(), Some(&parent_confirm));
            }
        );
    });

    reset_box.append(&reset_btn);
    reset_row.set_child(Some(&reset_box));
    maintenance_list.append(&reset_row);

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
