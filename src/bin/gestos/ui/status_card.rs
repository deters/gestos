use crate::daemon::{is_daemon_running, turn_off, turn_on};
use crate::system::get_autostart_file_path;
use gtk::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use std::cell::Cell;
use std::rc::Rc;

const STATUS_POLL_INTERVAL_SECS: u64 = 1;

fn status_text(running: bool, autostart: bool) -> &'static str {
    match (running, autostart) {
        (true, _) => "Running. Hold the gesture button (normally right) and draw.",
        (false, true) => "Stopped. Gestures start again at next login.",
        (false, false) => "Stopped. Your mouse works normally.",
    }
}

/// One plain sentence for the card. The daemon's full output, with its setup steps, stays
/// available under Details.
fn summarize_error(message: &str) -> String {
    if message.contains("Missing permissions") {
        return "Gestos can't access your mouse. Reinstalling usually fixes this.".to_string();
    }
    let first_line = message.lines().find(|line| !line.trim().is_empty()).unwrap_or(message);
    first_line.trim_start_matches("ERROR: ").trim().to_string()
}

/// The app's main control: whether gestures are running. Daemon calls can block, so they all run off the UI thread.
#[derive(Clone)]
pub struct StatusCard {
    root: gtk::Box,
    status_label: gtk::Label,
    action_button: gtk::Button,
    error_box: gtk::Box,
    error_summary: gtk::Label,
    error_details: gtk::Expander,
    error_label: gtk::Label,
    dbus_conn: Option<zbus::blocking::Connection>,
    busy: Rc<Cell<bool>>,
    polling: Rc<Cell<bool>>,
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
        root.set_margin_top(12);
        root.set_margin_start(24);
        root.set_margin_end(24);

        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        root.append(&list);

        let (gestures_row, gestures_box) = card_row();
        let (gestures_text, status_label) = row_text("Mouse Gestures", "Checking…");
        gestures_box.append(&gestures_text);
        let action_button = gtk::Button::with_label("Start");
        action_button.set_valign(gtk::Align::Center);
        action_button.set_width_request(80);
        action_button.set_sensitive(false);
        gestures_box.append(&action_button);
        list.append(&gestures_row);

        let error_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        error_box.add_css_class("warning-box");
        error_box.set_visible(false);

        let error_summary = gtk::Label::new(None);
        error_summary.add_css_class("warning-label");
        error_summary.set_wrap(true);
        error_summary.set_xalign(0.0);
        error_summary.set_halign(gtk::Align::Start);
        error_box.append(&error_summary);

        let error_label = gtk::Label::new(None);
        error_label.set_wrap(true);
        error_label.set_selectable(true);
        error_label.set_xalign(0.0);
        error_label.set_halign(gtk::Align::Start);
        let error_scroller = gtk::ScrolledWindow::new();
        error_scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        error_scroller.set_propagate_natural_height(true);
        error_scroller.set_min_content_height(80);
        error_scroller.set_max_content_height(180);
        error_scroller.set_child(Some(&error_label));
        let error_details = gtk::Expander::with_mnemonic("_Details");
        error_details.set_child(Some(&error_scroller));
        error_box.append(&error_details);
        root.append(&error_box);

        let card = StatusCard {
            root,
            status_label,
            action_button,
            error_box,
            error_summary,
            error_details,
            error_label,
            dbus_conn,
            busy: Rc::new(Cell::new(false)),
            polling: Rc::new(Cell::new(false)),
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
            "Starting…"
        } else {
            "Stopping…"
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

    /// Whether gestures will start at next login. A desktop that needs a manual config line
    /// doesn't use the autostart entry, so nothing can be said about it there.
    fn autostart_enabled() -> bool {
        gestos::config::manual_autostart_for(gestos::config::get_environment_suffix()).is_none()
            && get_autostart_file_path().is_some_and(|path| path.exists())
    }

    fn render(&self) {
        let Some(running) = self.last_running.get() else {
            return;
        };

        self.status_label
            .set_text(status_text(running, Self::autostart_enabled()));
        self.action_button
            .set_label(if running { "Stop" } else { "Start" });
        if running {
            self.action_button.remove_css_class("suggested-action");
        } else {
            self.action_button.add_css_class("suggested-action");
        }
        self.action_button.set_sensitive(true);
    }

    fn show_error(&self, message: Option<&str>) {
        self.error_box.set_visible(message.is_some());
        let Some(message) = message else {
            return;
        };
        let summary = summarize_error(message);
        self.error_summary.set_text(&summary);
        self.error_label.set_text(message);
        self.error_details.set_expanded(false);
        self.error_details.set_visible(summary != message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_summarized_in_one_line() {
        let permissions = "ERROR: Missing permissions or kernel module to run gestosd.\n - Cannot read";
        assert!(summarize_error(permissions).starts_with("Gestos can't access your mouse."));
        assert_eq!(
            summarize_error("\nERROR: Something else\nmore detail"),
            "Something else"
        );
        assert_eq!(summarize_error("Just one line"), "Just one line");
    }

    #[test]
    fn stopped_warns_only_when_gestures_will_return_at_login() {
        assert!(status_text(false, true).contains("next login"));
        assert!(!status_text(false, false).contains("next login"));
        assert!(status_text(true, true).starts_with("Running."));
        assert!(status_text(true, false).starts_with("Running."));
    }
}
