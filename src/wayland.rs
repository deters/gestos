use crate::config::ActionType;
use gio::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

// How long GNOME gets to pick up a changed shortcut before it's used. The action can't start
// until this is over, so it's as short as seems safe.
const TEMPORARY_SHORTCUT_PICKUP_DELAY_MILLIS: u64 = 100;
// How long GNOME gets to act on the shortcut before it's restored. Nothing waits for this.
const TEMPORARY_SHORTCUT_RESTORE_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

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
fn gio_reads_user_settings() -> bool {
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
fn read_gnome_setting(schema_id: &str, key: &str) -> Option<String> {
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

/// A command that runs the first of these that's installed.
fn first_available_command<'a>(commands: impl Iterator<Item = &'a &'a str>) -> String {
    let attempts: Vec<String> = commands
        .map(|command| {
            let program = command.split_whitespace().next().unwrap_or(command);
            format!("command -v {program} >/dev/null && exec {command}")
        })
        .collect();
    format!("sh -c '{}'", attempts.join("; "))
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
    pub username: String,
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

        // Resolve target user UID and username (handles running as root/sudo)
        let sudo_uid = std::env::var("SUDO_UID")
            .ok()
            .and_then(|s| s.parse::<u32>().ok());
        let sudo_user = std::env::var("SUDO_USER").ok();

        if let (Some(uid), Some(user)) = (sudo_uid, sudo_user) {
            ctx.uid = uid;
            ctx.username = user;
        } else {
            ctx.uid = unsafe { libc::getuid() };
            if ctx.uid > 0 {
                if let Some(user) = get_username_from_uid(ctx.uid) {
                    ctx.username = user;
                }
            }
        }

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

        // If still not resolved (running as raw root), look in /run/user for any user session
        if ctx.uid == 0 || (!ctx.is_sway && !ctx.is_hypr) {
            if let Ok(entries) = fs::read_dir("/run/user") {
                for entry in entries.flatten() {
                    let d_name = entry.file_name().to_string_lossy().into_owned();
                    if let Ok(d_uid) = d_name.parse::<u32>() {
                        if d_uid >= 1000 {
                            let user_run_dir = PathBuf::from(format!("/run/user/{}", d_uid));

                            // Check Sway socket
                            if let Ok(sub_entries) = fs::read_dir(&user_run_dir) {
                                for sub_entry in sub_entries.flatten() {
                                    let name = sub_entry.file_name().to_string_lossy().into_owned();
                                    if name.contains("sway-ipc") && name.contains(".sock") {
                                        ctx.sway_sock =
                                            user_run_dir.join(name).to_string_lossy().into_owned();
                                        ctx.is_sway = true;
                                        break;
                                    }
                                }
                            }

                            // Check Hyprland signature
                            if !ctx.is_sway {
                                let hypr_dir = user_run_dir.join("hypr");
                                if hypr_dir.exists() {
                                    if let Ok(sub_entries) = fs::read_dir(hypr_dir) {
                                        for sub_entry in sub_entries.flatten() {
                                            let name = sub_entry
                                                .file_name()
                                                .to_string_lossy()
                                                .into_owned();
                                            if name.len() >= 40 {
                                                ctx.hypr_sig = name;
                                                ctx.is_hypr = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            ctx.uid = d_uid;
                            if let Some(user) = get_username_from_uid(ctx.uid) {
                                ctx.username = user;
                            }
                            break;
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
                self.run_user_cmd("swaylock");
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
                self.run_user_cmd("hyprlock");
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
            ActionType::Kill => self.run_gnome_shortcut("close", "Alt_L+F4", uinput_handler),
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
                let cmd = format!(
                    "dbus-send --session --type=method_call --dest=org.kde.kglobalaccel /component/{} org.kde.kglobalaccel.Component.invokeShortcut string:\"{}\"",
                    comp, short
                );
                self.run_user_cmd(&cmd);
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
                    self_clone.run_shell_cmd("xflock4");
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

    fn control_center_command(&self) -> String {
        // Sway and Hyprland have no settings app of their own, so use whichever is installed.
        let candidates: &[&str] = if self.is_sway || self.is_hypr {
            &["gnome-control-center", "systemsettings", "xfce4-settings-manager"]
        } else if self.is_gnome {
            &["gnome-control-center"]
        } else if self.is_kde {
            &["systemsettings"]
        } else {
            &["xfce4-settings-manager"]
        };
        format!(
            "sh -c 'for c in {}; do command -v $c >/dev/null && exec $c; done'",
            candidates.join(" ")
        )
    }

    fn terminal_command(&self) -> String {
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
        first_available_command(
            ["xdg-terminal-exec"]
                .iter()
                .chain(preferred)
                .chain(&["ptyxis --new-window", "kgx", "gnome-terminal", "konsole", "xfce4-terminal", "foot", "kitty", "alacritty", "xterm"]),
        )
    }

    fn open_terminal(&self) {
        self.run_user_cmd(&self.terminal_command());
    }

    fn open_control_center(&self) {
        self.run_user_cmd(&self.control_center_command());
    }

    fn run_swaymsg(&self, msg: &str) {
        let cmd = format!("swaymsg {}", msg);
        self.run_user_cmd(&cmd);
    }

    fn run_hyprctl(&self, msg: &str) {
        let cmd = format!("hyprctl {}", msg);
        self.run_user_cmd(&cmd);
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
        // Running as root, GIO would read root's settings, and only the command can switch users.
        if unsafe { libc::getuid() } != 0 && gio_reads_user_settings() {
            return read_gnome_setting(schema, key);
        }
        let cmd = format!("gsettings get {} {}", schema, key);
        let output = self.run_user_cmd_output(&cmd)?;
        let clean = output.trim().to_string();
        if clean.is_empty() {
            None
        } else {
            Some(clean)
        }
    }

    fn gsettings_set(&self, schema: &str, key: &str, value: &str) {
        let cmd = format!("gsettings set {} {} \"{}\"", schema, key, value);
        self.run_user_cmd(&cmd);
    }

    fn run_shell_cmd(&self, cmd: &str) {
        let _ = Command::new("sh").arg("-c").arg(cmd).status();
    }

    // Runs a command in the user session's DBUS environment
    fn run_user_cmd(&self, cmd: &str) {
        if let Ok(mut child) = self
            .build_user_command(cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let _ = child.wait();
        }
    }

    fn run_user_cmd_output(&self, cmd: &str) -> Option<String> {
        let output = self.build_user_command(cmd).output().ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            None
        }
    }

    fn build_user_command(&self, cmd: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c");

        let current_uid = unsafe { libc::getuid() };
        let mut prefix = String::new();

        let dbus_env = if Path::new(&format!("/run/user/{}/bus", self.uid)).exists()
            && std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err()
        {
            format!(
                "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{}/bus ",
                self.uid
            )
        } else {
            String::new()
        };

        if current_uid == 0 && !self.username.is_empty() {
            if self.is_sway {
                prefix = format!(
                    "sudo -u {} env {}XDG_RUNTIME_DIR=/run/user/{} SWAYSOCK={} ",
                    self.username, dbus_env, self.uid, self.sway_sock
                );
            } else if self.is_hypr {
                prefix = format!(
                    "sudo -u {} env {}XDG_RUNTIME_DIR=/run/user/{} HYPRLAND_INSTANCE_SIGNATURE={} ",
                    self.username, dbus_env, self.uid, self.hypr_sig
                );
            } else {
                prefix = format!(
                    "sudo -u {} env {}XDG_RUNTIME_DIR=/run/user/{} ",
                    self.username, dbus_env, self.uid
                );
            }
        } else if !dbus_env.is_empty() {
            prefix = dbus_env;
        }

        command.arg(format!("{}{}", prefix, cmd));
        command
    }
}

fn get_username_from_uid(uid: u32) -> Option<String> {
    unsafe {
        let passwd_ptr = libc::getpwuid(uid);
        if !passwd_ptr.is_null() {
            let name_c = std::ffi::CStr::from_ptr((*passwd_ptr).pw_name);
            return Some(name_c.to_string_lossy().into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn terminal_command_tries_each_desktops_terminal_first() {
        let gnome = WaylandContext {
            is_gnome: true,
            ..Default::default()
        };
        let command = gnome.terminal_command();
        let position = |text: &str| command.find(text).unwrap();
        assert!(position("xdg-terminal-exec") < position("exec ptyxis --new-window"));
        assert!(position("exec ptyxis") < position("exec konsole"));

        let kde = WaylandContext {
            is_kde: true,
            ..Default::default()
        }
        .terminal_command();
        assert!(kde.find("exec konsole").unwrap() < kde.find("exec ptyxis").unwrap());
    }

    #[test]
    fn the_first_installed_command_runs_with_its_arguments() {
        let command = first_available_command(
            ["gestos-no-such-program", "echo hello"].iter(),
        );
        let output = Command::new("sh").arg("-c").arg(command).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
    }

    #[test]
    fn control_center_uses_the_desktops_own_settings_app() {
        let gnome = WaylandContext {
            is_gnome: true,
            ..Default::default()
        };
        assert!(gnome.control_center_command().contains("in gnome-control-center;"));

        let kde = WaylandContext {
            is_kde: true,
            ..Default::default()
        };
        assert!(kde.control_center_command().contains("in systemsettings;"));
    }

    #[test]
    fn control_center_command_runs_a_real_program_or_does_nothing() {
        let status = Command::new("sh")
            .arg("-c")
            .arg(
                WaylandContext::default()
                    .control_center_command()
                    .replace("xfce4-settings-manager", "gestos-no-such-program true"),
            )
            .status()
            .unwrap();
        assert!(status.success());
    }
}
