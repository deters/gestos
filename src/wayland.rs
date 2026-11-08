use crate::chooser::ChooserEntry;
use crate::config::ActionType;
use gio::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

// How long GNOME gets to pick up a changed shortcut before it's used. The action can't start
// until this is over, so it's as short as seems safe.
const TEMPORARY_SHORTCUT_PICKUP_DELAY_MILLIS: u64 = 100;
// How long GNOME gets to act on the shortcut before it's restored. Nothing waits for this.
const TEMPORARY_SHORTCUT_RESTORE_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

const GNOME_SHELL_NAME: &str = "org.gnome.Shell";
const GESTOS_EXTENSION_PATH: &str = "/io/github/deters/Gestos/Extension";
const GESTOS_EXTENSION_INTERFACE: &str = "io.github.deters.Gestos.Extension";
const NOTIFICATIONS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

fn call_extension(conn: &zbus::blocking::Connection, shell: &str, method: &str) -> zbus::Result<()> {
    conn.call_method(
        Some(shell),
        GESTOS_EXTENSION_PATH,
        Some(GESTOS_EXTENSION_INTERFACE),
        method,
        &(),
    )?;
    Ok(())
}

/// The app of the focused window, from the GNOME extension, by its desktop file ID. `None` when no
/// window is focused, or the extension can't say, such as when it isn't running.
pub fn focused_app_in_gnome_extension() -> Option<String> {
    let conn = zbus::blocking::Connection::session().ok()?;
    focused_app_from(&conn, GNOME_SHELL_NAME)
}

fn focused_app_from(conn: &zbus::blocking::Connection, shell: &str) -> Option<String> {
    let reply = conn
        .call_method(
            Some(shell),
            GESTOS_EXTENSION_PATH,
            Some(GESTOS_EXTENSION_INTERFACE),
            "FocusedApp",
            &(),
        )
        .ok()?;
    let app: String = reply.body().deserialize().ok()?;
    (!app.is_empty()).then_some(app)
}

// How long an app gets to come to the front before the steps after it are left out, and how often
// that's checked.
const OPEN_APP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const OPEN_APP_POLL: std::time::Duration = std::time::Duration::from_millis(100);
// Where nothing can say which app has focus, how long an app gets to open.
const OPEN_APP_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Opens an app, or brings its window to the front if it's open, for a custom action's next
/// steps, and says whether it's ready for them. On GNOME the extension opens it, as the Shell
/// lets nothing else bring a window to the front, and it's ready once it has focus. If it doesn't
/// get it, a notification says the rest didn't run. Elsewhere it's started, and given a moment.
pub fn open_app_for_steps(app: &str, is_gnome: bool) -> bool {
    if is_gnome {
        if let Ok(conn) = zbus::blocking::Connection::session() {
            if let Ok(found) = open_app_in(&conn, GNOME_SHELL_NAME, app) {
                if !found {
                    let name = crate::key_steps::app_name(app);
                    let _ = notify(
                        &conn,
                        NOTIFICATIONS_NAME,
                        &format!("{name} isn\u{2019}t installed"),
                        "The rest of the gesture didn\u{2019}t run.",
                    );
                    return false;
                }
                return wait_for_focus(&conn, GNOME_SHELL_NAME, NOTIFICATIONS_NAME, app);
            }
        }
    }
    let launched = gio::DesktopAppInfo::new(&format!("{app}.desktop"))
        .is_some_and(|info| info.launch(&[], None::<&gio::AppLaunchContext>).is_ok());
    if launched {
        std::thread::sleep(OPEN_APP_WAIT);
    }
    launched
}

fn open_app_in(conn: &zbus::blocking::Connection, shell: &str, app: &str) -> zbus::Result<bool> {
    let reply = conn.call_method(
        Some(shell),
        GESTOS_EXTENSION_PATH,
        Some(GESTOS_EXTENSION_INTERFACE),
        "OpenApp",
        &(app,),
    )?;
    reply.body().deserialize()
}

fn wait_for_focus(
    conn: &zbus::blocking::Connection,
    shell: &str,
    notifications: &str,
    app: &str,
) -> bool {
    let deadline = std::time::Instant::now() + OPEN_APP_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if focused_app_from(conn, shell).as_deref() == Some(app) {
            return true;
        }
        std::thread::sleep(OPEN_APP_POLL);
    }
    let name = crate::key_steps::app_name(app);
    let _ = notify(
        conn,
        notifications,
        &format!("{name} didn\u{2019}t come to the front"),
        "The rest of the gesture didn\u{2019}t run.",
    );
    false
}

/// Asks the GNOME extension which of several gestures was meant, on a card the Shell draws at the
/// pointer. `None` means the user cancelled. An error means the extension can't ask, such as when
/// it isn't running, or is a version from before it could.
pub fn choose_in_gnome_extension(entries: &[ChooserEntry]) -> zbus::Result<Option<usize>> {
    let conn = zbus::blocking::Connection::session()?;
    choose_in_extension(&conn, GNOME_SHELL_NAME, entries)
}

fn choose_in_extension(
    conn: &zbus::blocking::Connection,
    shell: &str,
    entries: &[ChooserEntry],
) -> zbus::Result<Option<usize>> {
    // Each gesture's points go as one list, x and y in turn.
    let gestures: Vec<(&str, &str, Vec<f64>)> = entries
        .iter()
        .map(|entry| {
            let points = entry.points.iter().flat_map(|p| [p.x, p.y]).collect();
            (entry.name.as_str(), entry.description.as_str(), points)
        })
        .collect();
    let reply = conn.call_method(
        Some(shell),
        GESTOS_EXTENSION_PATH,
        Some(GESTOS_EXTENSION_INTERFACE),
        "ChooseGesture",
        &(gestures,),
    )?;
    let choice: i32 = reply.body().deserialize()?;
    Ok(usize::try_from(choice).ok().filter(|&choice| choice < entries.len()))
}

// Whether this session was told the extension running is out of date. It stays out of date until
// the next login, so saying it once is enough.
static TOLD_OUTDATED: AtomicBool = AtomicBool::new(false);

/// Runs a window action in the GNOME Shell extension. The extension stays as the user left it, so
/// when it isn't running, a notification says so instead of doing nothing. An old copy can lack
/// the action, or run it the way an older Gestos did, so a notification says that once a session.
fn run_extension_action(
    conn: &zbus::blocking::Connection,
    shell: &str,
    notifications: &str,
    method: &str,
) -> zbus::Result<()> {
    run_extension_action_telling(conn, shell, notifications, method, &TOLD_OUTDATED)
}

fn run_extension_action_telling(
    conn: &zbus::blocking::Connection,
    shell: &str,
    notifications: &str,
    method: &str,
    told_outdated: &AtomicBool,
) -> zbus::Result<()> {
    let ran = call_extension(conn, shell, method).is_ok();
    let outdated = crate::gnome_extension::info_from(conn, shell).is_ok_and(|info| {
        crate::gnome_extension::is_running(&info) && !crate::gnome_extension::is_current(&info)
    });
    if outdated {
        if !told_outdated.swap(true, Ordering::SeqCst) {
            notify(
                conn,
                notifications,
                "The \u{201c}Gestos\u{201d} GNOME extension is out of date",
                "Open Gestos to see how to update it.",
            )?;
        }
        return Ok(());
    }
    if ran {
        return Ok(());
    }
    notify(
        conn,
        notifications,
        "This action needs the \u{201c}Gestos\u{201d} GNOME extension",
        "Turn it on in Gestos Preferences.",
    )
}

fn notify(
    conn: &zbus::blocking::Connection,
    notifications: &str,
    summary: &str,
    body: &str,
) -> zbus::Result<()> {
    conn.call_method(
        Some(notifications),
        NOTIFICATIONS_PATH,
        Some(NOTIFICATIONS_NAME),
        "Notify",
        &(
            "Gestos",
            0u32,
            "dialog-warning-symbolic",
            summary,
            body,
            Vec::<&str>::new(),
            HashMap::<&str, zbus::zvariant::Value>::new(),
            -1i32,
        ),
    )?;
    Ok(())
}

/// Experimental: GESTOS_SHORTCUT_DELAY_MS overrides the wait, to find what a desktop needs.
fn temporary_shortcut_pickup_delay(setting: Option<&str>) -> std::time::Duration {
    let millis = setting
        .and_then(|value| value.parse().ok())
        .unwrap_or(TEMPORARY_SHORTCUT_PICKUP_DELAY_MILLIS);
    std::time::Duration::from_millis(millis)
}

type GnomeSchema = (gio::Settings, gio::SettingsSchema);
type GnomeSettingRequest = (String, String, Sender<Option<String>>);

/// Without its dconf module GIO quietly reads built-in defaults instead of the user's settings.
pub fn gio_reads_user_settings() -> bool {
    static USES_DCONF: OnceLock<bool> = OnceLock::new();
    *USES_DCONF.get_or_init(|| {
        gio::SettingsBackend::default().type_().name() == "DConfSettingsBackend"
    })
}

/// Answers requests on one thread, because actions run on threads of their own and GIO's objects
/// can't be shared between them. It keeps schemas open, since opening one is the slow part.
/// Values aren't kept: reading one is quick and always sees a shortcut the user just changed.
fn gnome_settings_thread() -> Sender<GnomeSettingRequest> {
    let (sender, requests) = channel::<GnomeSettingRequest>();
    std::thread::spawn(move || {
        let mut schemas: HashMap<String, Option<GnomeSchema>> = HashMap::new();
        for (schema_id, key, reply) in requests {
            let opened = schemas.entry(schema_id).or_insert_with_key(|schema_id| {
                // Opening a schema that isn't installed would abort the whole program.
                let schema = gio::SettingsSchemaSource::default()?.lookup(schema_id, true)?;
                Some((gio::Settings::new(schema_id), schema))
            });
            let value = opened.as_ref().and_then(|(settings, schema)| {
                schema
                    .has_key(&key)
                    .then(|| settings.value(&key).print(true).to_string())
            });
            let _ = reply.send(value);
        }
    });
    sender
}

/// A setting as text in gsettings' format, or nothing if this schema doesn't have the key.
pub fn read_gnome_setting(schema_id: &str, key: &str) -> Option<String> {
    static REQUESTS: OnceLock<Sender<GnomeSettingRequest>> = OnceLock::new();
    let (reply, answer) = channel();
    REQUESTS
        .get_or_init(gnome_settings_thread)
        .send((schema_id.to_string(), key.to_string(), reply))
        .ok()?;
    answer.recv().ok()?
}

/// The first shortcut in a gsettings value like `['<Alt>F10', 'XF86Foo']`, as keys to press. An
/// unassigned one reads `['']` or `@as []`.
fn parse_gnome_shortcut(raw: &str) -> Option<String> {
    let shortcut = raw.split('\'').skip(1).step_by(2).find(|entry| !entry.is_empty())?;
    if shortcut == "disabled" {
        return None;
    }

    let mut keys = String::new();
    let mut rest = shortcut;
    while let Some(after_bracket) = rest.strip_prefix('<') {
        let (modifier, after) = after_bracket.split_once('>')?;
        keys.push_str(match modifier {
            "Alt" => "Alt_L+",
            "Super" => "Super_L+",
            "Shift" => "Shift_L+",
            "Control" | "Ctrl" | "Primary" => "Control_L+",
            _ => return None,
        });
        rest = after;
    }
    keys.push_str(rest);
    Some(keys)
}

/// Whether a program can be found in PATH.
fn is_installed(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(dir.join(program))
                .is_ok_and(|file| file.is_file() && file.permissions().mode() & 0o111 != 0)
        })
    })
}

/// The first of these commands whose program passes the check, as its program and arguments.
fn first_installed<'a>(
    commands: &[&'a str],
    installed: impl Fn(&str) -> bool,
) -> Option<Vec<&'a str>> {
    commands
        .iter()
        .map(|command| command.split_whitespace().collect::<Vec<_>>())
        .find(|parts| parts.first().is_some_and(|program| installed(program)))
}

/// On its own thread, so that a busy Shell can't hold up the next gesture.
fn spawn_extension_action(method: &'static str) {
    std::thread::spawn(move || {
        let result = zbus::blocking::Connection::session().and_then(|conn| {
            run_extension_action(&conn, GNOME_SHELL_NAME, NOTIFICATIONS_NAME, method)
        });
        if let Err(e) = result {
            log::warn!("Failed to run {}: {}", method, e);
        }
    });
}

#[derive(Debug, Clone, Default)]
pub struct WaylandContext {
    pub is_gnome: bool,
    pub is_kde: bool,
    pub is_sway: bool,
    pub is_hypr: bool,
    pub is_xfce: bool,
    pub sway_sock: String,
    pub hypr_sig: String,
    pub uid: u32,
}

impl WaylandContext {
    pub fn discover() -> Self {
        let mut ctx = WaylandContext::default();

        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        if desktop.contains("GNOME") || desktop.contains("gnome") || desktop.contains("Ubuntu") {
            ctx.is_gnome = true;
        } else if desktop.contains("KDE") || desktop.contains("kde") {
            ctx.is_kde = true;
        } else if desktop.contains("XFCE") || desktop.contains("xfce") {
            ctx.is_xfce = true;
        }

        if std::env::var("SWAYSOCK").is_ok() {
            ctx.is_sway = true;
            ctx.sway_sock = std::env::var("SWAYSOCK").unwrap();
        } else if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            ctx.is_hypr = true;
            ctx.hypr_sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap();
        }

        // Fallback checks using pgrep if env variables are missing
        if !ctx.is_gnome && !ctx.is_kde && !ctx.is_xfce {
            if Command::new("pgrep")
                .arg("-x")
                .arg("gnome-shell")
                .stdout(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                ctx.is_gnome = true;
            } else if Command::new("pgrep")
                .arg("-x")
                .arg("kwin_wayland")
                .stdout(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                ctx.is_kde = true;
            } else if Command::new("pgrep")
                .arg("-x")
                .arg("xfwm4")
                .stdout(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                ctx.is_xfce = true;
            }
        }

        ctx.uid = unsafe { libc::getuid() };

        // Find Sway socket or Hyprland signature if not already found in env
        if ctx.uid > 0 {
            let user_run_dir = PathBuf::from(format!("/run/user/{}", ctx.uid));
            if !ctx.is_sway && user_run_dir.exists() {
                if let Ok(entries) = fs::read_dir(&user_run_dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.contains("sway-ipc") && name.contains(".sock") {
                            ctx.sway_sock = user_run_dir.join(name).to_string_lossy().into_owned();
                            ctx.is_sway = true;
                            break;
                        }
                    }
                }
            }

            if !ctx.is_sway && !ctx.is_hypr {
                let hypr_dir = user_run_dir.join("hypr");
                if hypr_dir.exists() {
                    if let Ok(entries) = fs::read_dir(hypr_dir) {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().into_owned();
                            if name.len() >= 40 {
                                ctx.hypr_sig = name;
                                ctx.is_hypr = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        ctx
    }

    pub fn execute_action(&self, action: &ActionType, uinput_handler: &dyn Fn(&str)) {
        match action {
            ActionType::Execute(cmd) => {
                // Spawn the command asynchronously in shell
                let self_clone = self.clone();
                let cmd_clone = cmd.clone();
                std::thread::spawn(move || {
                    self_clone.run_shell_cmd(&cmd_clone);
                });
            }
            ActionType::Keypress(keys) => {
                uinput_handler(keys);
            }
            _ => {
                if self.is_sway {
                    self.execute_sway(action, uinput_handler);
                } else if self.is_hypr {
                    self.execute_hypr(action, uinput_handler);
                } else if self.is_gnome {
                    self.execute_gnome(action, uinput_handler);
                } else if self.is_kde {
                    self.execute_kde(action, uinput_handler);
                } else if self.is_xfce {
                    self.execute_xfce(action, uinput_handler);
                } else {
                    log::warn!("No active desktop compositor detected. Action ignored.");
                }
            }
        }
    }

    fn execute_sway(&self, action: &ActionType, uinput_handler: &dyn Fn(&str)) {
        match action {
            ActionType::Iconify => self.run_swaymsg("move scratchpad"),
            ActionType::Kill => self.run_swaymsg("kill"),
            ActionType::Maximize => self.run_swaymsg("fullscreen enable"),
            ActionType::Restore => self.run_swaymsg("fullscreen disable"),
            ActionType::ToggleMaximized | ActionType::ToggleFullscreen => {
                self.run_swaymsg("fullscreen toggle")
            }
            ActionType::WorkspaceLeft => self.run_swaymsg("workspace prev_on_output"),
            ActionType::WorkspaceRight => self.run_swaymsg("workspace next_on_output"),
            ActionType::WorkspaceUp => self.run_swaymsg("workspace prev"),
            ActionType::WorkspaceDown => self.run_swaymsg("workspace next"),
            ActionType::ShowOverview => uinput_handler("Super_L"),
            ActionType::ShowAppGrid => uinput_handler("Super_L+d"),
            ActionType::ShowDesktop => uinput_handler("Super_L+d"),
            ActionType::LockScreen => {
                self.run_user_cmd("swaylock", &[]);
            }
            ActionType::Terminal => self.open_terminal(),
            ActionType::VolumeUp => uinput_handler("XF86AudioRaiseVolume"),
            ActionType::VolumeDown => uinput_handler("XF86AudioLowerVolume"),
            ActionType::VolumeMute => uinput_handler("XF86AudioMute"),
            ActionType::MediaPlay => uinput_handler("XF86AudioPlay"),
            ActionType::MediaNext => uinput_handler("XF86AudioNext"),
            ActionType::MediaPrev => uinput_handler("XF86AudioPrev"),
            ActionType::Www => uinput_handler("XF86WWW"),
            ActionType::Home => uinput_handler("XF86Explorer"),
            ActionType::Email => uinput_handler("XF86Mail"),
            ActionType::Search => uinput_handler("XF86Search"),
            ActionType::Calculator => uinput_handler("XF86Calculator"),
            ActionType::ControlCenter => self.open_control_center(),
            ActionType::Logout => uinput_handler("Control_L+Alt_L+Delete"),
            ActionType::Screenshot => uinput_handler("Print"),
            ActionType::ScreenshotWindow => uinput_handler("Alt_L+Print"),
            ActionType::ScreenshotArea => uinput_handler("Shift_L+Print"),
            _ => {}
        }
    }

    fn execute_hypr(&self, action: &ActionType, uinput_handler: &dyn Fn(&str)) {
        match action {
            ActionType::Iconify => {
                self.run_hyprctl("dispatch movetoworkspacesilent special:minimized")
            }
            ActionType::Kill => self.run_hyprctl("dispatch killactive"),
            ActionType::Raise => self.run_hyprctl("dispatch alterzorder top"),
            ActionType::Lower => self.run_hyprctl("dispatch alterzorder bottom"),
            ActionType::Maximize | ActionType::Restore | ActionType::ToggleMaximized => {
                self.run_hyprctl("dispatch fullscreen 1")
            }
            ActionType::ToggleFullscreen => self.run_hyprctl("dispatch fullscreen 0"),
            ActionType::WorkspaceLeft => self.run_hyprctl("dispatch workspace e-1"),
            ActionType::WorkspaceRight => self.run_hyprctl("dispatch workspace e+1"),
            ActionType::WorkspaceUp => self.run_hyprctl("dispatch workspace m-1"),
            ActionType::WorkspaceDown => self.run_hyprctl("dispatch workspace m+1"),
            ActionType::ShowOverview => self.run_hyprctl("dispatch togglespecialworkspace"),
            ActionType::ShowAppGrid => uinput_handler("Super_L+a"),
            ActionType::ShowDesktop => uinput_handler("Super_L+d"),
            ActionType::LockScreen => {
                self.run_user_cmd("hyprlock", &[]);
            }
            ActionType::Terminal => self.open_terminal(),
            ActionType::VolumeUp => uinput_handler("XF86AudioRaiseVolume"),
            ActionType::VolumeDown => uinput_handler("XF86AudioLowerVolume"),
            ActionType::VolumeMute => uinput_handler("XF86AudioMute"),
            ActionType::MediaPlay => uinput_handler("XF86AudioPlay"),
            ActionType::MediaNext => uinput_handler("XF86AudioNext"),
            ActionType::MediaPrev => uinput_handler("XF86AudioPrev"),
            ActionType::Www => uinput_handler("XF86WWW"),
            ActionType::Home => uinput_handler("XF86Explorer"),
            ActionType::Email => uinput_handler("XF86Mail"),
            ActionType::Search => uinput_handler("XF86Search"),
            ActionType::Calculator => uinput_handler("XF86Calculator"),
            ActionType::ControlCenter => self.open_control_center(),
            ActionType::Logout => uinput_handler("Control_L+Alt_L+Delete"),
            ActionType::Screenshot => uinput_handler("Print"),
            ActionType::ScreenshotWindow => uinput_handler("Alt_L+Print"),
            ActionType::ScreenshotArea => uinput_handler("Shift_L+Print"),
            _ => {}
        }
    }

    fn execute_gnome(&self, action: &ActionType, uinput_handler: &dyn Fn(&str)) {
        match action {
            ActionType::Iconify => self.run_gnome_shortcut("minimize", "Super_L+h", uinput_handler),
            ActionType::Kill => self.close_gnome_window(uinput_handler),
            ActionType::Lower => uinput_handler("Alt_L+Escape"),
            ActionType::Maximize => {
                self.run_gnome_shortcut("maximize", "Super_L+Up", uinput_handler)
            }
            ActionType::Restore => {
                self.run_gnome_shortcut("unmaximize", "Super_L+Down", uinput_handler)
            }
            ActionType::ToggleMaximized => {
                self.run_gnome_shortcut("toggle-maximized", "Alt_L+F10", uinput_handler)
            }
            ActionType::WorkspaceLeft => self.run_gnome_shortcut(
                "switch-to-workspace-left",
                "Control_L+Alt_L+Left",
                uinput_handler,
            ),
            ActionType::WorkspaceRight => self.run_gnome_shortcut(
                "switch-to-workspace-right",
                "Control_L+Alt_L+Right",
                uinput_handler,
            ),
            ActionType::WorkspaceUp => self.run_gnome_shortcut(
                "switch-to-workspace-up",
                "Control_L+Alt_L+Up",
                uinput_handler,
            ),
            ActionType::WorkspaceDown => self.run_gnome_shortcut(
                "switch-to-workspace-down",
                "Control_L+Alt_L+Down",
                uinput_handler,
            ),
            ActionType::ShowOverview => {
                self.run_gnome_shortcut("toggle-overview", "Super_L", uinput_handler)
            }
            ActionType::ShowAppGrid => {
                self.run_gnome_shortcut("toggle-application-view", "Super_L+a", uinput_handler)
            }
            ActionType::ToggleFullscreen => {
                self.run_gnome_shortcut("toggle-fullscreen", "F11", uinput_handler)
            }
            ActionType::ShowDesktop => {
                self.run_gnome_shortcut("show-desktop", "Super_L+d", uinput_handler)
            }
            ActionType::LockScreen => {
                self.run_gnome_shortcut("screensaver", "Super_L+l", uinput_handler)
            }
            ActionType::ToggleTileWindows => spawn_extension_action("ToggleTileWindows"),
            ActionType::Terminal => self.open_terminal(),
            ActionType::VolumeUp => {
                self.run_gnome_shortcut("volume-up", "XF86AudioRaiseVolume", uinput_handler)
            }
            ActionType::VolumeDown => {
                self.run_gnome_shortcut("volume-down", "XF86AudioLowerVolume", uinput_handler)
            }
            ActionType::VolumeMute => {
                self.run_gnome_shortcut("volume-mute", "XF86AudioMute", uinput_handler)
            }
            ActionType::MediaPlay => {
                self.run_gnome_shortcut("play", "XF86AudioPlay", uinput_handler)
            }
            ActionType::MediaNext => {
                self.run_gnome_shortcut("next", "XF86AudioNext", uinput_handler)
            }
            ActionType::MediaPrev => {
                self.run_gnome_shortcut("previous", "XF86AudioPrev", uinput_handler)
            }
            ActionType::Www => self.run_gnome_shortcut("www", "XF86WWW", uinput_handler),
            ActionType::Home => self.run_gnome_shortcut("home", "XF86Explorer", uinput_handler),
            ActionType::Email => self.run_gnome_shortcut("email", "XF86Mail", uinput_handler),
            ActionType::Search => self.run_gnome_shortcut("search", "XF86Search", uinput_handler),
            ActionType::Calculator => {
                self.run_gnome_shortcut("calculator", "XF86Calculator", uinput_handler)
            }
            ActionType::ControlCenter => self.open_control_center(),
            ActionType::Logout => {
                self.run_gnome_shortcut("logout", "Control_L+Alt_L+Delete", uinput_handler)
            }
            ActionType::Screenshot => {
                self.run_gnome_shortcut("screenshot", "Print", uinput_handler)
            }
            ActionType::ScreenshotWindow => {
                self.run_gnome_shortcut("window-screenshot", "Alt_L+Print", uinput_handler)
            }
            ActionType::ScreenshotArea => {
                self.run_gnome_shortcut("area-screenshot", "Shift_L+Print", uinput_handler)
            }
            ActionType::Gnome(gkey) => {
                self.run_gnome_shortcut(gkey, "", uinput_handler);
            }
            _ => {}
        }
    }

    fn execute_kde(&self, action: &ActionType, uinput_handler: &dyn Fn(&str)) {
        match action {
            ActionType::Iconify => uinput_handler("Alt_L+F9"),
            ActionType::Kill => uinput_handler("Alt_L+F4"),
            ActionType::Lower => uinput_handler("Alt_L+Escape"),
            ActionType::Maximize | ActionType::ToggleMaximized => uinput_handler("Super_L+Page_Up"),
            ActionType::Restore => uinput_handler("Super_L+Page_Down"),
            ActionType::WorkspaceLeft => uinput_handler("Control_L+F1"),
            ActionType::WorkspaceRight => uinput_handler("Control_L+F2"),
            ActionType::WorkspaceUp => uinput_handler("Control_L+Alt_L+Up"),
            ActionType::WorkspaceDown => uinput_handler("Control_L+Alt_L+Down"),
            ActionType::ShowOverview => uinput_handler("Super_L+w"),
            ActionType::ShowAppGrid => uinput_handler("Alt_L+F1"),
            ActionType::ToggleFullscreen => uinput_handler("F11"),
            ActionType::ShowDesktop => uinput_handler("Super_L+d"),
            ActionType::LockScreen => uinput_handler("Super_L+l"),
            ActionType::Terminal => self.open_terminal(),
            ActionType::VolumeUp => uinput_handler("XF86AudioRaiseVolume"),
            ActionType::VolumeDown => uinput_handler("XF86AudioLowerVolume"),
            ActionType::VolumeMute => uinput_handler("XF86AudioMute"),
            ActionType::MediaPlay => uinput_handler("XF86AudioPlay"),
            ActionType::MediaNext => uinput_handler("XF86AudioNext"),
            ActionType::MediaPrev => uinput_handler("XF86AudioPrev"),
            ActionType::Www => uinput_handler("XF86WWW"),
            ActionType::Home => uinput_handler("XF86Explorer"),
            ActionType::Email => uinput_handler("XF86Mail"),
            ActionType::Search => uinput_handler("XF86Search"),
            ActionType::Calculator => uinput_handler("XF86Calculator"),
            ActionType::ControlCenter => self.open_control_center(),
            ActionType::Logout => uinput_handler("Control_L+Alt_L+Delete"),
            ActionType::Screenshot => uinput_handler("Print"),
            ActionType::ScreenshotWindow => uinput_handler("Alt_L+Print"),
            ActionType::ScreenshotArea => uinput_handler("Shift_L+Print"),
            ActionType::Kde(comp, short) => {
                self.run_user_cmd(
                    "dbus-send",
                    &[
                        "--session",
                        "--type=method_call",
                        "--dest=org.kde.kglobalaccel",
                        &format!("/component/{comp}"),
                        "org.kde.kglobalaccel.Component.invokeShortcut",
                        &format!("string:{short}"),
                    ],
                );
            }
            _ => {}
        }
    }

    fn execute_xfce(&self, action: &ActionType, uinput_handler: &dyn Fn(&str)) {
        match action {
            ActionType::Iconify => uinput_handler("Alt_L+F9"),
            ActionType::Kill => uinput_handler("Alt_L+F4"),
            ActionType::Lower => uinput_handler("Alt_L+Escape"),
            ActionType::Maximize | ActionType::ToggleMaximized => uinput_handler("Alt_L+F10"),
            ActionType::Restore => uinput_handler("Alt_L+F5"),
            ActionType::WorkspaceLeft => uinput_handler("Control_L+Alt_L+Left"),
            ActionType::WorkspaceRight => uinput_handler("Control_L+Alt_L+Right"),
            ActionType::WorkspaceUp => uinput_handler("Control_L+Alt_L+Up"),
            ActionType::WorkspaceDown => uinput_handler("Control_L+Alt_L+Down"),
            ActionType::ShowOverview => uinput_handler("Super_L"),
            ActionType::ShowAppGrid => uinput_handler("Super_L"),
            ActionType::ToggleFullscreen => uinput_handler("Alt_L+F11"),
            ActionType::ShowDesktop => uinput_handler("Control_L+Alt_L+d"),
            ActionType::LockScreen => {
                let self_clone = self.clone();
                std::thread::spawn(move || {
                    self_clone.run_user_cmd("xflock4", &[]);
                });
            }
            ActionType::Terminal => self.open_terminal(),
            ActionType::VolumeUp => uinput_handler("XF86AudioRaiseVolume"),
            ActionType::VolumeDown => uinput_handler("XF86AudioLowerVolume"),
            ActionType::VolumeMute => uinput_handler("XF86AudioMute"),
            ActionType::MediaPlay => uinput_handler("XF86AudioPlay"),
            ActionType::MediaNext => uinput_handler("XF86AudioNext"),
            ActionType::MediaPrev => uinput_handler("XF86AudioPrev"),
            ActionType::Www => uinput_handler("XF86WWW"),
            ActionType::Home => uinput_handler("XF86Explorer"),
            ActionType::Email => uinput_handler("XF86Mail"),
            ActionType::Search => uinput_handler("XF86Search"),
            ActionType::Calculator => uinput_handler("XF86Calculator"),
            ActionType::ControlCenter => self.open_control_center(),
            ActionType::Logout => uinput_handler("Control_L+Alt_L+Delete"),
            ActionType::Screenshot => uinput_handler("Print"),
            ActionType::ScreenshotWindow => uinput_handler("Alt_L+Print"),
            ActionType::ScreenshotArea => uinput_handler("Shift_L+Print"),
            _ => {}
        }
    }

    fn control_center_candidates(&self) -> &'static [&'static str] {
        // Sway and Hyprland have no settings app of their own, so use whichever is installed.
        if self.is_sway || self.is_hypr {
            &["gnome-control-center", "systemsettings", "xfce4-settings-manager"]
        } else if self.is_gnome {
            &["gnome-control-center"]
        } else if self.is_kde {
            &["systemsettings"]
        } else {
            &["xfce4-settings-manager"]
        }
    }

    fn terminal_candidates(&self) -> Vec<&'static str> {
        // Distributions differ in which terminal they ship, and desktops have no reliable shortcut
        // for it. The first choice is the standard way to ask for the user's preferred one.
        let preferred: &[&str] = if self.is_kde {
            &["konsole"]
        } else if self.is_xfce {
            &["xfce4-terminal"]
        } else if self.is_sway || self.is_hypr {
            &["foot", "kitty", "alacritty"]
        } else {
            // A new window, because Ptyxis otherwise only brings an open one forward.
            &["ptyxis --new-window", "kgx", "gnome-terminal"]
        };
        ["xdg-terminal-exec"]
            .iter()
            .chain(preferred)
            .chain(&["ptyxis --new-window", "kgx", "gnome-terminal", "konsole", "xfce4-terminal", "foot", "kitty", "alacritty", "xterm"])
            .copied()
            .collect()
    }

    fn run_first_installed(&self, commands: &[&str]) {
        if let Some(parts) = first_installed(commands, is_installed) {
            self.run_user_cmd(parts[0], &parts[1..]);
        }
    }

    fn open_terminal(&self) {
        self.run_first_installed(&self.terminal_candidates());
    }

    fn open_control_center(&self) {
        self.run_first_installed(self.control_center_candidates());
    }

    fn run_swaymsg(&self, msg: &str) {
        self.run_user_cmd("swaymsg", &msg.split_whitespace().collect::<Vec<_>>());
    }

    fn run_hyprctl(&self, msg: &str) {
        self.run_user_cmd("hyprctl", &msg.split_whitespace().collect::<Vec<_>>());
    }

    /// Closes the focused window in the extension, as its close button does. A pressed shortcut
    /// is only the way when the extension isn't running.
    fn close_gnome_window(&self, uinput_handler: &dyn Fn(&str)) {
        let closed = zbus::blocking::Connection::session()
            .and_then(|conn| call_extension(&conn, GNOME_SHELL_NAME, "CloseWindow"));
        if closed.is_err() {
            self.run_gnome_shortcut("close", "Alt_L+F4", uinput_handler);
        }
    }

    fn run_gnome_shortcut(&self, key: &str, fallback_keys: &str, uinput_handler: &dyn Fn(&str)) {
        let schemas = vec![
            "org.gnome.desktop.wm.keybindings",
            "org.gnome.settings-daemon.plugins.media-keys",
            "org.gnome.shell.keybindings",
        ];

        // An action can be unassigned and still have a fixed key, like www and www-static.
        let static_key = format!("{key}-static");
        for candidate in [key, static_key.as_str()] {
            for schema in &schemas {
                if let Some(shortcut) = self.get_gnome_shortcut(schema, candidate) {
                    uinput_handler(&shortcut);
                    return;
                }
            }
        }

        // GSettings lookup failed or returned disabled. Try temporary binding
        for schema in &schemas {
            if let Some(orig_val) = self.gsettings_get(schema, key) {
                self.gsettings_set(schema, key, "['<Super><Shift><Control><Alt>F12']");
                let setting = std::env::var("GESTOS_SHORTCUT_DELAY_MS").ok();
                std::thread::sleep(temporary_shortcut_pickup_delay(setting.as_deref()));

                uinput_handler("Super_L+Shift_L+Control_L+Alt_L+F12");
                std::thread::sleep(TEMPORARY_SHORTCUT_RESTORE_DELAY);

                self.gsettings_set(schema, key, &orig_val);
                return;
            }
        }

        if !fallback_keys.is_empty() {
            uinput_handler(fallback_keys);
        }
    }

    fn get_gnome_shortcut(&self, schema: &str, key: &str) -> Option<String> {
        parse_gnome_shortcut(&self.gsettings_get(schema, key)?)
    }

    fn gsettings_get(&self, schema: &str, key: &str) -> Option<String> {
        if gio_reads_user_settings() {
            return read_gnome_setting(schema, key);
        }
        let output = self.run_user_cmd_output("gsettings", &["get", schema, key])?;
        let clean = output.trim().to_string();
        if clean.is_empty() {
            None
        } else {
            Some(clean)
        }
    }

    fn gsettings_set(&self, schema: &str, key: &str, value: &str) {
        self.run_user_cmd("gsettings", &["set", schema, key, value]);
    }

    // The one place a shell belongs: Run Command takes whatever the user would type in one.
    fn run_shell_cmd(&self, cmd: &str) {
        let _ = Command::new("sh").arg("-c").arg(cmd).status();
    }

    // Runs a program in the user session's DBUS environment
    fn run_user_cmd(&self, program: &str, args: &[&str]) {
        if let Ok(mut child) = self
            .build_user_command(program, args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let _ = child.wait();
        }
    }

    fn run_user_cmd_output(&self, program: &str, args: &[&str]) -> Option<String> {
        let output = self.build_user_command(program, args).output().ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            None
        }
    }

    /// Arguments stay separate all the way, so nothing in them can be read as shell syntax.
    fn build_user_command(&self, program: &str, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args);
        if Path::new(&format!("/run/user/{}/bus", self.uid)).exists()
            && std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err()
        {
            command.env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path=/run/user/{}/bus", self.uid),
            );
        }
        // These may have been found by looking, rather than in the environment.
        if self.is_sway {
            command.env("SWAYSOCK", &self.sway_sock);
        } else if self.is_hypr {
            command.env("HYPRLAND_INSTANCE_SIGNATURE", &self.hypr_sig);
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, AtomicUsize};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct FakeShell {
        extension_active: Arc<AtomicBool>,
        tile_calls: Arc<AtomicUsize>,
        notifications: Arc<AtomicUsize>,
        choice: Arc<AtomicI32>,
        chooser_points: Arc<Mutex<Vec<Vec<f64>>>>,
        focused_app: Arc<Mutex<String>>,
        // The extension GNOME says it runs, if it runs one: its fingerprint, if it has one.
        running_fingerprint: Arc<Mutex<Option<Option<String>>>>,
    }

    struct FakeExtensions(FakeShell);
    #[zbus::interface(name = "org.gnome.Shell.Extensions")]
    impl FakeExtensions {
        fn get_extension_info(&self, _uuid: String) -> HashMap<String, zbus::zvariant::OwnedValue> {
            let owned = |value: zbus::zvariant::Value| value.try_to_owned().unwrap();
            let mut info = HashMap::new();
            if let Some(fingerprint) = self.0.running_fingerprint.lock().unwrap().clone() {
                info.insert("state".to_string(), owned(1.0f64.into()));
                let version = crate::gnome_extension::VERSION;
                info.insert("version-name".to_string(), owned(version.into()));
                if let Some(fingerprint) = fingerprint {
                    info.insert("gestos-fingerprint".to_string(), owned(fingerprint.into()));
                }
            }
            info
        }
    }

    struct FakeExtension(FakeShell);
    #[zbus::interface(name = "io.github.deters.Gestos.Extension")]
    impl FakeExtension {
        fn toggle_tile_windows(&self) -> zbus::fdo::Result<bool> {
            if !self.0.extension_active.load(Ordering::SeqCst) {
                return Err(zbus::fdo::Error::UnknownMethod("not loaded".to_string()));
            }
            self.0.tile_calls.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }

        fn focused_app(&self) -> String {
            self.0.focused_app.lock().unwrap().clone()
        }

        fn open_app(&self, app: String) -> bool {
            let known = app != "org.example.Missing";
            if known {
                *self.0.focused_app.lock().unwrap() = app;
            }
            known
        }

        fn choose_gesture(&self, gestures: Vec<(String, String, Vec<f64>)>) -> i32 {
            *self.0.chooser_points.lock().unwrap() =
                gestures.into_iter().map(|(_, _, points)| points).collect();
            self.0.choice.load(Ordering::SeqCst)
        }
    }

    struct FakeNotifications(FakeShell);
    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl FakeNotifications {
        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            _app: String,
            _replaces: u32,
            _icon: String,
            _summary: String,
            _body: String,
            _actions: Vec<String>,
            _hints: HashMap<String, zbus::zvariant::OwnedValue>,
            _timeout: i32,
        ) -> u32 {
            self.0.notifications.fetch_add(1, Ordering::SeqCst);
            1
        }
    }

    // The name is unique per test and process, so this can't reach the real Shell on the bus.
    fn tile_windows_with(shell: &FakeShell, test: &str) -> Option<()> {
        tile_windows_times(shell, test, 1)
    }

    fn tile_windows_times(shell: &FakeShell, test: &str, times: usize) -> Option<()> {
        let conn = zbus::blocking::Connection::session().ok()?;
        let name = format!("io.github.deters.GestosTestTile{}.P{}", test, std::process::id());
        let _service = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(name.clone()))
            .and_then(|b| b.serve_at(GESTOS_EXTENSION_PATH, FakeExtension(shell.clone())))
            .and_then(|b| b.serve_at(NOTIFICATIONS_PATH, FakeNotifications(shell.clone())))
            .and_then(|b| b.serve_at("/org/gnome/Shell", FakeExtensions(shell.clone())))
            .and_then(|b| b.build())
            .unwrap();
        let told = AtomicBool::new(false);
        for _ in 0..times {
            run_extension_action_telling(&conn, &name, &name, "ToggleTileWindows", &told).unwrap();
        }
        Some(())
    }

    #[test]
    fn an_outdated_extension_is_mentioned_once() {
        let shell = FakeShell {
            extension_active: Arc::new(AtomicBool::new(true)),
            // Another extension's code, with the same version.
            running_fingerprint: Arc::new(Mutex::new(Some(Some("0123456789abcdef".to_string())))),
            ..Default::default()
        };
        if tile_windows_times(&shell, "Outdated", 2).is_none() {
            return;
        }
        assert_eq!(shell.tile_calls.load(Ordering::SeqCst), 2);
        assert_eq!(shell.notifications.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_current_extension_is_not_mentioned() {
        let fingerprint = crate::gnome_extension::FINGERPRINT.to_string();
        let shell = FakeShell {
            extension_active: Arc::new(AtomicBool::new(true)),
            running_fingerprint: Arc::new(Mutex::new(Some(Some(fingerprint)))),
            ..Default::default()
        };
        if tile_windows_with(&shell, "Current").is_none() {
            return;
        }
        assert_eq!(shell.notifications.load(Ordering::SeqCst), 0);
    }

    fn choose_with(shell: &FakeShell, test: &str) -> Option<zbus::Result<Option<usize>>> {
        let conn = zbus::blocking::Connection::session().ok()?;
        let name = format!("io.github.deters.GestosTestChoose{}.P{}", test, std::process::id());
        let _service = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(name.clone()))
            .and_then(|b| b.serve_at(GESTOS_EXTENSION_PATH, FakeExtension(shell.clone())))
            .and_then(|b| b.build())
            .unwrap();
        let point = |x, y| crate::protractor::Point2D { x, y };
        let entries = ["Back", "Close"].map(|name| ChooserEntry {
            name: name.to_string(),
            description: String::new(),
            points: vec![point(0.0, 1.0), point(2.0, 3.0)],
        });
        Some(choose_in_extension(&conn, &name, &entries))
    }

    #[test]
    fn the_extension_says_which_app_has_focus() {
        let Ok(conn) = zbus::blocking::Connection::session() else { return };
        let shell = FakeShell::default();
        let name = format!("io.github.deters.GestosTestFocus.P{}", std::process::id());
        let _service = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(name.clone()))
            .and_then(|b| b.serve_at(GESTOS_EXTENSION_PATH, FakeExtension(shell.clone())))
            .and_then(|b| b.build())
            .unwrap();
        assert_eq!(focused_app_from(&conn, &name), None);
        *shell.focused_app.lock().unwrap() = "org.mozilla.firefox".to_string();
        assert_eq!(focused_app_from(&conn, &name).as_deref(), Some("org.mozilla.firefox"));
    }

    #[test]
    fn an_app_opened_by_the_extension_is_ready_once_it_has_focus() {
        let Ok(conn) = zbus::blocking::Connection::session() else { return };
        let shell = FakeShell::default();
        let name = format!("io.github.deters.GestosTestOpen.P{}", std::process::id());
        let _service = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(name.clone()))
            .and_then(|b| b.serve_at(GESTOS_EXTENSION_PATH, FakeExtension(shell.clone())))
            .and_then(|b| b.build())
            .unwrap();
        assert!(open_app_in(&conn, &name, "org.mozilla.firefox").unwrap());
        assert!(wait_for_focus(&conn, &name, &name, "org.mozilla.firefox"));
        assert!(!open_app_in(&conn, &name, "org.example.Missing").unwrap());
    }

    #[test]
    fn the_extension_says_which_gesture_was_chosen() {
        let shell = FakeShell::default();
        shell.choice.store(1, Ordering::SeqCst);
        let Some(choice) = choose_with(&shell, "Picked") else { return };
        assert_eq!(choice.unwrap(), Some(1));
        assert_eq!(*shell.chooser_points.lock().unwrap(), [[0.0, 1.0, 2.0, 3.0]; 2]);
    }

    #[test]
    fn a_cancelled_or_unknown_choice_is_none() {
        let shell = FakeShell::default();
        for choice in [-1, 2] {
            shell.choice.store(choice, Ordering::SeqCst);
            let Some(result) = choose_with(&shell, &format!("Cancelled{}", choice + 1)) else {
                return;
            };
            assert_eq!(result.unwrap(), None);
        }
    }

    #[test]
    fn tile_windows_uses_a_running_extension() {
        let shell = FakeShell {
            extension_active: Arc::new(AtomicBool::new(true)),
            ..Default::default()
        };
        if tile_windows_with(&shell, "Active").is_none() {
            return;
        }
        assert_eq!(shell.tile_calls.load(Ordering::SeqCst), 1);
        assert_eq!(shell.notifications.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn tile_windows_says_when_the_extension_isnt_running() {
        let shell = FakeShell::default();
        if tile_windows_with(&shell, "Missing").is_none() {
            return;
        }
        assert_eq!(shell.tile_calls.load(Ordering::SeqCst), 0);
        assert_eq!(shell.notifications.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn only_tiling_needs_the_extension() {
        assert!(ActionType::ToggleTileWindows.needs_gnome_extension());
        assert!(!ActionType::Maximize.needs_gnome_extension());
        assert!(!ActionType::Gnome("close".to_string()).needs_gnome_extension());
    }

    #[test]
    fn tile_windows_survives_saving() {
        let action = ActionType::ToggleTileWindows;
        assert_eq!(ActionType::from_string(&action.to_string()), Some(action));
    }

    #[test]
    fn arranging_windows_is_tiling_now() {
        assert_eq!(
            ActionType::from_string("toggle-arrange-windows"),
            Some(ActionType::ToggleTileWindows)
        );
    }

    fn gsettings_command(args: &[&str]) -> Option<String> {
        let output = Command::new("gsettings").args(args).output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[test]
    fn gio_reads_the_same_values_as_the_gsettings_command() {
        let schema = "org.gnome.desktop.wm.keybindings";
        if gsettings_command(&["list-keys", schema]).is_none() || !gio_reads_user_settings() {
            eprintln!("skipped: no GNOME settings here");
            return;
        }
        for key in ["close", "minimize", "panel-run-dialog", "switch-to-workspace-1"] {
            let expected = gsettings_command(&["get", schema, key]);
            assert_eq!(read_gnome_setting(schema, key), expected, "{key}");
        }
    }

    #[test]
    fn a_missing_gnome_schema_or_key_is_not_a_problem() {
        assert_eq!(read_gnome_setting("org.example.gestos.missing", "close"), None);
        assert_eq!(read_gnome_setting("org.gnome.desktop.wm.keybindings", "not-a-key"), None);
    }

    /// Changes a real setting for a moment, so it only runs on request.
    #[test]
    #[ignore]
    fn gio_sees_a_shortcut_changed_after_the_schema_was_opened() {
        let (schema, key) = ("org.gnome.desktop.wm.keybindings", "set-spew-mark");
        let original = gsettings_command(&["get", schema, key]).unwrap();
        assert_eq!(read_gnome_setting(schema, key).unwrap(), original);

        gsettings_command(&["set", schema, key, "['<Super><Alt>F11']"]).unwrap();
        let changed = read_gnome_setting(schema, key);
        gsettings_command(&["set", schema, key, &original]).unwrap();

        assert_eq!(changed.as_deref(), Some("['<Super><Alt>F11']"));
        assert_eq!(read_gnome_setting(schema, key).unwrap(), original);
    }

    #[test]
    fn the_temporary_shortcut_wait_can_be_overridden() {
        let millis = std::time::Duration::from_millis;
        assert_eq!(temporary_shortcut_pickup_delay(None), millis(100));
        assert_eq!(temporary_shortcut_pickup_delay(Some("250")), millis(250));
        assert_eq!(temporary_shortcut_pickup_delay(Some("soon")), millis(100));
    }

    #[test]
    fn an_unassigned_gnome_shortcut_has_no_keys() {
        assert_eq!(parse_gnome_shortcut("['']"), None);
        assert_eq!(parse_gnome_shortcut("@as []"), None);
        assert_eq!(parse_gnome_shortcut("[]"), None);
        assert_eq!(parse_gnome_shortcut("['disabled']"), None);
    }

    #[test]
    fn gnome_shortcuts_become_keys_to_press() {
        assert_eq!(parse_gnome_shortcut("['<Super>l']").unwrap(), "Super_L+l");
        assert_eq!(
            parse_gnome_shortcut("['<Control><Alt>Delete']").unwrap(),
            "Control_L+Alt_L+Delete"
        );
        assert_eq!(parse_gnome_shortcut("['<Primary>q']").unwrap(), "Control_L+q");
        assert_eq!(parse_gnome_shortcut("['XF86WWW']").unwrap(), "XF86WWW");
        assert_eq!(
            parse_gnome_shortcut("['XF86AudioRaiseVolume', '<Ctrl>XF86AudioRaiseVolume']").unwrap(),
            "XF86AudioRaiseVolume"
        );
    }

    #[test]
    fn the_first_assigned_shortcut_is_used() {
        assert_eq!(parse_gnome_shortcut("['', 'F4']").unwrap(), "F4");
    }

    #[test]
    fn an_unknown_modifier_is_not_guessed_at() {
        assert_eq!(parse_gnome_shortcut("['<Hyper>x']"), None);
    }

    #[test]
    fn terminals_try_each_desktops_own_first() {
        let gnome = WaylandContext {
            is_gnome: true,
            ..Default::default()
        }
        .terminal_candidates();
        let position = |list: &[&str], text: &str| list.iter().position(|c| *c == text).unwrap();
        assert!(position(&gnome, "xdg-terminal-exec") < position(&gnome, "ptyxis --new-window"));
        assert!(position(&gnome, "ptyxis --new-window") < position(&gnome, "konsole"));

        let kde = WaylandContext {
            is_kde: true,
            ..Default::default()
        }
        .terminal_candidates();
        assert!(position(&kde, "konsole") < position(&kde, "ptyxis --new-window"));
    }

    #[test]
    fn the_first_installed_command_keeps_its_arguments() {
        let found = first_installed(&["missing", "ptyxis --new-window", "kgx"], |program| {
            program != "missing"
        });
        assert_eq!(found.unwrap(), ["ptyxis", "--new-window"]);
        assert_eq!(first_installed(&["missing"], |_| false), None);
    }

    #[test]
    fn programs_are_found_in_path() {
        assert!(is_installed("sh"));
        assert!(!is_installed("gestos-no-such-program"));
    }

    #[test]
    fn control_center_uses_the_desktops_own_settings_app() {
        let gnome = WaylandContext {
            is_gnome: true,
            ..Default::default()
        };
        assert_eq!(gnome.control_center_candidates(), ["gnome-control-center"]);
        let kde = WaylandContext {
            is_kde: true,
            ..Default::default()
        };
        assert_eq!(kde.control_center_candidates(), ["systemsettings"]);
    }

    #[test]
    fn shell_syntax_in_an_argument_stays_plain_text() {
        let ctx = WaylandContext::default();
        let marker = std::env::temp_dir().join(format!("gestos-noshell-{}", std::process::id()));
        let trap = format!("$(touch {})", marker.display());
        let output = ctx.run_user_cmd_output("echo", &[&trap]).unwrap();
        assert_eq!(output.trim(), trap);
        assert!(!marker.exists());
    }
}
