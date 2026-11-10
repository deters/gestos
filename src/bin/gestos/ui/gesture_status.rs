use crate::daemon::{is_daemon_running, turn_off, turn_on};
use crate::system::{
    get_autostart_file_path, is_stopped_by_user, mark_autostart_decided, set_autostart_enabled,
    should_enable_autostart_on_first_start,
};
use gtk::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const STATUS_POLL_INTERVAL_SECS: u32 = 1;

fn status_text(running: bool, autostart: bool) -> &'static str {
    match (running, autostart) {
        (true, _) => "Running",
        // Short enough to keep the title centered in the header.
        (false, true) => "Stopped, starts at login",
        (false, false) => "Stopped",
    }
}

/// Opening the app starts gestures, unless the user is the one who stopped them.
fn should_start_on_open(running: bool, stopped_by_user: bool) -> bool {
    !running && !stopped_by_user
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
/// running, a Start or Stop button, an area for errors in the window's content, and toasts for
/// notices. Daemon calls can block, so they all run off the UI thread.
#[derive(Clone)]
pub struct GestureStatus {
    title: gtk::Box,
    status_label: gtk::Label,
    action_button: gtk::Button,
    error_box: gtk::Box,
    error_summary: gtk::Label,
    error_details: gtk::Expander,
    error_label: gtk::Label,
    toasts: adw::ToastOverlay,
    login_toast: Rc<RefCell<Option<adw::Toast>>>,
    dbus_conn: Option<zbus::blocking::Connection>,
    busy: Rc<Cell<bool>>,
    polling: Rc<Cell<bool>>,
    last_running: Rc<Cell<Option<bool>>>,
    just_opened: Rc<Cell<bool>>,
}

impl GestureStatus {
    pub fn new(
        title_text: &str,
        dbus_conn: Option<zbus::blocking::Connection>,
        toasts: &adw::ToastOverlay,
    ) -> Self {
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
        error_box.set_margin_bottom(12);
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

        let status = GestureStatus {
            title,
            status_label,
            action_button,
            error_box,
            error_summary,
            error_details,
            error_label,
            toasts: toasts.clone(),
            login_toast: Rc::new(RefCell::new(None)),
            dbus_conn,
            busy: Rc::new(Cell::new(false)),
            polling: Rc::new(Cell::new(false)),
            last_running: Rc::new(Cell::new(None)),
            just_opened: Rc::new(Cell::new(true)),
        };

        let clicked = status.clone();
        status.action_button.connect_clicked(move |_| {
            let start = clicked.last_running.get() != Some(true);
            clicked.set_gestures(start);
        });

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

    /// A new user's first start also turns on login startup, with a way to undo it. Either
    /// way, it's only decided for them once.
    fn enable_autostart_for_new_user(&self) {
        let result = mark_autostart_decided().and_then(|_| set_autostart_enabled(true));
        match result {
            Ok(()) => self.show_login_toast(),
            Err(e) => self.show_error(Some(&format!("Could not change the login setting: {e}"))),
        }
    }

    /// Stays until it's dismissed, as it's the only place that says so.
    fn show_login_toast(&self) {
        let toast = adw::Toast::new("Gestures will also start when you log in");
        toast.set_button_label(Some("_Undo"));
        toast.set_timeout(0);
        let undone = self.clone();
        toast.connect_button_clicked(move |_| {
            if let Err(e) = set_autostart_enabled(false) {
                undone.show_error(Some(&format!("Could not change the login setting: {e}")));
            }
            undone.render();
        });
        self.toasts.add_toast(toast.clone());
        self.login_toast.replace(Some(toast));
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
                if status.just_opened.replace(false)
                    && should_start_on_open(running, is_stopped_by_user())
                {
                    status.set_gestures(true);
                    return;
                }
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
        if let Some(toast) = self.login_toast.take() {
            toast.dismiss();
        }
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
    fn opening_starts_gestures_unless_the_user_stopped_them() {
        assert!(should_start_on_open(false, false));
        assert!(!should_start_on_open(false, true));
        assert!(!should_start_on_open(true, false));
        assert!(!should_start_on_open(true, true));
    }

    #[test]
    fn stopped_warns_only_when_gestures_will_return_at_login() {
        assert!(status_text(false, true).contains("login"));
        assert!(!status_text(false, false).contains("login"));
        assert_eq!(status_text(true, true), "Running");
        assert_eq!(status_text(true, false), "Running");
    }
}
