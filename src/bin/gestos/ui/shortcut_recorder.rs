use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub fn open_shortcut_recorder(
    parent: &gtk::Window,
    gesture_name: &str,
    entry: &gtk::Entry,
    udn: Rc<dyn Fn() + 'static>,
) {
    let dialog = gtk::Window::new();
    dialog.set_transient_for(Some(parent));
    dialog.set_modal(true);
    dialog.set_default_size(440, 300);
    dialog.set_resizable(false);

    // Every key is recorded, Escape and Backspace too, as apps use them, so the buttons are what
    // cancel and clear it.
    let header = gtk::HeaderBar::new();
    header.set_show_title_buttons(false);
    let title_label = gtk::Label::new(Some("Set Shortcut"));
    title_label.add_css_class("title");
    header.set_title_widget(Some(&title_label));
    dialog.set_titlebar(Some(&header));

    let cancel_btn = gtk::Button::with_label("Cancel");
    let dialog_cancel_clone = dialog.clone();
    cancel_btn.connect_clicked(move |_| {
        dialog_cancel_clone.destroy();
    });
    header.pack_start(&cancel_btn);

    // Only once there's a shortcut to set.
    let set_btn = gtk::Button::with_label("Set");
    set_btn.add_css_class("suggested-action");
    set_btn.set_sensitive(false);
    header.pack_end(&set_btn);

    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
    wrapper.add_css_class("dialog-content");
    wrapper.set_vexpand(true);
    wrapper.set_hexpand(true);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 16);
    vbox.set_margin_start(24);
    vbox.set_margin_end(24);
    vbox.set_margin_top(24);
    vbox.set_margin_bottom(24);
    vbox.set_valign(gtk::Align::Center);
    vbox.set_halign(gtk::Align::Center);
    vbox.set_vexpand(true);
    vbox.set_hexpand(true);

    wrapper.append(&vbox);

    let prompt_label = gtk::Label::new(None);
    let escaped_name = glib::markup_escape_text(gesture_name);
    prompt_label.set_markup(&format!(
        "Enter new shortcut to change <b>{}</b>",
        escaped_name
    ));
    prompt_label.set_wrap(true);
    prompt_label.set_justify(gtk::Justification::Center);
    vbox.append(&prompt_label);

    // Initial keyboard image
    let kb_image = gtk::Image::from_icon_name("input-keyboard-symbolic");
    kb_image.set_pixel_size(96);
    kb_image.set_opacity(0.8);
    kb_image.set_margin_top(8);
    kb_image.set_margin_bottom(8);
    vbox.append(&kb_image);

    // Container for displaying the keycaps combination
    let shortcut_display_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    shortcut_display_box.set_halign(gtk::Align::Center);
    shortcut_display_box.set_valign(gtk::Align::Center);
    shortcut_display_box.set_margin_top(24);
    shortcut_display_box.set_margin_bottom(24);
    shortcut_display_box.set_visible(false);
    vbox.append(&shortcut_display_box);

    // A shortcut that's set can be taken away.
    let clear_btn = gtk::Button::with_label("Clear Shortcut");
    clear_btn.add_css_class("flat");
    clear_btn.set_halign(gtk::Align::Center);
    clear_btn.set_visible(!entry.text().is_empty());
    let entry_clear = entry.clone();
    let udn_clear = Rc::clone(&udn);
    let dialog_clear = dialog.clone();
    clear_btn.connect_clicked(move |_| {
        entry_clear.set_text("");
        udn_clear();
        dialog_clear.destroy();
    });
    vbox.append(&clear_btn);

    dialog.set_child(Some(&wrapper));

    // Captured key combination state
    let captured_combination = Rc::new(RefCell::new(None::<String>));

    // Set button click callback
    let captured_save = Rc::clone(&captured_combination);
    let entry_save = entry.clone();
    let udn_save = Rc::clone(&udn);
    let dialog_save = dialog.clone();
    set_btn.connect_clicked(move |_| {
        if let Some(ref combo_str) = *captured_save.borrow() {
            entry_save.set_text(combo_str);
            udn_save();
        }
        dialog_save.destroy();
    });

    // Keys are caught before the window's own handling, which would move the focus with Tab or
    // press a button with Enter.
    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);

    // Shows a combination and keeps it as the one to set, or only shows it while it's held.
    let show: Rc<dyn Fn(&[&str], Option<&str>, bool)> = {
        let captured = Rc::clone(&captured_combination);
        let kb_image = kb_image.clone();
        let display_box = shortcut_display_box.clone();
        let set_btn = set_btn.clone();
        let clear_btn = clear_btn.clone();
        Rc::new(move |modifiers: &[&str], key: Option<&str>, keep: bool| {
            if keep {
                let combo: Vec<&str> = modifiers.iter().copied().chain(key).collect();
                *captured.borrow_mut() = Some(combo.join("+"));
                set_btn.set_sensitive(true);
            }
            clear_btn.set_visible(false);
            kb_image.set_visible(false);
            while let Some(child) = display_box.first_child() {
                display_box.remove(&child);
            }
            let shown = modifiers.iter().map(|m| if *m == "Control" { "Ctrl" } else { m });
            let key = key.map(|key| if key.len() == 1 { key.to_uppercase() } else { key.to_string() });
            for (i, part) in shown.map(str::to_string).chain(key).enumerate() {
                if i > 0 {
                    let plus = gtk::Label::new(Some("+"));
                    plus.set_opacity(0.6);
                    display_box.append(&plus);
                }
                let label = gtk::Label::new(Some(&part));
                label.add_css_class("keycap");
                display_box.append(&label);
            }
            display_box.set_visible(true);
        })
    };

    // The modifiers pressed since none were held, and whether another key came with them.
    // Modifiers alone, like Ctrl+Super, are a shortcut too, set once they're let go.
    let held = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let with_key = Rc::new(Cell::new(false));

    {
        let held = Rc::clone(&held);
        let with_key = Rc::clone(&with_key);
        let show = Rc::clone(&show);
        key_controller.connect_key_pressed(move |_, keyval, _keycode, state| {
            if let Some(modifier) = modifier_name(keyval) {
                // The first modifier of a new combination.
                if modifiers_in(state).is_empty() {
                    held.borrow_mut().clear();
                    with_key.set(false);
                }
                let mut held = held.borrow_mut();
                if !held.contains(&modifier) {
                    held.push(modifier);
                }
                if !with_key.get() {
                    show(&in_order(&held), None, false);
                }
                return glib::Propagation::Stop;
            }
            with_key.set(true);
            // Page Down shares its keysym with the alias "Next", which the daemon reads as the
            // media key, so the page keys are always named explicitly.
            // Shift+Tab reports a key of its own, which is Tab with the Shift already listed.
            let mut key_name = match keyval {
                gdk::Key::Page_Up => "Page_Up".to_string(),
                gdk::Key::Page_Down => "Page_Down".to_string(),
                gdk::Key::ISO_Left_Tab => "Tab".to_string(),
                _ => keyval.name().map(|s| s.to_string()).unwrap_or_default(),
            };
            if key_name.len() == 1 {
                key_name = key_name.to_uppercase();
            }
            let modifiers = in_order(&modifiers_in(state));
            show(&modifiers, (!key_name.is_empty()).then_some(key_name.as_str()), true);
            glib::Propagation::Stop
        });
    }
    {
        key_controller.connect_key_released(move |_, keyval, _keycode, _state| {
            // The first modifier let go ends a combination of modifiers alone.
            if modifier_name(keyval).is_some() && !with_key.get() && !held.borrow().is_empty() {
                let modifiers = in_order(&held.borrow());
                show(&modifiers, None, true);
                with_key.set(true);
            }
        });
    }
    // Shortcuts the desktop keeps for itself, like Ctrl+Alt+Up to change workspaces, reach the
    // window too while it's open, as in GNOME Settings. On X11 that's done by taking the keyboard.
    dialog.connect_map(|dialog| {
        if let Some(toplevel) = dialog.surface().and_downcast::<gdk::Toplevel>() {
            toplevel.inhibit_system_shortcuts(None::<&gdk::Event>);
        }
    });
    dialog.connect_unmap(|dialog| {
        if let Some(toplevel) = dialog.surface().and_downcast::<gdk::Toplevel>() {
            toplevel.restore_system_shortcuts();
        }
    });
    dialog.add_controller(key_controller);
    dialog.present();
}

/// The modifier a key is, by the name shortcuts give it. Meta is Alt, as most keyboards have it.
fn modifier_name(keyval: gdk::Key) -> Option<&'static str> {
    match keyval {
        gdk::Key::Control_L | gdk::Key::Control_R => Some("Control"),
        gdk::Key::Alt_L | gdk::Key::Alt_R | gdk::Key::Meta_L | gdk::Key::Meta_R => Some("Alt"),
        gdk::Key::Shift_L | gdk::Key::Shift_R => Some("Shift"),
        gdk::Key::Super_L | gdk::Key::Super_R | gdk::Key::Hyper_L | gdk::Key::Hyper_R => {
            Some("Super")
        }
        _ => None,
    }
}

/// The modifiers held as a key is pressed. On X11, Alt can be reported as Meta, and Super as Hyper,
/// depending on how the keyboard is set up.
fn modifiers_in(state: gdk::ModifierType) -> Vec<&'static str> {
    let mut modifiers = Vec::new();
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        modifiers.push("Control");
    }
    if state.intersects(gdk::ModifierType::ALT_MASK | gdk::ModifierType::META_MASK) {
        modifiers.push("Alt");
    }
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        modifiers.push("Shift");
    }
    if state.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::HYPER_MASK) {
        modifiers.push("Super");
    }
    modifiers
}

/// Modifiers in the order shortcuts name them: Ctrl, Alt, Shift, Super.
fn in_order(modifiers: &[&'static str]) -> Vec<&'static str> {
    ["Control", "Alt", "Shift", "Super"]
        .into_iter()
        .filter(|modifier| modifiers.contains(modifier))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_are_named_in_order_whatever_order_they_were_pressed() {
        assert_eq!(in_order(&["Super", "Control"]), ["Control", "Super"]);
        assert_eq!(in_order(&["Super", "Alt", "Control"]), ["Control", "Alt", "Super"]);
    }

    #[test]
    fn alt_and_super_are_found_however_x11_reports_them() {
        let state = gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::META_MASK
            | gdk::ModifierType::HYPER_MASK;
        assert_eq!(modifiers_in(state), ["Control", "Alt", "Super"]);
        assert_eq!(modifier_name(gdk::Key::Meta_L), Some("Alt"));
        assert_eq!(modifier_name(gdk::Key::Super_R), Some("Super"));
        assert_eq!(modifier_name(gdk::Key::Up), None);
    }
}
