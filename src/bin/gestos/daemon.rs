use gtk::prelude::*;
use crate::system::{set_stopped_by_user, start_overlay_if_enabled, stop_overlay};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use crate::ui::components::show_error_dialog;
use gtk4 as gtk;

pub fn is_daemon_running(conn: Option<&zbus::blocking::Connection>) -> bool {
    let dbus_name = gestos::config::get_dbus_name();

    let check_status = |c: &zbus::blocking::Connection| -> Option<bool> {
        let dbus_proxy = zbus::blocking::fdo::DBusProxy::new(c).ok()?;
        let bus_name = zbus::names::BusName::try_from(dbus_name.clone()).ok()?;
        dbus_proxy.name_has_owner(bus_name).ok()
    };

    if let Some(c) = conn {
        if let Some(running) = check_status(c) {
            return running;
        }
    }

    if let Ok(new_conn) = zbus::blocking::Connection::session() {
        if let Some(running) = check_status(&new_conn) {
            return running;
        }
    }
    false
}
/// The daemon installed beside this app, so that nothing in the current folder can be run
/// by accident.
fn daemon_binary() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("gestosd")))
        .filter(|path| path.exists())
        .unwrap_or_else(|| "gestosd".into())
}

fn start_log_path() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| std::path::PathBuf::from(dir).join("gestosd-start.log"))
}

/// Makes the daemon's startup output readable, dropping the banner lines around its errors.
fn clean_start_error(output: &str) -> String {
    output
        .lines()
        .filter(|line| !line.starts_with("====="))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn start_daemon(conn: Option<&zbus::blocking::Connection>) -> Result<(), String> {
    if is_daemon_running(conn) {
        return Ok(());
    }

    // The daemon outlives this app, so its output can't go to a pipe that closes with the app:
    // writing to it would make the daemon panic.
    let log_path = start_log_path();
    let stderr = log_path
        .as_ref()
        .and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .mode(0o600)
                .open(path)
                .ok()
        })
        .map_or_else(Stdio::null, Stdio::from);

    let mut child = Command::new(daemon_binary())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .process_group(0)
        .spawn()
        .map_err(|e| format!("Failed to start the daemon: {}", e))?;

    // Wait a short duration to see if the process exits immediately
    std::thread::sleep(std::time::Duration::from_millis(300));

    match child.try_wait() {
        Ok(Some(status)) => {
            let output = log_path
                .and_then(|path| std::fs::read_to_string(path).ok())
                .unwrap_or_default();
            let message = clean_start_error(&output);
            if message.is_empty() {
                Err(format!("The daemon exited immediately with {}", status))
            } else {
                Err(message)
            }
        }
        Ok(None) => {
            // Collect its exit status later, so that it doesn't linger as a zombie.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }
        Err(e) => Err(format!("Failed to query the daemon's status: {}", e)),
    }
}

pub fn stop_daemon(conn: Option<&zbus::blocking::Connection>) {
    // The name is D-Bus activatable, so calling Stop with nothing running would start a daemon.
    if is_daemon_running(conn) {
        let dbus_name = gestos::config::get_dbus_name();
        let stop_via_dbus = |c: &zbus::blocking::Connection| -> zbus::Result<()> {
            let proxy = zbus::blocking::Proxy::new(
                c,
                dbus_name.clone(),
                "/io/github/deters/Gestos/Daemon",
                "io.github.deters.Gestos.Daemon",
            )?;
            let _: () = proxy.call("Stop", &())?;
            Ok(())
        };
        // The daemon exits without replying, so an error here is the expected result.
        match conn {
            Some(c) => {
                let _ = stop_via_dbus(c);
            }
            None => {
                if let Ok(new_conn) = zbus::blocking::Connection::session() {
                    let _ = stop_via_dbus(&new_conn);
                }
            }
        }
        for _ in 0..50 {
            if !is_daemon_running(conn) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }

    // Not reachable over D-Bus, or not responding.
    let uid = nix::unistd::Uid::current();
    if let Ok(output) = Command::new("pgrep")
        .arg("-u")
        .arg(uid.to_string())
        .arg("-x")
        .arg("gestosd")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Ok(pid_val) = line.trim().parse::<i32>() {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(pid_val),
                        nix::sys::signal::Signal::SIGTERM,
                    );
                }
            }
        }
    }
}

/// Starts gestures, including the overlay if the user has it enabled.
pub fn turn_on(conn: Option<&zbus::blocking::Connection>) -> Result<(), String> {
    set_stopped_by_user(false);
    gestos::config::initialize_user_config_if_missing()
        .map_err(|e| format!("Could not create the configuration: {}", e))?;
    start_daemon(conn)?;
    start_overlay_if_enabled();
    Ok(())
}

/// Stops gestures completely, including the overlay.
pub fn turn_off(conn: Option<&zbus::blocking::Connection>) {
    set_stopped_by_user(true);
    stop_daemon(conn);
    stop_overlay();
}

fn send_notification(summary: &str, body: &str) {
    let Ok(conn) = zbus::blocking::Connection::session() else {
        return;
    };
    let _ = conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            "Gestos",
            0u32,
            "gestos",
            summary,
            body,
            Vec::<&str>::new(),
            std::collections::HashMap::<&str, zbus::zvariant::Value>::new(),
            -1i32,
        ),
    );
}

/// Handles `gestos --start` and `gestos --stop`, which also run from the app icon's menu. Only a
/// failure is worth a notification there. Returns the process exit code.
pub fn run_command(enable: bool) -> i32 {
    let conn = zbus::blocking::Connection::session().ok();
    if enable {
        match turn_on(conn.as_ref()) {
            Ok(()) => 0,
            Err(message) => {
                eprintln!("{}", message);
                send_notification(
                    "Could not start gestures",
                    "Open Gestos and start them there for details.",
                );
                1
            }
        }
    } else {
        turn_off(conn.as_ref());
        0
    }
}

pub fn reload_daemon<W: IsA<gtk::Window>>(
    conn: Option<&zbus::blocking::Connection>,
    parent: Option<&W>,
) {
    // The name is D-Bus activatable, so reloading with gestures stopped would start them.
    if !is_daemon_running(conn) {
        return;
    }

    let dbus_name = gestos::config::get_dbus_name();
    let reload_via_dbus = || -> zbus::Result<()> {
        let proxy = if let Some(c) = conn {
            zbus::blocking::Proxy::new(
                c,
                dbus_name.clone(),
                "/io/github/deters/Gestos/Daemon",
                "io.github.deters.Gestos.Daemon",
            )
        } else {
            let new_conn = zbus::blocking::Connection::session()?;
            zbus::blocking::Proxy::new(
                &new_conn,
                dbus_name.clone(),
                "/io/github/deters/Gestos/Daemon",
                "io.github.deters.Gestos.Daemon",
            )
        }?;
        let _: () = proxy.call("Reload", &())?;
        Ok(())
    };

    if let Err(zbus::Error::FDO(ref fdo_err)) = reload_via_dbus() {
        if let Some(p) = parent {
            show_error_dialog(p, &fdo_err.to_string());
        } else {
            eprintln!("gestos reload error: {}", fdo_err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_errors_lose_their_banner_lines() {
        let output = "\n=====\nERROR: Missing permissions\n - Cannot read device\n=====\n\n";
        assert_eq!(
            clean_start_error(output),
            "ERROR: Missing permissions\n - Cannot read device"
        );
        assert_eq!(clean_start_error("=====\n"), "");
    }
}
