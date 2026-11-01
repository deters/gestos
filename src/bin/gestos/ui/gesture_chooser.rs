use crate::ui::components::draw_gesture_path;
use gestos::chooser::ChooserEntry;
use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4 as gtk;
use std::cell::Cell;
use std::rc::Rc;

const CANCEL_AFTER_SECS: u32 = 15;
const CARD_MARGIN: f64 = 12.0;
const POINTER_WAIT_MILLIS: u64 = 150;

const EXIT_CANCELLED: i32 = 1;

fn pick(choice: usize) -> ! {
    println!("{choice}");
    std::process::exit(0);
}

fn cancel() -> ! {
    std::process::exit(EXIT_CANCELLED);
}

/// Keeps the card fully on screen when the pointer is near an edge.
fn card_position(pointer: f64, card_size: f64, screen_size: f64) -> f64 {
    let centered = pointer - card_size / 2.0;
    centered.min(screen_size - card_size - CARD_MARGIN).max(CARD_MARGIN)
}

fn entry_button(index: usize, entry: &ChooserEntry) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    let preview = gtk::DrawingArea::new();
    preview.set_content_width(72);
    preview.set_content_height(48);
    let points = entry.points.clone();
    preview.set_draw_func(move |_, cr, width, height| {
        draw_gesture_path(cr, &points, width as f64, height as f64, false, true);
    });
    content.append(&preview);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);
    let name = gtk::Label::new(Some(&entry.name));
    name.add_css_class("heading");
    name.set_xalign(0.0);
    text.append(&name);
    let description = gtk::Label::new(Some(&entry.description));
    description.add_css_class("dim-label");
    description.set_xalign(0.0);
    text.append(&description);
    content.append(&text);

    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.set_child(Some(&content));
    button.connect_clicked(move |_| pick(index));
    button
}

/// A transparent layer over the desktop with the candidates under the pointer. It's this large
/// because that's the only way to place something at the pointer on Wayland.
pub fn build_chooser_ui(app: &gtk::Application, entries: &[ChooserEntry]) {
    let window = gtk::ApplicationWindow::new(app);
    window.set_decorated(false);
    window.add_css_class("gesture-chooser");

    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "window.gesture-chooser { background: transparent; }\n\
         window.gesture-chooser.revealed { background: rgba(0, 0, 0, 0.12); }\n\
         .chooser-card { background-color: @theme_bg_color; color: @theme_fg_color; border: 1px solid alpha(currentColor, 0.2); border-radius: 12px; padding: 8px; box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3); }\n"
    );
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("chooser-card");
    for (index, entry) in entries.iter().enumerate() {
        card.append(&entry_button(index, entry));
    }

    let layer = gtk::Fixed::new();
    layer.put(&card, 0.0, 0.0);
    layer.set_hexpand(true);
    layer.set_vexpand(true);
    window.set_child(Some(&layer));

    // Nothing shows until the card is where it belongs, so it's never seen moving there.
    card.set_opacity(0.0);
    let reveal = {
        let window = window.clone();
        let card = card.clone();
        move || {
            card.set_opacity(1.0);
            window.add_css_class("revealed");
        }
    };

    // Placed once, where the pointer is when the layer appears, so the options don't move away.
    let placed = Rc::new(Cell::new(false));
    let motion = gtk::EventControllerMotion::new();
    let place = {
        let window = window.clone();
        let layer = layer.clone();
        let card = card.clone();
        let placed = Rc::clone(&placed);
        let reveal = reveal.clone();
        move |x: f64, y: f64| {
            if placed.replace(true) {
                return;
            }
            let (_, natural, _, _) = card.measure(gtk::Orientation::Horizontal, -1);
            let (_, natural_height, _, _) = card.measure(gtk::Orientation::Vertical, -1);
            layer.move_(
                &card,
                card_position(x, natural as f64, window.width() as f64),
                card_position(y, natural_height as f64, window.height() as f64),
            );
            reveal();
        }
    };
    let place_on_enter = place.clone();
    motion.connect_enter(move |_, x, y| place_on_enter(x, y));
    motion.connect_motion(move |_, x, y| place(x, y));
    window.add_controller(motion);

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(|_, keyval, _, _| {
        if keyval == gdk::Key::Escape {
            cancel();
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);

    // Clicking anywhere but an option cancels, with any button.
    let click_away = gtk::GestureClick::new();
    click_away.set_button(0);
    click_away.connect_released(|_, _, _, _| cancel());
    window.add_controller(click_away);

    // The pointer is only reported while it's over this window, and might not be, such as on
    // the top bar. Rather than stay hidden, show centered, and move once it's reported.
    glib::timeout_add_local_once(std::time::Duration::from_millis(POINTER_WAIT_MILLIS), {
        let window = window.clone();
        let layer = layer.clone();
        let card = card.clone();
        let placed = Rc::clone(&placed);
        move || {
            if !placed.get() {
                let (_, natural, _, _) = card.measure(gtk::Orientation::Horizontal, -1);
                let (_, natural_height, _, _) = card.measure(gtk::Orientation::Vertical, -1);
                layer.move_(
                    &card,
                    (window.width() - natural).max(0) as f64 / 2.0,
                    (window.height() - natural_height).max(0) as f64 / 2.0,
                );
                reveal();
            }
        }
    });

    glib::timeout_add_seconds_local(CANCEL_AFTER_SECS, || cancel());

    // Maximized rather than fullscreen, because a compositor doesn't draw what's behind a
    // fullscreen window, which would show as black through the transparent background. The size
    // covers desktops that don't maximize it.
    if let Some(monitor) = gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_then(|monitor| monitor.downcast::<gdk::Monitor>().ok())
    {
        let geometry = monitor.geometry();
        window.set_default_size(geometry.width(), geometry.height());
    }
    window.maximize();
    window.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_stays_on_screen() {
        assert_eq!(card_position(500.0, 100.0, 1000.0), 450.0);
        assert_eq!(card_position(0.0, 100.0, 1000.0), CARD_MARGIN);
        assert_eq!(card_position(1000.0, 100.0, 1000.0), 900.0 - CARD_MARGIN);
    }
}
