use crate::daemon::{is_daemon_running, turn_off, turn_on};
use crate::system::{
    get_autostart_file_path, mark_autostart_decided, set_autostart_enabled,
    should_enable_autostart_on_first_start,
};
use gtk::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use std::cell::Cell;
use std::rc::Rc;

const STATUS_POLL_INTERVAL_SECS: u32 = 1;

fn status_text(running: bool, autostart: bool) -> &'static str {
    match (running, autostart) {
        (true, _) => "Running",
        (false, true) => "Stopped, starts again at next login",
        (false, false) => "Stopped",
    }
}

fn button_tooltip(running: bool) -> &'static str {
    if running {
        "Stop gestures. Your mouse will work normally."
    } else {
        "Start gestures. Hold the gesture button (normally right) and draw."
    }
}

/// One plain sentence for the error area. The daemon's full output, with its setup steps, stays
/// available under Details.
fn summarize_error(message: &str) -> String {
    if message.contains("Missing permissions") {
        return "Gestos can't access your mouse. Reinstalling usually fixes this.".to_string();
    }
    let first_line = message.lines().find(|line| !line.trim().is_empty()).unwrap_or(message);
    first_line.trim_start_matches("ERROR: ").trim().to_string()
}

/// The app's main control, which lives in the header bar: a title showing whether gestures are
/// running, a Start or Stop button, and areas for errors and notices in the window's content. Daemon calls
/// can block, so they all run off the UI thread.
#[derive(Clone)]
pub struct GestureStatus {
    title: gtk::Box,
    status_label: gtk::Label,
    action_button: gtk::Button,
    error_box: gtk::Box,
    error_summary: gtk::Label,
    error_details: gtk::Expander,
    error_label: gtk::Label,
    notice_box: gtk::Box,
    dbus_conn: Option<zbus::blocking::Connection>,
    busy: Rc<Cell<bool>>,
    polling: Rc<Cell<bool>>,
    last_running: Rc<Cell<Option<bool>>>,
}

impl GestureStatus {
    pub fn new(title_text: &str, dbus_conn: Option<zbus::blocking::Connection>) -> Self {
        let title = gtk::Box::new(gtk::Orientation::Vertical, 0);
        title.set_valign(gtk::Align::Center);
        let title_label = gtk::Label::new(Some(title_text));
        title_label.add_css_class("title");
        title.append(&title_label);
        let status_label = gtk::Label::new(Some("Checking…"));
        status_label.add_css_class("subtitle");
        status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.append(&status_label);

        let action_button = gtk::Button::with_label("Start");
        action_button.set_sensitive(false);

        let error_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        error_box.add_css_class("warning-box");
        error_box.set_halign(gtk::Align::Center);
        error_box.set_width_request(500);
        error_box.set_margin_top(12);
        error_box.set_margin_start(24);
        error_box.set_margin_end(24);
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

        let notice_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        notice_box.add_css_class("notice-box");
        notice_box.set_halign(gtk::Align::Center);
        notice_box.set_width_request(500);
        notice_box.set_margin_top(12);
        notice_box.set_margin_start(24);
        notice_box.set_margin_end(24);
        notice_box.set_visible(false);
        let notice_label = gtk::Label::new(Some("Gestures will also start when you log in."));
        notice_label.set_hexpand(true);
        notice_label.set_xalign(0.0);
        notice_label.set_wrap(true);
        notice_box.append(&notice_label);
        let undo_button = gtk::Button::with_mnemonic("_Undo");
        undo_button.set_valign(gtk::Align::Center);
        notice_box.append(&undo_button);
        let dismiss_button = gtk::Button::from_icon_name("window-close-symbolic");
        dismiss_button.add_css_class("flat");
        dismiss_button.set_valign(gtk::Align::Center);
        dismiss_button.set_tooltip_text(Some("Dismiss"));
        notice_box.append(&dismiss_button);

        let status = GestureStatus {
            title,
            status_label,
            action_button,
            error_box,
            error_summary,
            error_details,
            error_label,
            notice_box,
            dbus_conn,
            busy: Rc::new(Cell::new(false)),
            polling: Rc::new(Cell::new(false)),
            last_running: Rc::new(Cell::new(None)),
        };

        let clicked = status.clone();
        status.action_button.connect_clicked(move |_| {
            let start = clicked.last_running.get() != Some(true);
            clicked.set_gestures(start);
        });

        let undone = status.clone();
        undo_button.connect_clicked(move |_| {
            undone.notice_box.set_visible(false);
            if let Err(e) = set_autostart_enabled(false) {
                undone.show_error(Some(&format!("Could not change the login setting: {e}")));
            }
            undone.render();
        });
        let dismissed = status.notice_box.clone();
        dismiss_button.connect_clicked(move |_| dismissed.set_visible(false));

        status.start_polling();
        status
    }

    pub fn title(&self) -> &gtk::Box {
        &self.title
    }

    pub fn action_button(&self) -> &gtk::Button {
        &self.action_button
    }

    pub fn error_area(&self) -> &gtk::Box {
        &self.error_box
    }

    pub fn notice_area(&self) -> &gtk::Box {
        &self.notice_box
    }

    /// A new user's first start also turns on login startup, with a way to undo it. Either
    /// way, it's only decided for them once.
    fn enable_autostart_for_new_user(&self) {
        let result = mark_autostart_decided().and_then(|_| set_autostart_enabled(true));
        match result {
            Ok(()) => self.notice_box.set_visible(true),
            Err(e) => self.show_error(Some(&format!("Could not change the login setting: {e}"))),
        }
    }

    fn start_polling(&self) {
        self.poll();
        let status = self.clone();
        glib::timeout_add_seconds_local(STATUS_POLL_INTERVAL_SECS, move || {
            status.poll();
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
        let status = self.clone();
        glib::spawn_future_local(async move {
            let running = check.await.unwrap_or(false);
            status.polling.set(false);
            if !status.busy.get() {
                // A daemon started some other way resolves an earlier failure to start.
                if running && status.last_running.get() != Some(true) {
                    status.show_error(None);
                }
                status.last_running.set(Some(running));
                status.render();
            }
        });
    }

    fn set_gestures(&self, start: bool) {
        if self.busy.get() {
            return;
        }
        self.busy.set(true);
        self.show_error(None);
        self.notice_box.set_visible(false);
        self.action_button.set_sensitive(false);
        let new_user = start && should_enable_autostart_on_first_start();
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
        let status = self.clone();
        glib::spawn_future_local(async move {
            let (result, running) = change
                .await
                .unwrap_or_else(|_| (Err("The operation stopped unexpectedly.".to_string()), false));
            status.busy.set(false);
            match result {
                Ok(()) if new_user => status.enable_autostart_for_new_user(),
                Ok(()) => {}
                Err(message) => status.show_error(Some(&message)),
            }
            status.last_running.set(Some(running));
            status.render();
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
        self.action_button.set_tooltip_text(Some(button_tooltip(running)));
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
        assert_eq!(status_text(true, true), "Running");
        assert_eq!(status_text(true, false), "Running");
    }
}
