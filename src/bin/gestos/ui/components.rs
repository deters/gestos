use crate::actions::standard_shortcut_name;
use gtk::prelude::*;
use gtk::{cairo, gdk, glib};
use gtk4 as gtk;
use std::rc::Rc;
use std::cell::RefCell;
use gestos::config::{ActionType, Gesture};
use gestos::protractor::Point2D;
use crate::state::AppState;
use crate::ui::main_window::refresh_gesture_list;

/// Escape and Ctrl+W close a dialog, as in GNOME's own apps.
pub fn close_with_keys(window: &impl IsA<gtk::Window>) {
    add_close_keys(window, true);
}

/// Ctrl+W closes a libadwaita window, which handles Escape itself, as it also leaves its subpages.
pub fn close_with_ctrl_w(window: &impl IsA<gtk::Window>) {
    add_close_keys(window, false);
}

fn add_close_keys(window: &impl IsA<gtk::Window>, escape: bool) {
    let keys = gtk::EventControllerKey::new();
    window.as_ref().add_controller(keys.clone());
    let closing = window.clone().upcast::<gtk::Window>();
    keys.connect_key_pressed(move |_, keyval, _, modifiers| {
        let ctrl_w = modifiers.contains(gdk::ModifierType::CONTROL_MASK)
            && keyval.to_lower() == gdk::Key::w;
        if (escape && keyval == gdk::Key::Escape) || ctrl_w {
            closing.destroy();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
}

/// Shows an error's message, which can be copied, to paste in a bug report.
pub fn show_error_dialog<W: IsA<gtk::Window>>(parent: &W, message: &str) {
    show_error_dialog_titled(parent, None, message);
}

/// Shows an error, under a title that says what failed, if it isn't plain from the message.
pub fn show_error_dialog_titled<W: IsA<gtk::Window>>(parent: &W, title: Option<&str>, message: &str) {
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_destroy_with_parent(true);
    dialog.set_title(Some("Error"));
    dialog.set_default_size(350, 100);
    dialog.set_resizable(false);
    
    close_with_keys(&dialog);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.add_css_class("dialog-content");
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let content_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    let error_icon = gtk::Image::from_icon_name("dialog-error-symbolic");
    error_icon.set_pixel_size(32);
    error_icon.set_valign(gtk::Align::Start);
    content_box.append(&error_icon);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    text_box.set_hexpand(true);

    if let Some(title) = title {
        let title_label = gtk::Label::new(Some(title));
        title_label.add_css_class("heading");
        title_label.set_wrap(true);
        title_label.set_halign(gtk::Align::Start);
        text_box.append(&title_label);
    }

    let label = gtk::Label::new(Some(message));
    label.set_wrap(true);
    label.set_selectable(true);
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    text_box.append(&label);

    content_box.append(&text_box);
    vbox.append(&content_box);

    let button_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    button_box.set_halign(gtk::Align::End);
    button_box.set_margin_top(12);
    // So that it needn't be typed, to look it up or to report it.
    let copy_button = gtk::Button::with_mnemonic("_Copy Message");
    let copied = match title {
        Some(title) => format!("{title}\n{message}"),
        None => message.to_string(),
    };
    copy_button.connect_clicked(move |button| button.clipboard().set_text(&copied));
    button_box.append(&copy_button);
    let ok_button = gtk::Button::with_mnemonic("_OK");
    ok_button.set_width_request(80);
    ok_button.set_receives_default(true);

    let dialog_clone = dialog.clone();
    ok_button.connect_clicked(move |_| {
        dialog_clone.destroy();
    });
    button_box.append(&ok_button);
    vbox.append(&button_box);

    dialog.set_child(Some(&vbox));
    dialog.set_default_widget(Some(&ok_button));
    dialog.present();
}
/// The icon an action is shown with, and the color behind it. A template's is the one it names, if
/// the icon theme has it.
pub fn get_action_category_icon(action: &ActionType) -> (std::borrow::Cow<'static, str>, &'static str) {
    if let ActionType::Template(_) = action {
        let icon = action.get_icon_name();
        let in_theme = gtk::gdk::Display::default()
            .is_some_and(|display| gtk::IconTheme::for_display(&display).has_icon(&icon));
        if in_theme {
            return (icon, "icon-bg-blue");
        }
    }
    let (icon, background) = kind_icon(action);
    (icon.into(), background)
}

fn kind_icon(action: &ActionType) -> (&'static str, &'static str) {
    match action {
        ActionType::Execute(_) => ("utilities-terminal-symbolic", "icon-bg-blue"),
        ActionType::Keyboard(_) => ("input-keyboard-symbolic", "icon-bg-blue"),
        // A custom shortcut shows a keyboard. A standard one has an icon of its own.
        ActionType::Keypress(keys) if keys.is_empty() || standard_shortcut_name(keys).is_none() => {
            ("input-keyboard-symbolic", "icon-bg-blue")
        }
        ActionType::Keypress(_) => (
            "input-keyboard-symbolic",
            "icon-bg-orange",
        ),
        ActionType::Gnome(_) => {
            let icon = action.own_icon_name();
            let is_media = icon.starts_with("audio-")
                || icon.starts_with("media-")
                || icon.starts_with("microphone-");
            (icon, if is_media { "icon-bg-green" } else { "icon-bg-blue" })
        }
        ActionType::Kde(_, _) => ("preferences-system-symbolic", "icon-bg-blue"),
        ActionType::Iconify => ("window-minimize-symbolic", "icon-bg-blue"),
        ActionType::Kill => ("window-close-symbolic", "icon-bg-blue"),
        ActionType::Maximize | ActionType::ToggleMaximized => ("window-maximize-symbolic", "icon-bg-blue"),
        ActionType::Restore => ("window-restore-symbolic", "icon-bg-blue"),
        ActionType::ToggleFullscreen => ("view-fullscreen-symbolic", "icon-bg-blue"),
        ActionType::ToggleTileWindows => ("view-grid-symbolic", "icon-purple"),
        ActionType::ToggleMiddleButtonWindows => ("input-mouse-symbolic", "icon-purple"),
        ActionType::ToggleMiddleClickPaste => ("edit-paste-symbolic", "icon-purple"),
        ActionType::Lower | ActionType::Raise => ("window-new-symbolic", "icon-bg-blue"),
        ActionType::WorkspaceLeft => ("go-previous-symbolic", "icon-bg-blue"),
        ActionType::WorkspaceRight => ("go-next-symbolic", "icon-bg-blue"),
        ActionType::WorkspaceUp => ("go-up-symbolic", "icon-bg-blue"),
        ActionType::WorkspaceDown => ("go-down-symbolic", "icon-bg-blue"),
        ActionType::VolumeUp | ActionType::VolumeDown | ActionType::VolumeMute => {
            ("audio-volume-high-symbolic", "icon-bg-green")
        }
        ActionType::MediaPlay | ActionType::MediaNext | ActionType::MediaPrev => {
            ("media-playback-start-symbolic", "icon-bg-green")
        }
        _ => (action.own_icon_name(), "icon-bg-blue"),
    }
}
pub fn draw_gesture_path(
    cr: &cairo::Context,
    points: &[Point2D],
    width: f64,
    height: f64,
    _draw_bg: bool,
    fit_to_canvas: bool,
) {
    // Background is transparent to naturally display the theme-dependent container background (e.g. @view_bg_color)

    if points.len() < 2 {
        return;
    }

    // Determine bounding box
    let mut min_x = points[0].x;
    let mut max_x = points[0].x;
    let mut min_y = points[0].y;
    let mut max_y = points[0].y;

    for p in points {
        if p.x < min_x {
            min_x = p.x;
        }
        if p.x > max_x {
            max_x = p.x;
        }
        if p.y < min_y {
            min_y = p.y;
        }
        if p.y > max_y {
            max_y = p.y;
        }
    }

    let w = max_x - min_x;
    let h = max_y - min_y;
    let max_dim = w.max(h);
    let scale = if fit_to_canvas {
        let size = width.min(height);
        if max_dim > 1.0 {
            (size * 0.62) / max_dim
        } else {
            1.0
        }
    } else {
        1.0
    };

    let cx = min_x + w / 2.0;
    let cy = min_y + h / 2.0;

    cr.save().unwrap();
    if fit_to_canvas {
        cr.translate(width / 2.0, height / 2.0);
        cr.scale(scale, scale);
        cr.translate(-cx, -cy);
    }

    cr.set_line_width(4.0 / scale);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);

    // Draw stroke gradient (violet to pink)
    let pat = cairo::LinearGradient::new(min_x, min_y, max_x, max_y);
    pat.add_color_stop_rgba(0.0, 0.49, 0.27, 0.90, 0.9);
    pat.add_color_stop_rgba(1.0, 0.98, 0.72, 0.80, 0.9);
    cr.set_source(&pat).unwrap();

    // In curves, as it was drawn, with its sharp corners kept.
    cr.move_to(points[0].x, points[0].y);
    for curve in gestos::smooth::smooth(points) {
        let (c1, c2, to) = (curve.control1, curve.control2, curve.to);
        cr.curve_to(c1.x, c1.y, c2.x, c2.y, to.x, to.y);
    }
    cr.stroke().unwrap();

    // Draw end indicator (arrowhead pointing in the direction of the last segment)
    if points.len() >= 2 {
        let end = points.last().unwrap();
        let prev = &points[points.len() - 2];

        let dy = end.y - prev.y;
        let dx = end.x - prev.x;

        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.001 {
            let angle = dy.atan2(dx);

            // Arrowhead size relative to scale
            let arrow_length = 12.0 / scale;

            // Calculate base corners perpendicular to the end of the line
            let half_width = 5.0 / scale;
            let perp_angle = angle + std::f64::consts::FRAC_PI_2;
            let x1 = end.x + half_width * perp_angle.cos();
            let y1 = end.y + half_width * perp_angle.sin();
            let x2 = end.x - half_width * perp_angle.cos();
            let y2 = end.y - half_width * perp_angle.sin();

            // Calculate the tip pointing forward from the end of the line
            let tip_x = end.x + arrow_length * angle.cos();
            let tip_y = end.y + arrow_length * angle.sin();

            cr.set_source_rgba(0.49, 0.27, 0.90, 1.0);
            cr.move_to(tip_x, tip_y);
            cr.line_to(x1, y1);
            cr.line_to(x2, y2);
            cr.close_path();
            cr.fill().unwrap();
        } else {
            // Fallback: draw a simple circle if the last segment has zero length
            cr.set_source_rgba(0.49, 0.27, 0.90, 1.0);
            cr.arc(end.x, end.y, 5.0 / scale, 0.0, 2.0 * std::f64::consts::PI);
            cr.fill().unwrap();
        }
    }

    cr.restore().unwrap();
}
pub fn get_action_human_readable(action: &ActionType) -> String {
    match action {
        ActionType::Iconify => "Minimize Window".to_string(),
        ActionType::Kill => "Close Window".to_string(),
        ActionType::Lower => "Lower Window".to_string(),
        ActionType::Raise => "Raise Window".to_string(),
        ActionType::Maximize => "Maximize Window".to_string(),
        ActionType::Restore => "Restore Window".to_string(),
        ActionType::ToggleMaximized => "Toggle Maximized".to_string(),
        ActionType::Keypress(keys) => {
            if keys.is_empty() {
                "Custom Keyboard Shortcut".to_string()
            } else if let Some(name) = standard_shortcut_name(keys) {
                name.to_string()
            } else {
                format!("Custom Keyboard Shortcut ({})", keys)
            }
        }
        ActionType::Keyboard(steps) => {
            if steps.is_empty() {
                "Advanced Action".to_string()
            } else {
                format!(
                    "Advanced Action ({})",
                    gestos::key_steps::describe_steps(steps)
                )
            }
        }
        ActionType::Template(id) => crate::action_templates::find(id).map_or_else(|| id.clone(), |t| t.name),
        ActionType::Execute(cmd) => {
            if cmd.is_empty() {
                "Run Command in Shell".to_string()
            } else {
                format!("Run Command: {}", gestos::config::short_command(cmd))
            }
        }
        ActionType::WorkspaceLeft => "Workspace Left".to_string(),
        ActionType::WorkspaceRight => "Workspace Right".to_string(),
        ActionType::WorkspaceUp => "Workspace Up".to_string(),
        ActionType::WorkspaceDown => "Workspace Down".to_string(),
        ActionType::ShowOverview => "Show Overview".to_string(),
        ActionType::ShowAppGrid => "Show App Grid".to_string(),
        ActionType::Click(btn) => {
            if let Some(b) = btn {
                format!("Click Mouse Button {}", b)
            } else {
                "Click Mouse Button 1".to_string()
            }
        }
        ActionType::ToggleFullscreen => "Toggle Fullscreen".to_string(),
        ActionType::ToggleTileWindows => "Toggle Tile Windows".to_string(),
        ActionType::ShowDesktop => "Show Desktop".to_string(),
        ActionType::LockScreen => "Lock Screen".to_string(),
        ActionType::Terminal => "Open Terminal".to_string(),
        ActionType::VolumeUp => "Volume Up".to_string(),
        ActionType::VolumeDown => "Volume Down".to_string(),
        ActionType::VolumeMute => "Volume Mute".to_string(),
        ActionType::MediaPlay => "Play/Pause Media".to_string(),
        ActionType::MediaNext => "Next Track".to_string(),
        ActionType::MediaPrev => "Previous Track".to_string(),
        ActionType::Www => "Web Browser".to_string(),
        ActionType::Home => "Home Folder".to_string(),
        ActionType::Email => "Email Client".to_string(),
        ActionType::Search => "System Search".to_string(),
        ActionType::Calculator => "Calculator".to_string(),
        ActionType::ControlCenter => "Control Center".to_string(),
        ActionType::Logout => "Log Out".to_string(),
        ActionType::Screenshot => "Take Screenshot".to_string(),
        ActionType::ScreenshotWindow => "Screenshot Window".to_string(),
        ActionType::ScreenshotArea => "Screenshot Area".to_string(),
        ActionType::Gnome(key) => {
            let last_part = key.split('.').last().unwrap_or(key);
            let cleaned = last_part.replace('-', " ");
            let mut chars = cleaned.chars();
            match chars.next() {
                None => "GNOME Action".to_string(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str()
            }
        }
        ActionType::Kde(comp, short) => {
            format!("{} - {}", comp, short)
        }
        ActionType::Abort => "Abort Gesture".to_string(),
        ActionType::Lookup => "Look Up Selection".to_string(),
        ActionType::ToggleMiddleButtonWindows => "Toggle Middle Button Window Control".to_string(),
        ActionType::ToggleMiddleClickPaste => "Toggle Middle Click Paste".to_string(),
    }
}
pub fn create_gesture_row(gesture: &Gesture, state_rc: &Rc<RefCell<AppState>>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("gesture-row");

    let main_hbox = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    main_hbox.set_margin_start(16);
    main_hbox.set_margin_end(16);
    main_hbox.set_margin_top(12);
    main_hbox.set_margin_bottom(12);

    let drag_handle = gtk::Image::from_icon_name("list-drag-handle-symbolic");
    drag_handle.add_css_class("drag-handle");
    // Set here, as GTK's CSS has no cursor property. While dragging, the drag's own one shows.
    drag_handle.set_cursor_from_name(Some("grab"));
    drag_handle.set_valign(gtk::Align::Center);
    main_hbox.append(&drag_handle);

    // A custom action or shortcut named by the user shows that name, even one with a standard
    // shortcut's keys. Others are named after their action, as "Advanced Action", followed by its
    // keys or steps, as the standard shortcut they are, or as "Custom Keyboard Action" from before.
    let is_keyboard_action = matches!(
        gesture.actions.first(),
        Some(ActionType::Keyboard(_)) | Some(ActionType::Keypress(_))
    );
    let standard_name = match gesture.actions.first() {
        Some(ActionType::Keypress(keys)) => standard_shortcut_name(keys),
        _ => None,
    };
    let named = is_keyboard_action
        && standard_name.as_deref() != Some(gesture.name.as_str())
        && !gesture.name.starts_with("Advanced Action")
        && !gesture.name.starts_with("Custom Action")
        && !gesture.name.starts_with("Keyboard Shortcut")
        && !gesture.name.starts_with("Custom Keyboard Shortcut")
        && !gesture.name.starts_with("Custom Keyboard Action");

    let has_keypress = if !gesture.actions.is_empty() && !named {
        // A standard shortcut shows its name, like the other actions. A custom one shows its keys,
        // even ones a standard shortcut has.
        if let ActionType::Keypress(ref keys) = gesture.actions[0] {
            (standard_name.as_deref() != Some(gesture.name.as_str())).then_some(keys)
        } else {
            None
        }
    } else {
        None
    };

    // A shortcut for only one app is that app's, and shows its icon, even beside its keys.
    let only_app = match (gesture.actions.first(), gesture.apps.as_slice()) {
        (Some(ActionType::Keypress(_)), [app]) => Some(app.as_str()),
        _ => None,
    };

    // 1. Icon Category Holder (only if not a keypress action)
    if has_keypress.is_none() || only_app.is_some() {
        let (icon_name, bg_class) = if let Some(ActionType::Keypress(_)) = gesture.actions.first() {
            // Named by the user, it's a custom shortcut, whatever its keys.
            if named {
                ("input-keyboard-symbolic".into(), "icon-bg-blue")
            } else {
                get_action_category_icon(&gesture.actions[0])
            }
        } else if let Some(ActionType::Keyboard(steps)) = gesture.actions.first() {
            // An advanced action shows the icon of its first step, as what it starts with.
            let (icon_name, bg_class) = get_action_category_icon(&gesture.actions[0]);
            let icon_name = steps.first().map_or(icon_name, |step| crate::ui::keyboard_steps::step_icon(step).into());
            (icon_name, bg_class)
        } else if !gesture.actions.is_empty() {
            get_action_category_icon(&gesture.actions[0])
        } else {
            ("system-run-symbolic".into(), "icon-bg-blue")
        };

        let icon_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        icon_box.add_css_class("icon-holder");
        icon_box.add_css_class(bg_class);
        icon_box.set_valign(gtk::Align::Center);
        // One that opens an app first shows that app's own icon.
        let app = gesture.actions.first().and_then(crate::ui::app_picker::first_app).or(only_app);
        let icon = match app {
            Some(app) => crate::ui::app_picker::app_icon(app, 32),
            None => {
                let icon = gtk::Image::from_icon_name(&icon_name);
                icon.set_icon_size(gtk::IconSize::Large);
                icon
            }
        };
        icon_box.append(&icon);
        main_hbox.append(&icon_box);
    }

    // 2. Info text label
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.set_hexpand(true);
    vbox.set_valign(gtk::Align::Center);

    if let Some(keys) = has_keypress {
        let row_hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row_hbox.set_halign(gtk::Align::Start);
        row_hbox.set_valign(gtk::Align::Center);

        if !keys.is_empty() {
            let parts: Vec<&str> = keys.split('+').filter(|s| !s.trim().is_empty()).collect();
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    let plus = gtk::Label::new(Some("+"));
                    plus.set_opacity(0.6);
                    plus.set_valign(gtk::Align::Center);
                    row_hbox.append(&plus);
                }
                // As shortcuts' names write it.
                let part = if *part == "Control" { "Ctrl" } else { part };
                let keycap_label = gtk::Label::new(Some(part));
                keycap_label.add_css_class("keycap");
                keycap_label.set_valign(gtk::Align::Center);
                row_hbox.append(&keycap_label);
            }
        } else {
            let none_label = gtk::Label::new(Some("None"));
            none_label.set_opacity(0.5);
            none_label.set_valign(gtk::Align::Center);
            row_hbox.append(&none_label);
        }
        vbox.append(&row_hbox);
    } else {
        let action_desc = if named {
            gesture.name.clone()
        } else if let Some(ActionType::Gnome(_) | ActionType::Kde(_, _)) =
            gesture.actions.first()
        {
            // The name picked when creating it, since the action itself is only an internal key.
            gesture.name.clone()
        } else if !gesture.actions.is_empty() {
            get_action_human_readable(&gesture.actions[0])
        } else {
            "No Action Configured".to_string()
        };
        let title_label = gtk::Label::new(Some(&action_desc));
        title_label.set_halign(gtk::Align::Start);
        title_label.set_markup(&format!("<b>{}</b>", gtk::glib::markup_escape_text(&action_desc)));
        vbox.append(&title_label);
    }

    main_hbox.append(&vbox);

    // A gesture says so when an action of it can't run here: one of another desktop's, or one
    // that needs the GNOME extension while the extension can't run it.
    let note = match gesture.needs_other_desktop() {
        Some("kde") => Some("Only works on KDE"),
        Some(_) => Some("Only works on GNOME"),
        None => gesture.actions.first().and_then(|action| {
            let status = &state_rc.borrow().extension_status;
            (!status.is_running())
                .then(|| crate::ui::main_window::extension_note(action, status))
                .flatten()
        }),
    };
    if let Some(note) = note {
        let warning = gtk::Image::from_icon_name("dialog-warning-symbolic");
        warning.add_css_class("warning");
        warning.set_valign(gtk::Align::Center);
        warning.set_tooltip_text(Some(note));
        warning.update_property(&[gtk::accessible::Property::Label(note)]);
        main_hbox.append(&warning);
    }

    // 3. Mini Cairo preview canvas
    let preview_frame = gtk::Frame::new(None);
    preview_frame.add_css_class("gesture-preview-frame");
    preview_frame.set_size_request(120, 60);
    preview_frame.set_valign(gtk::Align::Center);

    let preview_canvas = gtk::DrawingArea::new();
    preview_canvas.set_vexpand(true);
    let pts_clone = gesture.points.clone();
    preview_canvas.set_draw_func(move |_, cr, width, height| {
        draw_gesture_path(cr, &pts_clone, width as f64, height as f64, true, true);
    });
    // The keys to hold and the apps it's for show on the drawing, small, in its corner, as they're
    // part of what triggers the gesture.
    let preview = gtk::Overlay::new();
    preview.set_child(Some(&preview_canvas));
    let keys = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    keys.set_halign(gtk::Align::Start);
    keys.set_valign(gtk::Align::Start);
    keys.set_margin_start(4);
    keys.set_margin_top(4);
    for label in gesture.modifiers.shown_labels(&gestos::keyboards::modifier_names()) {
        let keycap = gtk::Label::new(Some(label));
        keycap.add_css_class("keycap-small");
        keys.append(&keycap);
    }
    if !gesture.apps.is_empty() {
        keys.append(&crate::ui::app_picker::app_icons(&gesture.apps));
    }
    preview.add_overlay(&keys);
    // The path is drawn below them, so they don't hide it.
    if !gesture.apps.is_empty() {
        preview_canvas.set_margin_top(20);
    } else if !gesture.modifiers.is_empty() {
        preview_canvas.set_margin_top(16);
    }
    preview_frame.set_child(Some(&preview));
    main_hbox.append(&preview_frame);
    row.set_child(Some(&main_hbox));

    // 4. Drag and Drop Controllers for reordering
    let drag_source = gtk::DragSource::new();
    drag_source.set_actions(gdk::DragAction::MOVE);
    let id_str = gesture.id.clone();
    drag_source.connect_prepare(move |_, _, _| {
        let value = id_str.to_value();
        let provider = gdk::ContentProvider::for_value(&value);
        Some(provider)
    });
    let row_clone = row.clone();
    drag_source.connect_drag_begin(move |source, _| {
        let paintable = gtk::WidgetPaintable::new(Some(&row_clone));
        source.set_icon(Some(&paintable), 0, 0);
    });
    drag_handle.add_controller(drag_source);

    let drop_target = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
    let state_clone = Rc::clone(state_rc);
    let target_id = gesture.id.clone();
    drop_target.connect_drop(move |_, value, _, _| {
        if let Ok(dragged_id) = value.get::<String>() {
            if dragged_id != target_id {
                let mut state = state_clone.borrow_mut();
                let source_idx = state.config.gestures.iter().position(|g| g.id == dragged_id);
                let dest_idx = state.config.gestures.iter().position(|g| g.id == target_id);
                if let (Some(s_idx), Some(d_idx)) = (source_idx, dest_idx) {
                    let item = state.config.gestures.remove(s_idx);
                    let name_clone = item.name.clone();
                    state.config.gestures.insert(d_idx, item);
                    if let Err(e) = state.config.save_to_file() {
                        println!("Failed to save config on reorder: {}", e);
                    }
                    let state_clone_inner = Rc::clone(&state_clone);
                    glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
                        refresh_gesture_list(&state_clone_inner, Some(&name_clone));
                    });
                }
            }
        }
        true
    });
    row.add_controller(drop_target);

    row
}
pub fn show_confirm_dialog<W: IsA<gtk::Window>, F: FnOnce() + 'static>(
    parent: &W,
    title: &str,
    message: &str,
    confirm_label: &str,
    is_destructive: bool,
    on_confirm: F,
) {
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_destroy_with_parent(true);
    dialog.set_title(Some(title));
    dialog.set_default_size(350, 120);
    dialog.set_resizable(false);
    
    close_with_keys(&dialog);

    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 18);
    main_box.add_css_class("dialog-content");
    main_box.set_margin_start(24);
    main_box.set_margin_end(24);
    main_box.set_margin_top(24);
    main_box.set_margin_bottom(24);
    dialog.set_child(Some(&main_box));

    let label = gtk::Label::new(Some(message));
    label.set_wrap(true);
    label.set_halign(gtk::Align::Center);
    main_box.append(&label);

    let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    btn_box.set_halign(gtk::Align::End);

    let cancel_btn = gtk::Button::with_mnemonic("_Cancel");
    let dialog_cancel_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_cancel_clone.destroy();
    });
    btn_box.append(&cancel_btn);

    // If a custom label isn't using a mnemonic, we could try to add one, but let's just use it as is for now.
    // Assuming confirm_label might have a mnemonic, we use with_mnemonic.
    let confirm_btn = gtk::Button::with_mnemonic(confirm_label);
    if is_destructive {
        confirm_btn.add_css_class("destructive-action");
    } else {
        confirm_btn.add_css_class("suggested-action");
    }
    let dialog_confirm_clone = dialog.clone();
    let on_confirm_cell = Rc::new(RefCell::new(Some(on_confirm)));
    confirm_btn.connect_clicked(move |_| {
        if let Some(on_confirm) = on_confirm_cell.borrow_mut().take() {
            on_confirm();
        }
        dialog_confirm_clone.destroy();
    });
    btn_box.append(&confirm_btn);

    main_box.append(&btn_box);
    // Enter must never destroy anything, so a destructive confirmation takes a deliberate click.
    let default_btn = if is_destructive { &cancel_btn } else { &confirm_btn };
    default_btn.set_receives_default(true);
    dialog.set_default_widget(Some(default_btn));
    dialog.present();
}
