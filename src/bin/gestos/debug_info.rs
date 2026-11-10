//! Debugging information: what the user's setup is, what state each part of Gestos is in, and
//! what it logged lately, as plain text to attach to a bug report. What gestures type and the
//! commands they run are left out, as they can hold passwords and other private things.

use crate::extension;
use adw::prelude::*;
use gestos::config::{ActionType, Configuration};
use gestos::key_steps::KeyStep;
use gtk4::gio;
use gtk4::glib;
use std::fmt::Write;
use std::path::Path;
use std::process::Command;

// How many of the latest log lines to include from each part.
const LOG_LINES: usize = 200;
// GNOME Shell logs a lot about other things, so more of its lines are read to find Gestos's.
const SHELL_LOG_LINES: usize = 5000;

pub fn report() -> String {
    let mut out = String::new();
    let now = glib::DateTime::now_local()
        .ok()
        .and_then(|now| now.format("%F %T").ok())
        .map(|now| now.to_string())
        .unwrap_or_default();
    let _ = writeln!(out, "Gestos debugging information, {now}");

    section(&mut out, "Gestos");
    line(&mut out, "Version", env!("GESTOS_SHOWN_VERSION"));
    line(
        &mut out,
        "App",
        &std::env::current_exe().map(|exe| exe.display().to_string()).unwrap_or_default(),
    );
    line(&mut out, "Gestures", &daemon_state());
    line(
        &mut out,
        "Start when logging in",
        yes_no(crate::system::get_autostart_file_path().is_some_and(|path| path.exists())),
    );

    section(&mut out, "System");
    line(&mut out, "Distribution", &os_name());
    line(&mut out, "Kernel", read_trimmed("/proc/sys/kernel/osrelease").as_str());
    line(&mut out, "Desktop", &env("XDG_CURRENT_DESKTOP"));
    line(&mut out, "Session", &env("XDG_SESSION_TYPE"));
    let keyboard = match gestos::keymap::active_layout() {
        Some(name) => [name.layout, name.variant, name.options]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        None => "unknown".to_string(),
    };
    line(&mut out, "Keyboard", &keyboard);
    let app_in_use = if gestos::config::get_environment_suffix() == Some("gnome") {
        "from the GNOME extension"
    } else if gestos::active_app::is_kde() {
        "from KWin"
    } else if gestos::active_app::is_x11_session() {
        "from X11"
    } else {
        "unknown here"
    };
    line(&mut out, "App in use", app_in_use);
    if gestos::config::get_environment_suffix() == Some("gnome") {
        line(&mut out, "GNOME Shell", &command_output("gnome-shell", &["--version"]));
    }

    section(&mut out, "Input");
    line(
        &mut out,
        "Input service",
        &if gestos::input_protocol::uses_systemd() {
            command_output("systemctl", &["is-active", "gestos-input.socket"])
        } else {
            command_output("rc-service", &["gestos-input", "status"])
        },
    );
    line(&mut out, "Groups", &command_output("id", &["-Gn"]));
    line(&mut out, "Access rules", &access_rules());
    line(&mut out, "/dev/uinput", &device_access("/dev/uinput"));
    line(
        &mut out,
        "Left-handed",
        &command_output("gsettings", &["get", "org.gnome.desktop.peripherals.mouse", "left-handed"]),
    );
    for mouse in mice() {
        line(&mut out, "Mouse", &mouse);
    }

    if gestos::config::get_environment_suffix() == Some("gnome") {
        section(&mut out, "GNOME extension");
        let conn = zbus::blocking::Connection::session().ok();
        line(&mut out, "Status", &extension::status(conn.as_ref()).description());
        for copy in extension::installed_copies() {
            let version = extension_version(&copy);
            let current = if extension::copy_is_current(&copy) { "current" } else { "not current" };
            line(&mut out, "Copy", &format!("{} ({version}, {current})", copy.display()));
        }
        if let Some(settings) = extension::settings() {
            if let Some(schema) = settings.settings_schema() {
                for key in schema.list_keys() {
                    // Only settings, not what the extension keeps track of by itself.
                    if matches!(key.as_str(), "tiled-workspaces" | "closed-sizes" | "given-sizes") {
                        continue;
                    }
                    line(&mut out, &key, &settings.value(&key).print(false));
                }
            }
        }
    }

    let config_path = gestos::config::get_default_config_path();
    section(&mut out, "Settings");
    line(&mut out, "File", &config_path.display().to_string());
    let loads = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|content| serde_yaml::from_str::<gestos::config::YamlConfig>(&content).ok())
        .is_some();
    line(&mut out, "Loads", yes_no(loads));
    let button = Configuration::load_from_file(&config_path).and_then(|config| config.button);
    line(&mut out, "Button", &button.map_or("3, the default".to_string(), |b| b.to_string()));
    // The button and sensitivity are given to the daemon when it starts.
    let start_command = crate::system::get_autostart_file_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|entry| {
            entry.lines().find_map(|line| line.strip_prefix("Exec=")).map(str::to_string)
        });
    line(&mut out, "Started at login with", &start_command.unwrap_or_default());

    let gestures: Vec<_> = Configuration::load_from_defaults()
        .gestures
        .into_iter()
        .filter(|gesture| !gesture.is_deleted)
        .collect();
    section(&mut out, &format!("Gestures ({})", gestures.len()));
    for gesture in &gestures {
        let actions: Vec<String> = gesture.actions.iter().map(action_summary).collect();
        let mut summary = format!("{}: {}", gesture.name, actions.join(", "));
        if !gesture.modifiers.is_empty() {
            let _ = write!(summary, "; hold {}", gesture.modifiers);
        }
        if !gesture.apps.is_empty() {
            let _ = write!(summary, "; in {}", gesture.apps.join(", "));
        }
        let _ = writeln!(out, "- {summary}");
    }

    section(&mut out, "Gestures log");
    // Started at login it's logged as itself; started by the app, under its name.
    log(&mut out, &["--user", "_COMM=gestosd", "+", "SYSLOG_IDENTIFIER=gestosd"], LOG_LINES, None);
    if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(|dir| Path::new(&dir).join("gestosd-start.log"))
        .filter(|path| path.exists())
    {
        section(&mut out, "Last start from the app");
        let _ = writeln!(out, "{}", std::fs::read_to_string(path).unwrap_or_default().trim());
    }
    section(&mut out, "Input service log");
    log(&mut out, &["_COMM=gestos-input"], LOG_LINES, None);
    if gestos::config::get_environment_suffix() == Some("gnome") {
        section(&mut out, "GNOME Shell log, about Gestos");
        log(&mut out, &["--user", "_COMM=gnome-shell"], SHELL_LOG_LINES, Some("gestos"));
    }
    out
}

fn section(out: &mut String, title: &str) {
    let _ = writeln!(out, "\n== {title} ==");
}

fn line(out: &mut String, name: &str, value: &str) {
    let value = if value.is_empty() { "unknown" } else { value };
    let _ = writeln!(out, "{name}: {value}");
}

fn yes_no(yes: bool) -> &'static str {
    if yes {
        "yes"
    } else {
        "no"
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn read_trimmed(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

/// What a command printed, or why it couldn't run.
fn command_output(program: &str, args: &[&str]) -> String {
    match Command::new(program).args(args).output() {
        Ok(output) => {
            let text = if output.stdout.is_empty() { output.stderr } else { output.stdout };
            String::from_utf8_lossy(&text).trim().to_string()
        }
        Err(e) => format!("could not run {program}: {e}"),
    }
}

fn os_name() -> String {
    read_trimmed("/etc/os-release")
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|name| name.trim_matches('"').to_string())
        .unwrap_or_default()
}

fn daemon_state() -> String {
    let conn = zbus::blocking::Connection::session().ok();
    let stopped = if crate::system::is_stopped_by_user() { ", turned off by the user" } else { "" };
    if !crate::daemon::is_daemon_running(conn.as_ref()) {
        return format!("not running{stopped}");
    }
    // Asked only once it's known to run, so that asking doesn't start it.
    let version = conn
        .and_then(|conn| {
            zbus::blocking::Proxy::new(
                &conn,
                gestos::config::get_dbus_name(),
                "/io/github/deters/Gestos/Daemon",
                "io.github.deters.Gestos.Daemon",
            )
            .ok()?
            .get_property::<String>("Version")
            .ok()
        })
        .unwrap_or_default();
    format!("running, version {version}{stopped}")
}

fn access_rules() -> String {
    let rules: Vec<String> = ["/etc/udev/rules.d", "/usr/lib/udev/rules.d", "/lib/udev/rules.d"]
        .iter()
        .map(|dir| Path::new(dir).join("99-gestos.rules"))
        .filter(|path| path.exists())
        .map(|path| path.display().to_string())
        .collect();
    if rules.is_empty() {
        "not installed".to_string()
    } else {
        rules.join(", ")
    }
}

fn device_access(path: &str) -> String {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    match std::fs::metadata(path) {
        Ok(meta) => {
            let group = command_output("getent", &["group", &meta.gid().to_string()]);
            let group = group.split(':').next().unwrap_or_default().to_string();
            format!("mode {:o}, group {group}", meta.permissions().mode() & 0o777)
        }
        Err(e) => e.to_string(),
    }
}

/// The input devices the kernel lists as mice.
fn mice() -> Vec<String> {
    read_trimmed("/proc/bus/input/devices")
        .split("\n\n")
        .filter(|device| device.lines().any(|line| line.starts_with("H:") && line.contains("mouse")))
        .filter_map(|device| {
            let name = device.lines().find_map(|line| line.strip_prefix("N: Name="))?;
            let handlers = device.lines().find_map(|line| line.strip_prefix("H: Handlers="))?;
            Some(format!("{} ({})", name.trim_matches('"'), handlers.trim()))
        })
        .collect()
}

fn extension_version(copy: &Path) -> String {
    std::fs::read_to_string(copy.join("metadata.json"))
        .unwrap_or_default()
        .lines()
        .find(|line| line.contains("\"version-name\""))
        .and_then(|line| line.split('"').nth(3))
        .unwrap_or("no version")
        .to_string()
}

/// What an action does, without the text it types or the command it runs.
fn action_summary(action: &ActionType) -> String {
    match action {
        ActionType::Execute(_) => "run a command (left out)".to_string(),
        ActionType::Keyboard(steps) => {
            let steps: Vec<String> = steps
                .iter()
                .map(|step| match step {
                    KeyStep::Press(keys) => format!("press {keys}"),
                    KeyStep::Type(_) => "type (left out)".to_string(),
                    KeyStep::Wait(ms) => format!("wait {ms}"),
                    KeyStep::Open(app) => format!("open {app}"),
                })
                .collect();
            format!("keyboard [{}]", steps.join("; "))
        }
        other => format!("{other:?}"),
    }
}

/// The latest lines of this boot's journal that match, without the computer's name, which every
/// line repeats. With a filter, only the lines that mention it.
fn log(out: &mut String, matches: &[&str], lines: usize, filter: Option<&str>) {
    let count = lines.to_string();
    let mut args = vec!["-b", "--no-pager", "-o", "short-iso", "-n", &count];
    args.extend_from_slice(matches);
    let output = match Command::new("journalctl").args(&args).output() {
        Ok(output) => output,
        Err(e) => {
            let _ = writeln!(out, "Could not read the log: {e}");
            return;
        }
    };
    let host = format!(" {} ", glib::host_name());
    let text = String::from_utf8_lossy(&output.stdout);
    // An entry can go on over several indented lines, like an error with its stack trace, and
    // is kept whole if any of them matches.
    let mut entries: Vec<Vec<&str>> = Vec::new();
    for text_line in text.lines() {
        match entries.last_mut() {
            Some(entry) if text_line.starts_with(char::is_whitespace) => entry.push(text_line),
            _ => entries.push(vec![text_line]),
        }
    }
    let mut shown = 0;
    for entry in entries {
        let matches = filter.is_none_or(|word| {
            entry.iter().any(|text_line| text_line.to_lowercase().contains(word))
        });
        if !matches {
            continue;
        }
        let _ = writeln!(out, "{}", entry[0].replacen(&host, " ", 1));
        for text_line in &entry[1..] {
            let _ = writeln!(out, "{text_line}");
        }
        shown += 1;
    }
    if shown == 0 {
        let error = String::from_utf8_lossy(&output.stderr);
        let _ = writeln!(out, "Nothing. {}", error.trim());
    }
}

/// Writes the report to a file and opens it in the user's text editor, to read or attach.
pub fn open(parent: &impl IsA<gtk4::Window>) {
    let path = glib::user_cache_dir().join("gestos").join("gestos-debug.txt");
    let text = report();
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| std::fs::write(&path, &text));
    if let Err(e) = written {
        crate::ui::components::show_error_dialog(
            parent,
            &format!("Could not save the debugging information: {e}"),
        );
        return;
    }
    let opened = gio::AppInfo::launch_default_for_uri(
        &gio::File::for_path(&path).uri(),
        gio::AppLaunchContext::NONE,
    );
    // With no app for text files, as on a bare system, say where it is and let it be copied.
    if opened.is_err() {
        show_saved(parent, &path, text);
    }
}

fn show_saved(parent: &impl IsA<gtk4::Window>, path: &Path, text: String) {
    let dialog = adw::MessageDialog::new(
        Some(parent),
        Some("Report Saved"),
        Some(&path.display().to_string()),
    );
    dialog.add_response("close", "Close");
    dialog.add_response("copy", "Copy Report");
    dialog.set_response_appearance("copy", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("copy"));
    dialog.set_close_response("close");
    dialog.connect_response(Some("copy"), move |dialog, _| {
        dialog.clipboard().set_text(&text);
    });
    dialog.present();
}
