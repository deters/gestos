use crate::daemon::{is_daemon_running, turn_off, turn_on};
use crate::system::{get_autostart_file_path, set_autostart_enabled};
use gtk::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use std::cell::Cell;
use std::rc::Rc;

const STATUS_POLL_INTERVAL_SECS: u64 = 1;

fn status_text(running: bool, autostart: bool) -> &'static str {
    match (running, autostart) {
        (true, _) => "Running. Hold the gesture button (normally right) and draw.",
        (false, true) => "Stopped. Your mouse works normally. Gestures start again at next login.",
        (false, false) => "Stopped. Your mouse works normally.",
    }
}

/// The app's main control: whether gestures are on, and whether they start at login. Daemon
/// calls can block, so they all run off the UI thread.
#[derive(Clone)]
pub struct StatusCard {
    root: gtk::Box,
    status_label: gtk::Label,
    action_button: gtk::Button,
    autostart_switch: Option<gtk::Switch>,
    error_scroller: gtk::ScrolledWindow,
    error_label: gtk::Label,
    dbus_conn: Option<zbus::blocking::Connection>,
    busy: Rc<Cell<bool>>,
    polling: Rc<Cell<bool>>,
    updating_switches: Rc<Cell<bool>>,
    last_running: Rc<Cell<Option<bool>>>,
}

fn card_row() -> (gtk::ListBoxRow, gtk::Box) {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_start(16);
    row_box.set_margin_end(16);
    row_box.set_margin_top(12);
    row_box.set_margin_bottom(12);
    row.set_child(Some(&row_box));
    (row, row_box)
}

fn row_text(title: &str, subtitle: &str) -> (gtk::Box, gtk::Label) {
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

    (text, subtitle_label)
}

impl StatusCard {
    pub fn new(dbus_conn: Option<zbus::blocking::Connection>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_halign(gtk::Align::Center);
        root.set_width_request(500);
        root.set_margin_top(24);
        root.set_margin_start(24);
        root.set_margin_end(24);

        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        root.append(&list);

        let (gestures_row, gestures_box) = card_row();
        let (gestures_text, status_label) = row_text("Mouse gestures", "Checking...");
        gestures_box.append(&gestures_text);
        let action_button = gtk::Button::with_label("Start");
        action_button.set_valign(gtk::Align::Center);
        action_button.set_width_request(80);
        action_button.set_sensitive(false);
        gestures_box.append(&action_button);
        list.append(&gestures_row);

        let manual_autostart =
            gestos::config::manual_autostart_for(gestos::config::get_environment_suffix());
        let (autostart_row, autostart_box) = card_row();
        let autostart_switch = match manual_autostart {
            Some((desktop, config_file, line)) => {
                let subtitle =
                    format!("{desktop} doesn't use this setting. Add this line to {config_file}:");
                let (text, _) = row_text("Start when I log in", &subtitle);
                let config_line = gtk::Label::new(Some(line));
                config_line.add_css_class("monospace");
                config_line.set_halign(gtk::Align::Start);
                config_line.set_selectable(true);
                text.append(&config_line);
                autostart_box.append(&text);
                None
            }
            None => {
                let (text, _) = row_text("Start when I log in", "Start gestures automatically");
                autostart_box.append(&text);
                let switch = gtk::Switch::new();
                switch.set_valign(gtk::Align::Center);
                autostart_box.append(&switch);
                Some(switch)
            }
        };
        list.append(&autostart_row);

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("warning-label");
        error_label.set_wrap(true);
        error_label.set_selectable(true);
        error_label.set_xalign(0.0);
        error_label.set_halign(gtk::Align::Start);
        let error_scroller = gtk::ScrolledWindow::new();
        error_scroller.add_css_class("warning-box");
        error_scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        error_scroller.set_propagate_natural_height(true);
        error_scroller.set_min_content_height(80);
        error_scroller.set_max_content_height(180);
        error_scroller.set_child(Some(&error_label));
        error_scroller.set_visible(false);
        root.append(&error_scroller);

        let card = StatusCard {
            root,
            status_label,
            action_button,
            autostart_switch,
            error_scroller,
            error_label,
            dbus_conn,
            busy: Rc::new(Cell::new(false)),
            polling: Rc::new(Cell::new(false)),
            updating_switches: Rc::new(Cell::new(false)),
            last_running: Rc::new(Cell::new(None)),
        };
        card.connect_signals();
        card.start_polling();
        card
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    fn connect_signals(&self) {
        let card = self.clone();
        self.action_button.connect_clicked(move |_| {
            let start = card.last_running.get() != Some(true);
            card.set_gestures(start);
        });

        if let Some(switch) = &self.autostart_switch {
            let card = self.clone();
            switch.connect_state_set(move |_, enabled| {
                if !card.updating_switches.get() {
                    card.show_error(None);
                    if let Err(e) = set_autostart_enabled(enabled) {
                        card.show_error(Some(&format!("Could not change the login setting: {e}")));
                    }
                    card.render();
                }
                glib::Propagation::Proceed
            });
        }
    }

    fn start_polling(&self) {
        self.poll();
        let card = self.clone();
        glib::timeout_add_seconds_local(STATUS_POLL_INTERVAL_SECS as u32, move || {
            card.poll();
            glib::ControlFlow::Continue
        });
    }

    fn poll(&self) {
        if self.busy.get() || self.polling.get() {
            return;
        }
        self.polling.set(true);
        let conn = self.dbus_conn.clone();
        let check = gio::spawn_blocking(move || is_daemon_running(conn.as_ref()));
        let card = self.clone();
        glib::spawn_future_local(async move {
            let running = check.await.unwrap_or(false);
            card.polling.set(false);
            if !card.busy.get() {
                // A daemon started some other way resolves an earlier failure to start.
                if running && card.last_running.get() != Some(true) {
                    card.show_error(None);
                }
                card.last_running.set(Some(running));
                card.render();
            }
        });
    }

    fn set_gestures(&self, start: bool) {
        if self.busy.get() {
            return;
        }
        self.busy.set(true);
        self.show_error(None);
        self.action_button.set_sensitive(false);
        self.status_label.set_text(if start {
            "Starting..."
        } else {
            "Stopping..."
        });

        let conn = self.dbus_conn.clone();
        let change = gio::spawn_blocking(move || {
            let result = if start {
                turn_on(conn.as_ref())
            } else {
                turn_off(conn.as_ref());
                Ok(())
            };
            (result, is_daemon_running(conn.as_ref()))
        });
        let card = self.clone();
        glib::spawn_future_local(async move {
            let (result, running) = change
                .await
                .unwrap_or_else(|_| (Err("The operation stopped unexpectedly.".to_string()), false));
            card.busy.set(false);
            if let Err(message) = result {
                card.show_error(Some(&message));
            }
            card.last_running.set(Some(running));
            card.render();
        });
    }

    fn autostart_enabled() -> bool {
        get_autostart_file_path().is_some_and(|path| path.exists())
    }

    fn render(&self) {
        let Some(running) = self.last_running.get() else {
            return;
        };
        let manual_autostart = self.autostart_switch.is_none();
        let autostart = !manual_autostart && Self::autostart_enabled();

        self.status_label.set_text(status_text(running, autostart));
        self.action_button
            .set_label(if running { "Stop" } else { "Start" });
        if running {
            self.action_button.remove_css_class("suggested-action");
        } else {
            self.action_button.add_css_class("suggested-action");
        }
        self.action_button.set_sensitive(true);

        self.updating_switches.set(true);
        if let Some(switch) = &self.autostart_switch {
            switch.set_active(autostart);
        }
        self.updating_switches.set(false);
    }

    fn show_error(&self, message: Option<&str>) {
        self.error_label.set_text(message.unwrap_or_default());
        self.error_scroller.set_visible(message.is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_warns_only_when_gestures_will_return_at_login() {
        assert!(status_text(false, true).contains("next login"));
        assert!(!status_text(false, false).contains("next login"));
        assert!(status_text(true, true).starts_with("Running."));
        assert!(status_text(true, false).starts_with("Running."));
    }
}
