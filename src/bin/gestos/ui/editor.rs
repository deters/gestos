use gtk::prelude::*;
use gtk::{gio, glib};
use gestos::protractor::match_gesture;
use gtk4 as gtk;
use std::rc::Rc;
use std::cell::RefCell;
use gestos::config::{ActionType, Gesture};
use gestos::modifiers::Modifiers;
use gestos::config::generate_unique_id;
use crate::state::{AppState, EditorActionOption, CATEGORY_NAMES};
use crate::suggestions::search_matches;
use crate::actions::{action_needs_parameter, fetch_gnome_action_options, fetch_kde_action_options, get_static_action_options, action_matches};
use crate::ui::shortcut_recorder::open_shortcut_recorder;
use crate::ui::components::{draw_gesture_path, get_action_category_icon, show_confirm_dialog};
use crate::daemon::reload_daemon;
use crate::ui::main_window::refresh_gesture_list;
use gestos::protractor::Point2D;

pub fn get_default_gesture_name(opt: &EditorActionOption, detail: &str) -> String {
    match &opt.action_type {
        ActionType::Keypress(_) => {
            if detail.trim().is_empty() {
                opt.name.clone()
            } else {
                format!("{} ({})", opt.name, detail.trim())
            }
        }
        ActionType::Execute(_) if opt.category == 7 => {
            if detail.trim().is_empty() {
                opt.name.clone()
            } else {
                format!("{} ({})", opt.name, detail.trim())
            }
        }
        ActionType::Click(_) => {
            if detail.trim().is_empty() {
                opt.name.clone()
            } else {
                format!("{} ({})", opt.name, detail.trim())
            }
        }
        _ => opt.name.clone(),
    }
}
/// The modifier keys a gesture needs. Holding them while drawing sets them too.
#[derive(Clone)]
struct ModifierToggles {
    ctrl: gtk::ToggleButton,
    alt: gtk::ToggleButton,
    shift: gtk::ToggleButton,
    super_key: gtk::ToggleButton,
}

impl ModifierToggles {
    fn new(initial: Modifiers) -> Self {
        let toggles = ModifierToggles {
            ctrl: gtk::ToggleButton::with_label("Ctrl"),
            alt: gtk::ToggleButton::with_label("Alt"),
            shift: gtk::ToggleButton::with_label("Shift"),
            super_key: gtk::ToggleButton::with_label("Super"),
        };
        toggles.set(initial);
        toggles
    }

    fn set(&self, modifiers: Modifiers) {
        self.ctrl.set_active(modifiers.ctrl);
        self.alt.set_active(modifiers.alt);
        self.shift.set_active(modifiers.shift);
        self.super_key.set_active(modifiers.super_key);
    }

    fn get(&self) -> Modifiers {
        Modifiers {
            ctrl: self.ctrl.is_active(),
            alt: self.alt.is_active(),
            shift: self.shift.is_active(),
            super_key: self.super_key.is_active(),
        }
    }

    fn buttons(&self) -> [&gtk::ToggleButton; 4] {
        [&self.ctrl, &self.alt, &self.shift, &self.super_key]
    }
}

fn held_modifiers(state: gtk::gdk::ModifierType) -> Modifiers {
    Modifiers {
        ctrl: state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
        alt: state.contains(gtk::gdk::ModifierType::ALT_MASK),
        shift: state.contains(gtk::gdk::ModifierType::SHIFT_MASK),
        super_key: state.contains(gtk::gdk::ModifierType::SUPER_MASK),
    }
}

/// Shows the parameter row only for an action that takes one, in the form that action uses.
fn show_action_parameter(
    opt: &EditorActionOption,
    row: &gtk::Box,
    label: &gtk::Label,
    entry: &gtk::Entry,
    shortcut_display: &gtk::Box,
    record_button: &gtk::Button,
) {
    // The list's own row, because hiding only its content leaves an empty strip.
    if let Some(list_row) = row.parent() {
        list_row.set_visible(action_needs_parameter(opt));
    }

    let is_keypress = matches!(&opt.action_type, ActionType::Keypress(_));
    entry.set_visible(!is_keypress);
    shortcut_display.set_visible(is_keypress);
    record_button.set_visible(is_keypress);

    match &opt.action_type {
        ActionType::Keypress(_) => label.set_text("Keys to Send"),
        ActionType::Execute(_) => {
            label.set_text("Command to Execute");
            entry.set_placeholder_text(Some("e.g. firefox"));
        }
        ActionType::Click(_) => {
            label.set_text("Mouse Button");
            entry.set_placeholder_text(Some("e.g. 1 (Left), 2 (Middle), 3 (Right)"));
        }
        _ => {}
    }
}

pub fn open_gesture_editor(state_rc: &Rc<RefCell<AppState>>, target_gesture: Option<Gesture>) {
    open_editor(state_rc, target_gesture, None);
}

/// Opens the editor for a new gesture with its action already chosen.
pub fn open_gesture_editor_for_action(state_rc: &Rc<RefCell<AppState>>, action: EditorActionOption) {
    open_editor(state_rc, None, Some(action));
}

fn open_editor(
    state_rc: &Rc<RefCell<AppState>>,
    target_gesture: Option<Gesture>,
    preset_action: Option<EditorActionOption>,
) {
    let state = state_rc.borrow();
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(&state.window));
    dialog.set_modal(true);
    let title_text = if target_gesture.is_some() { "Edit Gesture" } else { "Add Gesture" };
    dialog.set_title(Some(title_text));
    dialog.set_default_size(700, 560);

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

    let dialog_header = gtk::HeaderBar::new();
    dialog_header.set_show_title_buttons(false);
    let dialog_title = gtk::Label::new(Some(title_text));
    dialog_title.add_css_class("title");
    dialog_header.set_title_widget(Some(&dialog_title));
    dialog.set_titlebar(Some(&dialog_header));

    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    main_box.add_css_class("dialog-content");
    main_box.set_margin_start(20);
    main_box.set_margin_end(20);
    main_box.set_margin_top(16);
    main_box.set_margin_bottom(16);
    dialog.set_child(Some(&main_box));

    // Scrollable area for all inputs
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_vscrollbar_policy(gtk::PolicyType::Automatic);
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);
    main_box.append(&scrolled);

    let scroll_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    scrolled.set_child(Some(&scroll_content));

    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some("e.g. Back Action"));
    if let Some(ref g) = target_gesture {
        name_entry.set_text(&g.name);
    }

    let is_name_customized = Rc::new(RefCell::new(target_gesture.is_some()));
    let is_updating_programmatically = Rc::new(RefCell::new(false));

    let cust_clone = Rc::clone(&is_name_customized);
    let prog_clone = Rc::clone(&is_updating_programmatically);
    name_entry.connect_changed(move |entry| {
        if *prog_clone.borrow() {
            return;
        }
        *cust_clone.borrow_mut() = !entry.text().trim().is_empty();
    });

    // --- GESTURE PATH SECTION ---
    let path_section_label = gtk::Label::new(Some("Gesture Path"));
    path_section_label.set_halign(gtk::Align::Start);
    path_section_label.add_css_class("section-header");
    scroll_content.append(&path_section_label);

    let canvas_frame = gtk::Frame::new(None);
    canvas_frame.add_css_class("gesture-preview-frame");
    canvas_frame.set_size_request(300, 150);
    canvas_frame.set_vexpand(true);

    let canvas = gtk::DrawingArea::new();
    let recorded_points: Rc<RefCell<Vec<Point2D>>> = Rc::new(RefCell::new(
        target_gesture
            .as_ref()
            .map(|g| g.points.clone())
            .unwrap_or_default(),
    ));
    let is_recording = Rc::new(RefCell::new(false));

    let pts_clone = Rc::clone(&recorded_points);
    let rec_clone = Rc::clone(&is_recording);
    canvas.set_draw_func(move |_, cr, width, height| {
        let pts = pts_clone.borrow();
        let active = *rec_clone.borrow();
        draw_gesture_path(cr, &pts, width as f64, height as f64, true, !active);
    });

    // GestureDrag controller for recording drawing
    let drag = gtk::GestureDrag::new();

    let conflict_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    conflict_box.add_css_class("warning-box");
    conflict_box.set_visible(false);
    conflict_box.set_margin_top(4);
    conflict_box.set_margin_bottom(4);

    let conflict_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
    conflict_icon.set_icon_size(gtk::IconSize::Normal);
    conflict_box.append(&conflict_icon);

    let conflict_label = gtk::Label::new(None);
    conflict_label.set_halign(gtk::Align::Start);
    conflict_label.add_css_class("warning-label");
    conflict_label.set_wrap(true);
    conflict_box.append(&conflict_label);

    let modifier_toggles = ModifierToggles::new(
        target_gesture
            .as_ref()
            .map(|gesture| gesture.modifiers)
            .unwrap_or_default(),
    );

    let canvas_clone = canvas.clone();
    let pts_drag = Rc::clone(&recorded_points);
    let rec_drag = Rc::clone(&is_recording);
    let conflict_box_begin = conflict_box.clone();
    let toggles_begin = modifier_toggles.clone();
    drag.connect_drag_begin(move |gesture, start_x, start_y| {
        *rec_drag.borrow_mut() = true;
        // Nothing held keeps what the toggles say, so they can be set before drawing.
        let held = held_modifiers(gesture.current_event_state());
        if !held.is_empty() {
            toggles_begin.set(held);
        }
        let mut pts = pts_drag.borrow_mut();
        pts.clear();
        pts.push(Point2D {
            x: start_x,
            y: start_y,
        });
        println!(
            "GUI: Gesture drawing started at ({:.1}, {:.1})",
            start_x, start_y
        );
        canvas_clone.queue_draw();
        conflict_box_begin.set_visible(false);
    });

    let canvas_clone2 = canvas.clone();
    let pts_drag2 = Rc::clone(&recorded_points);
    let rec_drag2 = Rc::clone(&is_recording);
    drag.connect_drag_update(move |gesture, offset_x, offset_y| {
        if !*rec_drag2.borrow() {
            return;
        }
        if let Some((start_x, start_y)) = gesture.start_point() {
            let cx = start_x + offset_x;
            let cy = start_y + offset_y;
            let mut pts = pts_drag2.borrow_mut();

            // Filter jitter
            let add = match pts.last() {
                Some(lp) => {
                    let dx = cx - lp.x;
                    let dy = cy - lp.y;
                    dx * dx + dy * dy >= 9.0
                }
                None => true,
            };
            if add {
                pts.push(Point2D { x: cx, y: cy });
                canvas_clone2.queue_draw();
            }
        }
    });

    let canvas_clone3 = canvas.clone();
    let rec_drag3 = Rc::clone(&is_recording);
    let pts_drag_end = Rc::clone(&recorded_points);
    let state_clone_drag = Rc::clone(state_rc);
    let edited_gesture_name = target_gesture.as_ref().map(|g| g.name.clone());
    let conflict_box_end = conflict_box.clone();
    let conflict_label_end = conflict_label.clone();
    let toggles_end = modifier_toggles.clone();
    drag.connect_drag_end(move |_, _, _| {
        *rec_drag3.borrow_mut() = false;
        canvas_clone3.queue_draw();
        println!("GUI: Gesture drawing finished.");

        let pts = pts_drag_end.borrow();
        if pts.len() >= 2 {
            let state = state_clone_drag.borrow();
            let drawn_modifiers = toggles_end.get();
            let templates: Vec<(String, Vec<Point2D>)> = state
                .config
                .gestures
                .iter()
                .filter(|g| !g.is_deleted)
                .filter(|g| g.modifiers == drawn_modifiers)
                .filter(|g| {
                    if let Some(ref name) = edited_gesture_name {
                        g.name != *name
                    } else {
                        true
                    }
                })
                .map(|g| (g.name.clone(), g.points.clone()))
                .collect();

            if let Some(matched_name) = match_gesture(&pts[..], &templates) {
                conflict_label_end.set_text(&format!(
                    "Warning: This path is very similar to the existing gesture '{}'.",
                    matched_name
                ));
                conflict_box_end.set_visible(true);
            } else {
                conflict_box_end.set_visible(false);
            }
        } else {
            conflict_box_end.set_visible(false);
        }
    });

    canvas.add_controller(drag);
    canvas_frame.set_child(Some(&canvas));
    scroll_content.append(&canvas_frame);
    scroll_content.append(&conflict_box);

    let modifiers_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    modifiers_row.add_css_class("settings-row");
    let modifiers_label = gtk::Label::new(Some("Modifier Keys"));
    modifiers_label.add_css_class("status-label");
    modifiers_label.set_halign(gtk::Align::Start);
    modifiers_label.set_hexpand(true);
    modifiers_row.append(&modifiers_label);
    for button in modifier_toggles.buttons() {
        button.set_tooltip_text(Some("Hold this key while drawing the gesture to use it"));
        modifiers_row.append(button);
    }
    let modifiers_list = gtk::ListBox::new();
    modifiers_list.add_css_class("boxed-list");
    modifiers_list.set_selection_mode(gtk::SelectionMode::None);
    modifiers_list.append(&modifiers_row);
    scroll_content.append(&modifiers_list);

    // --- ACTION SETTINGS SECTION ---
    let settings_section_label = gtk::Label::new(Some("Action Settings"));
    settings_section_label.set_halign(gtk::Align::Start);
    settings_section_label.add_css_class("section-header");
    scroll_content.append(&settings_section_label);

    let settings_list = gtk::ListBox::new();
    settings_list.add_css_class("boxed-list");
    settings_list.set_selection_mode(gtk::SelectionMode::None);
    scroll_content.append(&settings_list);



    // Row 2: Action Selection
    let action_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    action_row.add_css_class("settings-row");

    let action_label = gtk::Label::new(Some("Action"));
    action_label.set_halign(gtk::Align::Start);
    action_label.add_css_class("status-label");
    action_row.append(&action_label);

    let action_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    action_spacer.set_hexpand(true);
    action_row.append(&action_spacer);

    let action_select_btn = gtk::Button::new();
    action_select_btn.set_halign(gtk::Align::End);
    action_select_btn.set_size_request(240, -1);
    
    let btn_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let action_select_icon = gtk::Image::new();
    let action_select_label = gtk::Label::new(Some("Select Action…"));
    action_select_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    action_select_label.set_hexpand(true);
    action_select_label.set_halign(gtk::Align::Start);
    
    let arrow_icon = gtk::Image::from_icon_name("go-next-symbolic");
    
    btn_content.append(&action_select_icon);
    btn_content.append(&action_select_label);
    btn_content.append(&arrow_icon);
    
    action_select_btn.set_child(Some(&btn_content));
    action_row.append(&action_select_btn);

    settings_list.append(&action_row);

    // Row 4: Action Details Parameter
    let action_details_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    action_details_row.add_css_class("settings-row");

    let action_details_label = gtk::Label::new(None);
    action_details_label.set_halign(gtk::Align::Start);
    action_details_label.add_css_class("status-label");
    action_details_row.append(&action_details_label);

    let action_details_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    action_details_spacer.set_hexpand(true);
    action_details_row.append(&action_details_spacer);

    let entry_container = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    entry_container.set_halign(gtk::Align::End);

    let action_details_entry = gtk::Entry::new();
    action_details_entry.set_size_request(160, -1);
    entry_container.append(&action_details_entry);

    // Add shortcut display box for keycaps representation
    let shortcut_display_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    shortcut_display_box.set_halign(gtk::Align::End);
    shortcut_display_box.set_valign(gtk::Align::Center);
    shortcut_display_box.set_visible(false);
    entry_container.append(&shortcut_display_box);

    let record_btn = gtk::Button::from_icon_name("media-record-symbolic");
    record_btn.set_tooltip_text(Some("Record Keybinding"));
    entry_container.append(&record_btn);

    action_details_row.append(&entry_container);
    settings_list.append(&action_details_row);

    let selected_action: Rc<RefCell<Option<EditorActionOption>>> = Rc::new(RefCell::new(None));

    let action_picker = gtk::Window::new();
    action_picker.set_transient_for(Some(&dialog));
    action_picker.set_modal(true);
    action_picker.set_destroy_with_parent(true);
    action_picker.set_hide_on_close(true);
    action_picker.set_title(Some("Choose Action"));
    action_picker.set_titlebar(Some(&gtk::HeaderBar::new()));
    action_picker.set_default_size(400, 480);

    let picker_esc_controller = gtk::EventControllerKey::new();
    let picker_esc_window = action_picker.clone();
    picker_esc_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            picker_esc_window.set_visible(false);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    action_picker.add_controller(picker_esc_controller);

    let picker_vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
    picker_vbox.set_margin_top(6);
    picker_vbox.set_margin_bottom(6);
    picker_vbox.set_margin_start(6);
    picker_vbox.set_margin_end(6);

    let search_entry = gtk::SearchEntry::new();
    search_entry.set_hexpand(true);
    search_entry.set_placeholder_text(Some("Search actions…"));
    picker_vbox.append(&search_entry);

    let scrolled_window = gtk::ScrolledWindow::new();
    scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled_window.set_min_content_height(350);
    scrolled_window.set_min_content_width(320);
    picker_vbox.append(&scrolled_window);

    let list_store = gio::ListStore::new::<glib::BoxedAnyObject>();

    let mut all_options = get_static_action_options();
    all_options.extend(fetch_gnome_action_options());
    all_options.extend(fetch_kde_action_options());
    all_options.sort_by(|a, b| {
        match a.category.cmp(&b.category) {
            std::cmp::Ordering::Equal => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            other => other,
        }
    });

    for opt in &all_options {
        list_store.append(&glib::BoxedAnyObject::new(opt.clone()));
    }

    let filter_text = Rc::new(RefCell::new(String::new()));
    let custom_filter = gtk::CustomFilter::new({
        let filter_text = Rc::clone(&filter_text);
        move |item| {
            let query = filter_text.borrow();
            let boxed = item.downcast_ref::<glib::BoxedAnyObject>().unwrap();
            let opt = boxed.borrow::<EditorActionOption>();
            let category = CATEGORY_NAMES.get(opt.category).unwrap_or(&"");
            search_matches(&format!("{} {}", category, opt.name), &query)
        }
    });

    let search_entry_focus = search_entry.clone();
    action_picker.connect_map(move |_| {
        search_entry_focus.grab_focus();
    });

    let filter_model = gtk::FilterListModel::new(Some(list_store.clone()), Some(custom_filter.clone()));
    let selection_model = gtk::SingleSelection::new(Some(filter_model.clone()));
    selection_model.set_autoselect(false); // Don't auto select

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, list_item| {
        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        box_.set_margin_top(6);
        box_.set_margin_bottom(6);
        box_.set_margin_start(6);
        box_.set_margin_end(6);

        let img = gtk::Image::new();
        img.set_icon_size(gtk::IconSize::Large);
        img.set_valign(gtk::Align::Center);

        let text_vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text_vbox.set_valign(gtk::Align::Center);

        let cat_label = gtk::Label::new(None);
        cat_label.set_halign(gtk::Align::Start);
        cat_label.add_css_class("action-label"); // Dimmed subtitle

        let act_label = gtk::Label::new(None);
        act_label.set_halign(gtk::Align::Start);
        act_label.add_css_class("status-label");
        act_label.set_wrap(true);
        act_label.set_max_width_chars(30);

        text_vbox.append(&cat_label);
        text_vbox.append(&act_label);

        box_.append(&img);
        box_.append(&text_vbox);
        list_item.set_child(Some(&box_));
    });

    factory.connect_bind(|_, list_item| {
        let item = list_item.item().unwrap();
        let boxed = item.downcast_ref::<glib::BoxedAnyObject>().unwrap();
        let opt = boxed.borrow::<EditorActionOption>();

        let child = list_item.child().unwrap();
        let box_ = child.downcast::<gtk::Box>().unwrap();
        let img = box_.first_child().unwrap().downcast::<gtk::Image>().unwrap();
        let text_vbox = img.next_sibling().unwrap().downcast::<gtk::Box>().unwrap();
        let cat_label = text_vbox.first_child().unwrap().downcast::<gtk::Label>().unwrap();
        let act_label = cat_label.next_sibling().unwrap().downcast::<gtk::Label>().unwrap();

        let (icon_name, _) = get_action_category_icon(&opt.action_type);
        img.set_icon_name(Some(icon_name));
        cat_label.set_text(CATEGORY_NAMES.get(opt.category).unwrap_or(&"Unknown"));
        act_label.set_text(&opt.name);
    });

    let list_view = gtk::ListView::new(Some(selection_model.clone()), Some(factory.clone()));
    list_view.set_single_click_activate(true);
    list_view.add_css_class("boxed-list");
    scrolled_window.set_child(Some(&list_view));
    action_picker.set_child(Some(&picker_vbox));
    let picker_window = action_picker.clone();
    action_select_btn.connect_clicked(move |_| picker_window.present());

    let filter_clone = custom_filter.clone();
    let text_clone = Rc::clone(&filter_text);
    search_entry.connect_search_changed(move |entry| {
        *text_clone.borrow_mut() = entry.text().to_string();
        filter_clone.changed(gtk::FilterChange::Different);
    });

    // Helper to dynamically update the gesture name if not customized
    let update_default_name = Rc::new({
        let name_entry = name_entry.clone();
        let action_details_entry = action_details_entry.clone();
        let selected_action = Rc::clone(&selected_action);
        let is_name_customized = Rc::clone(&is_name_customized);
        let is_updating_programmatically = Rc::clone(&is_updating_programmatically);

        move || {
            if *is_name_customized.borrow() {
                return;
            }
            if let Some(opt) = selected_action.borrow().as_ref() {
                let detail = action_details_entry.text().to_string();
                let default_name = get_default_gesture_name(opt, &detail);
                
                *is_updating_programmatically.borrow_mut() = true;
                name_entry.set_text(&default_name);
                *is_updating_programmatically.borrow_mut() = false;
            }
        }
    });

    // Setup record button to open the "Set Shortcut" modal dialog
    let entry_click = action_details_entry.clone();
    let dialog_parent = dialog.clone();
    let name_entry_click = name_entry.clone();
    let udn_click = Rc::clone(&update_default_name);

    record_btn.connect_clicked(move |_| {
        let gesture_name = name_entry_click.text().to_string();
        open_shortcut_recorder(
            &dialog_parent,
            if gesture_name.trim().is_empty() {
                "Gesture"
            } else {
                &gesture_name
            },
            &entry_click,
            Rc::clone(&udn_click) as Rc<dyn Fn()>,
        );
    });

    // Helper to update shortcut display box with keycaps
    let update_shortcut_display = {
        let entry = action_details_entry.clone();
        let display_box = shortcut_display_box.clone();
        Rc::new(move || {
            // Clear previous children
            while let Some(child) = display_box.first_child() {
                display_box.remove(&child);
            }

            let text = entry.text().to_string();
            if text.trim().is_empty() {
                let label = gtk::Label::new(Some("None"));
                label.set_opacity(0.5);
                display_box.append(&label);
            } else {
                let parts: Vec<&str> = text.split('+').filter(|s| !s.trim().is_empty()).collect();
                for (i, part) in parts.iter().enumerate() {
                    if i > 0 {
                        let plus = gtk::Label::new(Some("+"));
                        plus.set_opacity(0.6);
                        display_box.append(&plus);
                    }
                    let label = gtk::Label::new(Some(part));
                    label.add_css_class("keycap");
                    display_box.append(&label);
                }
            }
        })
    };

    let btn_label_clone = action_select_label.clone();
    let btn_icon_clone = action_select_icon.clone();
    let row_clone = action_details_row.clone();
    let entry_clone = action_details_entry.clone();
    let label_clone = action_details_label.clone();
    let sdb_clone_act = shortcut_display_box.clone();
    let record_btn_clone_act = record_btn.clone();
    let udn_clone = Rc::clone(&update_default_name);
    let selected_action_clone = Rc::clone(&selected_action);

    let picker_activate_clone = action_picker.clone();
    let selection_model_clone = selection_model.clone();
    list_view.connect_activate(move |_, position| {
        picker_activate_clone.set_visible(false);
        let item = match selection_model_clone.item(position) {
            Some(i) => i,
            None => return,
        };
        let boxed = item.downcast_ref::<glib::BoxedAnyObject>().unwrap();
        let opt = boxed.borrow::<EditorActionOption>().clone();
        
        btn_label_clone.set_text(&opt.name);
        let (icon_name, _) = get_action_category_icon(&opt.action_type);
        btn_icon_clone.set_icon_name(Some(icon_name));
        *selected_action_clone.borrow_mut() = Some(opt.clone());

        show_action_parameter(
            &opt,
            &row_clone,
            &label_clone,
            &entry_clone,
            &sdb_clone_act,
            &record_btn_clone_act,
        );

        udn_clone();
    });

    let udn_clone4 = Rc::clone(&update_default_name);
    let usd_clone_change = Rc::clone(&update_shortcut_display);
    action_details_entry.connect_changed(move |_| {
        udn_clone4();
        usd_clone_change();
    });

    // Find initial matching option
    let mut initial_opt = None;
    if let Some(ref g) = target_gesture {
        if !g.actions.is_empty() {
            let a = &g.actions[0];
            if let Some(pos) = all_options.iter().position(|opt| action_matches(a, opt)) {
                selection_model.set_selected(pos as u32);
                initial_opt = Some(all_options[pos].clone());
                
                match a {
                    ActionType::Keypress(combo) => action_details_entry.set_text(combo),
                    ActionType::Execute(cmd) => {
                        let opt = &all_options[pos];
                        if opt.category == 7 {
                            action_details_entry.set_text(cmd);
                        }
                    }
                    ActionType::Click(btn) => {
                        let btn_str = btn.map(|b| b.to_string()).unwrap_or_default();
                        action_details_entry.set_text(&btn_str);
                    }
                    _ => {}
                }
            }
        }
    } else {
        let initial_pos = preset_action
            .as_ref()
            .and_then(|preset| {
                all_options
                    .iter()
                    .position(|opt| opt.action_type == preset.action_type)
            })
            .unwrap_or(0);
        selection_model.set_selected(initial_pos as u32);
        initial_opt = Some(all_options[initial_pos].clone());
        let udn_init = Rc::clone(&update_default_name);
        udn_init();
    }

    // Nothing to fill in until an action is known, such as one that's no longer available
    if let Some(list_row) = action_details_row.parent() {
        list_row.set_visible(false);
    }
    if let Some(opt) = initial_opt {
        *selected_action.borrow_mut() = Some(opt.clone());
        show_action_parameter(
            &opt,
            &action_details_row,
            &action_details_label,
            &action_details_entry,
            &shortcut_display_box,
            &record_btn,
        );
        // A chosen action is all a new gesture's default name needs.
        if preset_action.is_some() {
            update_default_name();
        }
        action_select_label.set_text(&opt.name);
        let (icon_name, _) = get_action_category_icon(&opt.action_type);
        action_select_icon.set_icon_name(Some(icon_name));
    }

    let usd_init = Rc::clone(&update_shortcut_display);
    usd_init(); // Set initial keycaps if keys exist

    // Save and Cancel buttons
    let cancel_btn = gtk::Button::with_mnemonic("_Cancel");
    let dialog_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_clone.destroy();
    });
    dialog_header.pack_start(&cancel_btn);

    let save_btn = gtk::Button::with_mnemonic("_Save");
    save_btn.add_css_class("suggested-action");
    save_btn.set_receives_default(true);

    let state_clone = Rc::clone(state_rc);
    let is_edit = target_gesture.is_some();
    let target_id = target_gesture.as_ref().map(|g| g.id.clone());
    let dialog_clone2 = dialog.clone();
    let selected_action_save = Rc::clone(&selected_action);

    let toggles_save = modifier_toggles.clone();
    save_btn.connect_clicked(move |_| {
        let modifiers = toggles_save.get();
        let name = name_entry.text().to_string();
        if name.trim().is_empty() {
            println!("Gesture save failed: Name is empty.");
            return;
        }

        let pts = recorded_points.borrow();
        if pts.len() < 2 {
            println!("Gesture save failed: Not enough points drawn.");
            return;
        }

        // Convert recorded points back to string coords representation
        let raw_movement = pts
            .iter()
            .map(|p| format!("{},{}", p.x as i32, p.y as i32))
            .collect::<Vec<_>>()
            .join(" ");

        let opt = if let Some(opt) = selected_action_save.borrow().as_ref() {
            opt.clone()
        } else {
            println!("Gesture save failed: No action selected.");
            return;
        };
        let detail = action_details_entry.text().to_string();

        let action = match &opt.action_type {
            ActionType::Keypress(_) => ActionType::Keypress(detail),
            ActionType::Execute(_) if opt.category == 7 => ActionType::Execute(detail),
            ActionType::Click(_) => {
                let btn = if detail.trim().is_empty() {
                    None
                } else {
                    detail.trim().parse::<i32>().ok()
                };
                ActionType::Click(btn)
            }
            other => other.clone(),
        };

        let mut state = state_clone.borrow_mut();
        if is_edit {
            let lookup_id = target_id.as_ref().unwrap();
            if let Some(pos) = state
                .config
                .gestures
                .iter()
                .position(|g| g.id == *lookup_id)
            {
                let lookup_name = state.config.gestures[pos].name.clone();
                if name != lookup_name {
                    if state.config.gestures.iter().any(|g| g.name == name) {
                        println!(
                            "Gesture save failed: A gesture with the name '{}' already exists.",
                            name
                        );
                        return;
                    }
                    state.config.gestures[pos].modifiers = modifiers;
                    state.config.gestures[pos].name = name.clone();
                    state.config.gestures[pos].raw_movement = raw_movement;
                    state.config.gestures[pos].points = pts.clone();
                    state.config.gestures[pos].actions = vec![action];
                    println!("Gesture renamed successfully: {} -> {}", lookup_name, name);
                } else {
                    state.config.gestures[pos].modifiers = modifiers;
                    state.config.gestures[pos].raw_movement = raw_movement;
                    state.config.gestures[pos].points = pts.clone();
                    state.config.gestures[pos].actions = vec![action];
                    println!("Gesture modified successfully: {}", name);
                }
            } else {
                println!(
                    "Gesture modification failed: Could not find gesture with ID '{}'",
                    lookup_id
                );
            }
        } else {
            // Check if name conflict
            if state.config.gestures.iter().any(|g| g.name == name) {
                println!(
                    "Gesture save failed: A gesture with the name '{}' already exists.",
                    name
                );
                return;
            }
            println!("Gesture added successfully: {}", name);
            let new_id = generate_unique_id();
            state.config.gestures.push(Gesture {
                id: new_id,
                name: name.clone(),
                raw_movement,
                points: pts.clone(),
                actions: vec![action],
                is_custom: true,
                is_modified: false,
                is_deleted: false,
                modifiers,
            });
            state.newly_added_gestures.push(name.clone());
        }

        if let Err(e) = state.config.save_to_file() {
            println!("Failed to save configuration to file: {}", e);
        }
        reload_daemon(state.dbus_conn.as_ref(), Some(&dialog_clone2));
        drop(state);

        refresh_gesture_list(&state_clone, Some(&name));
        dialog_clone2.destroy();
    });
    dialog_header.pack_end(&save_btn);

    if let Some(ref gest) = target_gesture {
        let delete_btn = gtk::Button::with_mnemonic("_Delete Gesture");
        delete_btn.add_css_class("destructive-action");
        delete_btn.set_halign(gtk::Align::Center);
        delete_btn.set_margin_top(8);

        let state_clone = Rc::clone(state_rc);
        let target_id = gest.id.clone();
        let name_clone = gest.name.clone();
        let dialog_clone3 = dialog.clone();
        delete_btn.connect_clicked(move |_| {
            let state_clone = Rc::clone(&state_clone);
            let target_id = target_id.clone();
            let name_clone = name_clone.clone();
            let dialog_for_delete = dialog_clone3.clone();
            show_confirm_dialog(
                &dialog_clone3,
                "Delete Gesture?",
                &format!("\u{201c}{}\u{201d} will be removed. This can't be undone.", name_clone),
                "Delete",
                true,
                move || {
                    let mut state = state_clone.borrow_mut();
                    if let Some(pos) = state.config.gestures.iter().position(|g| g.id == target_id) {
                        state.config.gestures.remove(pos);
                        if let Err(e) = state.config.save_to_file() {
                            println!("Failed to save config to file on delete: {}", e);
                        }
                        println!("Gesture deleted successfully: {}", name_clone);
                        reload_daemon(state.dbus_conn.as_ref(), Some(&dialog_for_delete));
                        drop(state);
                        refresh_gesture_list(&state_clone, None);
                        dialog_for_delete.destroy();
                    }
                },
            );
        });
        main_box.append(&delete_btn);
    }

    dialog.set_default_widget(Some(&save_btn));
    dialog.present();
}
