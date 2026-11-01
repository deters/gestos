use gtk::prelude::*;
use gtk::{gdk, glib};
use crate::ui::components::get_action_human_readable;
use gtk4 as gtk;
use std::rc::Rc;
use std::cell::RefCell;
use gestos::config::{Configuration, Gesture};
use crate::state::AppState;
use crate::actions::{fetch_gnome_action_options, fetch_kde_action_options, get_static_action_options};
use crate::suggestions::find_suggestions;
use crate::ui::components::get_action_category_icon;
use crate::ui::editor::{open_gesture_editor, open_gesture_editor_for_action};
use crate::ui::settings::open_settings_window;
use crate::ui::components::create_gesture_row;
use crate::ui::gesture_status::GestureStatus;

pub fn get_visible_gestures(state: &AppState) -> Vec<Gesture> {
    let filter = state.search_entry.text().to_lowercase();

    let gestures: Vec<Gesture> = state
        .config
        .gestures
        .iter()
        .filter(|g| !g.is_deleted)
        .filter(|g| {
            if filter.is_empty() {
                true
            } else {
                g.name.to_lowercase().contains(&filter) || (
                    !g.actions.is_empty() && get_action_human_readable(&g.actions[0]).to_lowercase().contains(&filter)
                )
            }
        })
        .cloned()
        .collect();

    gestures
}
fn suggestion_row(option: &crate::state::EditorActionOption) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);

    let (icon_name, _) = get_action_category_icon(&option.action_type);
    row_box.append(&gtk::Image::from_icon_name(icon_name));
    let name = gtk::Label::new(Some(&option.name));
    name.set_hexpand(true);
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row_box.append(&name);
    row_box.append(&gtk::Image::from_icon_name("go-next-symbolic"));

    row.set_child(Some(&row_box));
    row
}

/// Below the search results, offers actions that match the search but no gesture uses yet.
fn refresh_suggestions(state_rc: &Rc<RefCell<AppState>>, no_gestures_shown: bool) {
    let mut state = state_rc.borrow_mut();
    while let Some(child) = state.suggestion_list.first_child() {
        state.suggestion_list.remove(&child);
    }

    let query = state.search_entry.text().to_string();
    let found = find_suggestions(&state.action_options, &state.config.gestures, &query);
    for option in &found {
        state.suggestion_list.append(&suggestion_row(option));
    }

    state
        .suggestion_no_match
        .set_text(&format!("No gestures match \u{201c}{}\u{201d}.", query.trim()));
    state
        .suggestion_no_match
        .set_visible(no_gestures_shown && !query.trim().is_empty());
    state.suggestion_box.set_visible(!found.is_empty());
    if no_gestures_shown && !found.is_empty() {
        state.empty_state_box.set_visible(false);
    }
    state.visible_suggestions = found;
}

pub fn refresh_gesture_list(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) {
    let no_gestures_shown = show_gestures(state_rc, select_name);
    refresh_suggestions(state_rc, no_gestures_shown);
}

fn show_gestures(state_rc: &Rc<RefCell<AppState>>, select_name: Option<&str>) -> bool {
    let state = state_rc.borrow();

    // Clear list
    while let Some(child) = state.main_list.first_child() {
        state.main_list.remove(&child);
    }

    let visible = get_visible_gestures(&state);

    if visible.is_empty() {
        state.empty_state_box.set_visible(true);
        state.main_list.set_visible(false);
    } else {
        state.empty_state_box.set_visible(false);
        state.main_list.set_visible(true);

        for gesture in &visible {
            let row = create_gesture_row(gesture, state_rc);
            state.main_list.append(&row);

            if let Some(name) = select_name {
                if gesture.name == name {
                    state.main_list.select_row(Some(&row));
                }
            }
        }
    }
    visible.is_empty()
}
pub fn show_about_dialog<W: IsA<gtk::Window>>(parent: &W) {
    let dialog = gtk::AboutDialog::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_program_name(Some("Gestos"));
    dialog.set_version(Some("5.0.0"));
    dialog.set_comments(Some(
        "A modern mouse gestures editor for Wayland desktop environments.",
    ));
    dialog.set_authors(&["Lucas Augusto Deters <lucasdeters@gmail.com>"]);
    dialog.present();
}

pub fn build_ui(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some("Gestos"));
    // Wide enough for the header to keep its title centered with any status.
    window.set_default_size(700, 700);

    let header = gtk::HeaderBar::new();
    window.set_titlebar(Some(&header));

    // 1. Add gesture button (Primary action on the left)
    let add_gest_btn = gtk::Button::from_icon_name("list-add-symbolic");
    add_gest_btn.set_tooltip_text(Some("New Gesture"));
    add_gest_btn.update_property(&[gtk::accessible::Property::Label("New Gesture")]);
    header.pack_start(&add_gest_btn);

    // 2. Preferences button
    let settings_btn = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_btn.set_tooltip_text(Some("Preferences"));
    settings_btn.update_property(&[gtk::accessible::Property::Label("Preferences")]);
    header.pack_end(&settings_btn);

    // Content VBox
    let content_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_vbox.add_css_class("main-window-content");
    window.set_child(Some(&content_vbox));

    // The search stays visible, since it's also how new gestures start. Typing anywhere in the
    // window goes to it.
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_halign(gtk::Align::Center);
    search_entry.set_width_request(500);
    search_entry.set_margin_top(12);
    search_entry.set_margin_bottom(12);
    search_entry.set_margin_start(24);
    search_entry.set_margin_end(24);
    search_entry.set_placeholder_text(Some("Search gestures and actions…"));
    search_entry.set_key_capture_widget(Some(&window));
    search_entry.connect_stop_search(|entry| entry.set_text(""));
    content_vbox.append(&search_entry);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    content_vbox.append(&scrolled);

    let list_container = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let main_list = gtk::ListBox::new();
    main_list.set_halign(gtk::Align::Center);
    main_list.set_width_request(500);
    main_list.set_margin_top(0);
    main_list.set_margin_bottom(24);
    main_list.set_margin_start(24);
    main_list.set_margin_end(24);
    main_list.add_css_class("boxed-list");
    main_list.set_selection_mode(gtk::SelectionMode::None);
    list_container.append(&main_list);

    let empty_state_box = gtk::Box::new(gtk::Orientation::Vertical, 16);
    empty_state_box.set_valign(gtk::Align::Center);
    empty_state_box.set_halign(gtk::Align::Center);
    empty_state_box.set_margin_top(72);
    empty_state_box.set_margin_bottom(72);

    let empty_icon = gtk::Image::from_icon_name("view-list-bullet-symbolic");
    empty_icon.set_pixel_size(80);
    empty_icon.set_opacity(0.4);
    empty_state_box.append(&empty_icon);

    let empty_title = gtk::Label::new(None);
    empty_title.set_markup("<span size='large' weight='bold'>No Gestures</span>");
    empty_state_box.append(&empty_title);

    let empty_subtitle = gtk::Label::new(Some("Search for an action above, or click + to create a gesture."));
    empty_subtitle.set_opacity(0.65);
    empty_state_box.append(&empty_subtitle);

    list_container.append(&empty_state_box);

    let suggestion_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    suggestion_box.set_halign(gtk::Align::Center);
    suggestion_box.set_width_request(500);
    suggestion_box.set_margin_start(24);
    suggestion_box.set_margin_end(24);
    suggestion_box.set_margin_bottom(24);
    suggestion_box.set_visible(false);

    let suggestion_no_match = gtk::Label::new(None);
    suggestion_no_match.set_xalign(0.0);
    suggestion_no_match.set_margin_top(24);
    suggestion_no_match.set_margin_bottom(8);
    suggestion_box.append(&suggestion_no_match);

    let suggestion_heading = gtk::Label::new(Some("Create a Gesture For"));
    suggestion_heading.add_css_class("section-header");
    suggestion_heading.set_xalign(0.0);
    suggestion_box.append(&suggestion_heading);

    let suggestion_list = gtk::ListBox::new();
    suggestion_list.add_css_class("boxed-list");
    suggestion_list.set_selection_mode(gtk::SelectionMode::None);
    suggestion_box.append(&suggestion_list);

    list_container.append(&suggestion_box);
    scrolled.set_child(Some(&list_container));

    if let Err(e) = gestos::config::initialize_user_config_if_missing() {
        eprintln!("Warning: Failed to initialize configuration: {}", e);
    }
    let config = Configuration::load_from_defaults();
    let dbus_conn = zbus::blocking::Connection::session().ok();

    let search_entry_widget = search_entry.clone();
    let gesture_status = GestureStatus::new("Gestures", dbus_conn.clone());
    header.set_title_widget(Some(gesture_status.title()));
    header.pack_end(gesture_status.action_button());
    content_vbox.insert_child_after(gesture_status.error_area(), Some(&search_entry_widget));
    content_vbox.insert_child_after(gesture_status.notice_area(), Some(gesture_status.error_area()));

    let state = Rc::new(RefCell::new(AppState {
        config,
        main_list,
        search_entry,
        window,
        newly_added_gestures: Vec::new(),
        dbus_conn,
        empty_state_box,
        action_options: Vec::new(),
        visible_suggestions: Vec::new(),
        suggestion_box,
        suggestion_no_match,
        suggestion_list,
    }));

    // Refresh initially
    refresh_gesture_list(&state, None);

    // Connect search entry
    let state_clone = Rc::clone(&state);
    state
        .borrow()
        .search_entry
        .connect_search_changed(move |_| {
            refresh_gesture_list(&state_clone, None);
        });

    let state_suggestion = Rc::clone(&state);
    state
        .borrow()
        .suggestion_list
        .connect_row_activated(move |_, row| {
            let option = state_suggestion
                .borrow()
                .visible_suggestions
                .get(row.index() as usize)
                .cloned();
            if let Some(option) = option {
                open_gesture_editor_for_action(&state_suggestion, option);
            }
        });

    // Reading the desktop's action lists can be slow, so it doesn't happen on the UI thread.
    let load_options = gtk::gio::spawn_blocking(|| {
        let mut options = get_static_action_options();
        options.extend(fetch_gnome_action_options());
        options.extend(fetch_kde_action_options());
        options
    });
    let state_options = Rc::clone(&state);
    glib::spawn_future_local(async move {
        if let Ok(options) = load_options.await {
            state_options.borrow_mut().action_options = options;
            refresh_gesture_list(&state_options, None);
        }
    });

    // Connect Add button
    let state_clone2 = Rc::clone(&state);
    add_gest_btn.connect_clicked(move |_| {
        open_gesture_editor(&state_clone2, None);
    });

    // Connect row activation for editing
    let state_clone3 = Rc::clone(&state);
    state
        .borrow()
        .main_list
        .connect_row_activated(move |_, row| {
            let idx = row.index();
            let state_borrow = state_clone3.borrow();

            let visible_gestures = get_visible_gestures(&state_borrow);

            if idx >= 0 && (idx as usize) < visible_gestures.len() {
                let gesture = visible_gestures[idx as usize].clone();
                drop(state_borrow);
                open_gesture_editor(&state_clone3, Some(gesture));
            }
        });

    // Connect settings button
    let state_settings_clone = Rc::clone(&state);
    settings_btn.connect_clicked(move |_| {
        open_settings_window(&state_settings_clone);
    });

    // Global Keyboard Shortcuts
    let key_controller = gtk::EventControllerKey::new();
    let state_shortcut_clone = Rc::clone(&state);
    let window_shortcut_clone = state.borrow().window.clone();
    key_controller.connect_key_pressed(move |_, keyval, _, state_modifier| {
        let is_ctrl = state_modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK) 
                   || state_modifier.contains(gtk::gdk::ModifierType::META_MASK); // meta/cmd on mac
        
        if is_ctrl && keyval == gtk::gdk::Key::n {
            open_gesture_editor(&state_shortcut_clone, None);
            return glib::Propagation::Stop;
        }
        if is_ctrl && keyval == gtk::gdk::Key::f {
            state_shortcut_clone.borrow().search_entry.grab_focus();
            return glib::Propagation::Stop;
        }
        if is_ctrl && keyval == gtk::gdk::Key::comma {
            open_settings_window(&state_shortcut_clone);
            return glib::Propagation::Stop;
        }
        if is_ctrl && keyval == gtk::gdk::Key::q {
            window_shortcut_clone.destroy();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    state.borrow().window.add_controller(key_controller);

    // Stylesheet injection
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "headerbar { background: @window_bg_color; border: none; box-shadow: none; }\n\
         .main-window-content, .dialog-content { background-color: @window_bg_color; }\n\
         scrolledwindow, viewport { background-color: transparent !important; background-image: none !important; }\n\
         .boxed-list, .boxed-list row, .boxed-list listrow, row, listrow { background-color: @card_bg_color !important; }\n\
         .gesture-preview-frame { background: @window_bg_color; border-radius: 6px; }\n\
         searchentry, entry.search { border-radius: 18px; }\n\
         .context-title { font-size: 1.5em; font-weight: bold; }\n\
         .gesture-row { padding: 6px; }\n\
         .icon-holder { padding: 4px; }\n\
         .action-label { font-size: 0.9em; opacity: 0.7; }\n\
         .status-dot-running { color: #10b981; }\n\
         .status-dot-stopped { color: #6b7280; }\n\
         .settings-row { padding: 8px; }\n\
         .section-header { font-weight: bold; margin-top: 8px; margin-bottom: 4px; }\n\
         .status-label { font-weight: bold; }\n\
         .warning-box { padding: 8px; background-color: alpha(@warning_color, 0.15); border: 1px solid alpha(@warning_color, 0.3); border-radius: 6px; }\n\
         .warning-label { color: @warning_color; font-size: 0.95em; }\n\
         .notice-box { padding: 8px 8px 8px 12px; background-color: alpha(currentColor, 0.06); border-radius: 6px; }\n\
         .keycap { padding: 6px 12px; background-color: alpha(currentColor, 0.08); border: 1px solid alpha(currentColor, 0.15); border-radius: 6px; font-weight: bold; font-size: 1.1em; }\n\
         .drag-handle { opacity: 0.55; cursor: grab; margin-right: 4px; }\n\
         .drag-handle:hover { opacity: 1; }\n\
         .drag-handle:active { cursor: grabbing; }\n"
    );

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    state.borrow().window.present();
    state.borrow().search_entry.grab_focus();
}
