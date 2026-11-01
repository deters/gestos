use crate::modifiers::Modifiers;
use crate::protractor::Point2D;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionType {
    Iconify,
    Kill,
    Lower,
    Raise,
    Maximize,
    Restore,
    ToggleMaximized,
    Keypress(String),
    Execute(String),
    WorkspaceLeft,
    WorkspaceRight,
    WorkspaceUp,
    WorkspaceDown,
    ShowOverview,
    ShowAppGrid,
    Click(Option<i32>),
    ToggleFullscreen,
    ShowDesktop,
    LockScreen,
    Terminal,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MediaPlay,
    MediaNext,
    MediaPrev,
    Www,
    Home,
    Email,
    Search,
    Calculator,
    ControlCenter,
    Logout,
    Screenshot,
    ScreenshotWindow,
    ScreenshotArea,
    Gnome(String),
    Kde(String, String),
    Abort,
}

impl ActionType {
    pub fn from_string(s: &str) -> Option<Self> {
        let s = s.trim();
        let mut parts = s.splitn(2, [' ', '\t']);
        let cmd = parts.next()?.to_lowercase();
        let arg = parts.next().unwrap_or("").trim().to_string();

        match cmd.as_str() {
            "iconify" => Some(ActionType::Iconify),
            "kill" => Some(ActionType::Kill),
            "lower" => Some(ActionType::Lower),
            "raise" => Some(ActionType::Raise),
            "maximize" => Some(ActionType::Maximize),
            "restore" => Some(ActionType::Restore),
            "toggle-maximized" => Some(ActionType::ToggleMaximized),
            "keypress" | "keys" => {
                let normalized = arg.replace("_L", "").replace("_R", "");
                Some(ActionType::Keypress(normalized))
            }
            "exec" => Some(ActionType::Execute(arg)),
            "workspace-left" => Some(ActionType::WorkspaceLeft),
            "workspace-right" => Some(ActionType::WorkspaceRight),
            "workspace-up" => Some(ActionType::WorkspaceUp),
            "workspace-down" => Some(ActionType::WorkspaceDown),
            "show-overview" => Some(ActionType::ShowOverview),
            "show-app-grid" => Some(ActionType::ShowAppGrid),
            "click" => {
                let btn = arg.parse::<i32>().ok();
                Some(ActionType::Click(btn))
            }
            "toggle-fullscreen" => Some(ActionType::ToggleFullscreen),
            "show-desktop" => Some(ActionType::ShowDesktop),
            "lock-screen" => Some(ActionType::LockScreen),
            "terminal" => Some(ActionType::Terminal),
            "volume-up" => Some(ActionType::VolumeUp),
            "volume-down" => Some(ActionType::VolumeDown),
            "volume-mute" => Some(ActionType::VolumeMute),
            "media-play" => Some(ActionType::MediaPlay),
            "media-next" => Some(ActionType::MediaNext),
            "media-prev" => Some(ActionType::MediaPrev),
            "www" => Some(ActionType::Www),
            "home" => Some(ActionType::Home),
            "email" => Some(ActionType::Email),
            "search" => Some(ActionType::Search),
            "calculator" => Some(ActionType::Calculator),
            "control-center" => Some(ActionType::ControlCenter),
            "logout" => Some(ActionType::Logout),
            "screenshot" => Some(ActionType::Screenshot),
            "screenshot-window" => Some(ActionType::ScreenshotWindow),
            "screenshot-area" => Some(ActionType::ScreenshotArea),
            "gnome" => Some(ActionType::Gnome(arg)),
            "kde" => {
                let mut parts = arg.splitn(2, [' ', '\t']);
                let comp = parts.next()?.to_string();
                let short = parts.next()?.to_string();
                Some(ActionType::Kde(comp, short))
            }
            "disabled" | "abort" => Some(ActionType::Abort),
            _ => None,
        }
    }

    /// The GNOME shortcut key a built-in action runs on GNOME, if it has one.
    pub fn gnome_key(&self) -> Option<&'static str> {
        match self {
            ActionType::Iconify => Some("minimize"),
            ActionType::Kill => Some("close"),
            ActionType::Maximize => Some("maximize"),
            ActionType::Restore => Some("unmaximize"),
            ActionType::ToggleMaximized => Some("toggle-maximized"),
            ActionType::ToggleFullscreen => Some("toggle-fullscreen"),
            ActionType::WorkspaceLeft => Some("switch-to-workspace-left"),
            ActionType::WorkspaceRight => Some("switch-to-workspace-right"),
            ActionType::WorkspaceUp => Some("switch-to-workspace-up"),
            ActionType::WorkspaceDown => Some("switch-to-workspace-down"),
            ActionType::ShowOverview => Some("toggle-overview"),
            ActionType::ShowAppGrid => Some("toggle-application-view"),
            ActionType::ShowDesktop => Some("show-desktop"),
            ActionType::LockScreen => Some("screensaver"),
            ActionType::Terminal => Some("terminal"),
            ActionType::VolumeUp => Some("volume-up"),
            ActionType::VolumeDown => Some("volume-down"),
            ActionType::VolumeMute => Some("volume-mute"),
            ActionType::MediaPlay => Some("play"),
            ActionType::MediaNext => Some("next"),
            ActionType::MediaPrev => Some("previous"),
            ActionType::Www => Some("www"),
            ActionType::Home => Some("home"),
            ActionType::Email => Some("email"),
            ActionType::Search => Some("search"),
            ActionType::Calculator => Some("calculator"),
            ActionType::Logout => Some("logout"),
            ActionType::Screenshot => Some("screenshot"),
            ActionType::ScreenshotWindow => Some("window-screenshot"),
            ActionType::ScreenshotArea => Some("area-screenshot"),
            _ => None,
        }
    }

    pub fn get_icon_name(&self) -> &'static str {
        match self {
            ActionType::Execute(_) => "utilities-terminal-symbolic",
            ActionType::Keypress(_) => "preferences-desktop-keyboard-shortcuts-symbolic",
            ActionType::Gnome(_) => "preferences-system-symbolic",
            ActionType::Kde(_, _) => "preferences-system-symbolic",
            ActionType::Iconify => "window-minimize-symbolic",
            ActionType::Kill => "window-close-symbolic",
            ActionType::Maximize | ActionType::ToggleMaximized => "window-maximize-symbolic",
            ActionType::Restore => "window-restore-symbolic",
            ActionType::ToggleFullscreen => "view-fullscreen-symbolic",
            ActionType::Lower | ActionType::Raise => "window-new-symbolic",
            ActionType::WorkspaceLeft => "go-previous-symbolic",
            ActionType::WorkspaceRight => "go-next-symbolic",
            ActionType::WorkspaceUp => "go-up-symbolic",
            ActionType::WorkspaceDown => "go-down-symbolic",
            ActionType::VolumeUp | ActionType::VolumeDown | ActionType::VolumeMute => {
                "audio-volume-high-symbolic"
            }
            ActionType::MediaPlay | ActionType::MediaNext | ActionType::MediaPrev => {
                "media-playback-start-symbolic"
            }
            ActionType::Terminal => "utilities-terminal-symbolic",
            ActionType::Www => "web-browser-symbolic",
            ActionType::Home => "user-home-symbolic",
            ActionType::Email => "mail-unread-symbolic",
            ActionType::Search => "system-search-symbolic",
            ActionType::Calculator => "accessories-calculator-symbolic",
            ActionType::ControlCenter => "preferences-system-symbolic",
            ActionType::LockScreen => "system-lock-screen-symbolic",
            ActionType::Logout => "system-log-out-symbolic",
            ActionType::Screenshot | ActionType::ScreenshotWindow | ActionType::ScreenshotArea => {
                "camera-photo-symbolic"
            }
            _ => "system-run-symbolic",
        }
    }

    pub fn get_description(&self) -> String {
        match self {
            ActionType::Iconify => "Minimize Window".to_string(),
            ActionType::Kill => "Close Window".to_string(),
            ActionType::Lower => "Lower Window".to_string(),
            ActionType::Raise => "Raise Window".to_string(),
            ActionType::Maximize => "Maximize Window".to_string(),
            ActionType::Restore => "Restore Window".to_string(),
            ActionType::ToggleMaximized => "Toggle Maximize".to_string(),
            ActionType::Keypress(keys) => format!("Press keys: {}", keys),
            ActionType::Execute(cmd) => format!("Run: {}", cmd),
            ActionType::WorkspaceLeft => "Workspace Left".to_string(),
            ActionType::WorkspaceRight => "Workspace Right".to_string(),
            ActionType::WorkspaceUp => "Workspace Up".to_string(),
            ActionType::WorkspaceDown => "Workspace Down".to_string(),
            ActionType::ShowOverview => "Show Overview".to_string(),
            ActionType::ShowAppGrid => "Show App Grid".to_string(),
            ActionType::Click(btn) => format!("Click Button {}", btn.unwrap_or(1)),
            ActionType::ToggleFullscreen => "Toggle Fullscreen".to_string(),
            ActionType::ShowDesktop => "Show Desktop".to_string(),
            ActionType::LockScreen => "Lock Screen".to_string(),
            ActionType::Terminal => "Open Terminal".to_string(),
            ActionType::VolumeUp => "Volume Up".to_string(),
            ActionType::VolumeDown => "Volume Down".to_string(),
            ActionType::VolumeMute => "Mute Volume".to_string(),
            ActionType::MediaPlay => "Play/Pause Media".to_string(),
            ActionType::MediaNext => "Next Track".to_string(),
            ActionType::MediaPrev => "Previous Track".to_string(),
            ActionType::Www => "Open Web Browser".to_string(),
            ActionType::Home => "Open Home Folder".to_string(),
            ActionType::Email => "Open Email Client".to_string(),
            ActionType::Search => "Search".to_string(),
            ActionType::Calculator => "Open Calculator".to_string(),
            ActionType::ControlCenter => "Open Control Center".to_string(),
            ActionType::Logout => "Log Out".to_string(),
            ActionType::Screenshot => "Take Screenshot".to_string(),
            ActionType::ScreenshotWindow => "Screenshot Window".to_string(),
            ActionType::ScreenshotArea => "Screenshot Area".to_string(),
            ActionType::Gnome(cmd) => format!("GNOME: {}", cmd),
            ActionType::Kde(comp, method) => format!("KDE: {} {}", comp, method),
            ActionType::Abort => "Abort".to_string(),
        }
    }
}


impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::Iconify => write!(f, "iconify"),
            ActionType::Kill => write!(f, "kill"),
            ActionType::Lower => write!(f, "lower"),
            ActionType::Raise => write!(f, "raise"),
            ActionType::Maximize => write!(f, "maximize"),
            ActionType::Restore => write!(f, "restore"),
            ActionType::ToggleMaximized => write!(f, "toggle-maximized"),
            ActionType::Keypress(arg) => write!(f, "keypress {}", arg),
            ActionType::Execute(arg) => write!(f, "exec {}", arg),
            ActionType::WorkspaceLeft => write!(f, "workspace-left"),
            ActionType::WorkspaceRight => write!(f, "workspace-right"),
            ActionType::WorkspaceUp => write!(f, "workspace-up"),
            ActionType::WorkspaceDown => write!(f, "workspace-down"),
            ActionType::ShowOverview => write!(f, "show-overview"),
            ActionType::ShowAppGrid => write!(f, "show-app-grid"),
            ActionType::Click(btn) => {
                if let Some(b) = btn {
                    write!(f, "click {}", b)
                } else {
                    write!(f, "click")
                }
            }
            ActionType::ToggleFullscreen => write!(f, "toggle-fullscreen"),
            ActionType::ShowDesktop => write!(f, "show-desktop"),
            ActionType::LockScreen => write!(f, "lock-screen"),
            ActionType::Terminal => write!(f, "terminal"),
            ActionType::VolumeUp => write!(f, "volume-up"),
            ActionType::VolumeDown => write!(f, "volume-down"),
            ActionType::VolumeMute => write!(f, "volume-mute"),
            ActionType::MediaPlay => write!(f, "media-play"),
            ActionType::MediaNext => write!(f, "media-next"),
            ActionType::MediaPrev => write!(f, "media-prev"),
            ActionType::Www => write!(f, "www"),
            ActionType::Home => write!(f, "home"),
            ActionType::Email => write!(f, "email"),
            ActionType::Search => write!(f, "search"),
            ActionType::Calculator => write!(f, "calculator"),
            ActionType::ControlCenter => write!(f, "control-center"),
            ActionType::Logout => write!(f, "logout"),
            ActionType::Screenshot => write!(f, "screenshot"),
            ActionType::ScreenshotWindow => write!(f, "screenshot-window"),
            ActionType::ScreenshotArea => write!(f, "screenshot-area"),
            ActionType::Gnome(arg) => write!(f, "gnome {}", arg),
            ActionType::Kde(comp, short) => write!(f, "kde {} {}", comp, short),
            ActionType::Abort => write!(f, "abort"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionsVecOrString {
    Single(String),
    Multiple(Vec<String>),
}

impl ActionsVecOrString {
    pub fn to_actions(&self) -> Vec<ActionType> {
        match self {
            ActionsVecOrString::Single(s) => ActionType::from_string(s).into_iter().collect(),
            ActionsVecOrString::Multiple(v) => v
                .iter()
                .filter_map(|s| ActionType::from_string(s))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GestureConfig {
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<String>,
    #[serde(rename = "move")]
    pub movement_expr: Option<String>,
    #[serde(rename = "do")]
    pub action_expr: ActionsVecOrString,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMatch {
    pub class: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub name: String,
    #[serde(rename = "match")]
    pub match_rules: AppMatch,
    pub gestures: HashMap<String, GestureConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct YamlConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<serde_yaml::Mapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apps: Option<Vec<AppConfig>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reduce_notifications: bool,
}

#[derive(Debug, Clone)]
pub struct Gesture {
    pub id: String,
    pub name: String,
    pub raw_movement: String,
    pub points: Vec<Point2D>,
    pub actions: Vec<ActionType>,
    pub is_custom: bool,
    pub is_modified: bool,
    pub is_deleted: bool,
    pub modifiers: Modifiers,
}

impl Gesture {
    /// What the gesture does, for showing below its name. Desktop actions are left out, because
    /// all there is to say about them is an internal key, and their name already says it.
    pub fn action_description(&self) -> String {
        self.actions
            .iter()
            .filter(|action| !matches!(action, ActionType::Gnome(_) | ActionType::Kde(_, _)))
            .map(|action| action.get_description())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Clone)]
pub struct Configuration {
    pub gestures: Vec<Gesture>,
    pub apps: Vec<AppConfig>,
    pub user_config_path: PathBuf,
    /// Whether to skip the notification that says which gesture ran.
    pub reduce_notifications: bool,
}

pub fn parse_points(expr: &str) -> Vec<Point2D> {
    let mut points = Vec::new();
    for token in expr.split_whitespace() {
        let mut coords = token.split(',');
        if let (Some(x_str), Some(y_str)) = (coords.next(), coords.next()) {
            if let (Ok(x), Ok(y)) = (x_str.parse::<f64>(), y_str.parse::<f64>()) {
                points.push(Point2D { x, y });
            }
        }
    }
    points
}

pub fn get_environment_suffix() -> Option<&'static str> {
    if std::env::var("SWAYSOCK").is_ok() {
        return Some("sway");
    }
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return Some("hyprland");
    }
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        if desktop.contains("GNOME") || desktop.contains("gnome") || desktop.contains("Ubuntu") {
            return Some("gnome");
        }
        if desktop.contains("KDE") || desktop.contains("kde") {
            return Some("kde");
        }
        if desktop.contains("XFCE") || desktop.contains("xfce") {
            return Some("xfce");
        }
    }
    None
}

/// A desktop that doesn't run autostart entries by itself: its name, its config file, and the
/// line that starts the daemon from there.
pub fn manual_autostart_for(
    environment: Option<&str>,
) -> Option<(&'static str, &'static str, &'static str)> {
    match environment {
        Some("sway") => Some(("Sway", "~/.config/sway/config", "exec gestosd")),
        Some("hyprland") => Some((
            "Hyprland",
            "~/.config/hypr/hyprland.conf",
            "exec-once = gestosd",
        )),
        _ => None,
    }
}

pub fn get_dbus_name() -> String {
    "io.github.deters.Gestos.Daemon".to_string()
}

pub fn config_home() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config")
    } else {
        PathBuf::from(".")
    }
}

/// The autostart folder this project has always written to, which ignores XDG_CONFIG_HOME.
pub fn autostart_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config").join("autostart"))
}

/// The daemon's autostart entry, for a command line such as `gestosd -b 8`.
pub fn daemon_autostart_entry() -> &'static str {
    "[Desktop Entry]\n\
     Type=Application\n\
     Name=Gestos Daemon\n\
     Comment=Gesture recognition daemon\n\
     Exec=gestosd\n\
     Icon=gestos\n\
     Terminal=false\n\
     X-GNOME-Autostart-enabled=true\n"
}

/// Rewrites an old autostart entry for the new binary, keeping any custom arguments.
fn migrate_autostart_entry(old: &str) -> String {
    let mut lines: Vec<String> = old
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                line.replace("mygestures", "gestosd")
            } else if line.starts_with("Icon=") {
                line.replace("mygestures", "gestos")
            } else {
                line.replace("MyGestures", "Gestos")
            }
        })
        .collect();
    lines.push(String::new());
    lines.join("\n")
}

/// Carries a MyGestures setup over to Gestos, once, for an upgrading user. The old config is
/// copied rather than moved so that going back to the old version still works. Files that
/// already exist are never overwritten.
pub fn migrate_legacy_user_files(config_home: &Path, autostart_dir: Option<&Path>) {
    let new_dir = config_home.join("gestos");
    if let Ok(entries) = fs::read_dir(config_home.join("mygestures")) {
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().replacen("mygestures", "gestos", 1);
            let target = new_dir.join(&name);
            if target.exists() {
                continue;
            }
            let copied = fs::create_dir_all(&new_dir)
                .and_then(|_| fs::copy(entry.path(), &target).map(|_| ()));
            match copied {
                Ok(()) => println!("Migrated configuration to {}", target.display()),
                Err(e) => eprintln!("Failed to migrate {}: {}", entry.path().display(), e),
            }
        }
    }

    let Some(autostart_dir) = autostart_dir else {
        return;
    };
    let old_entry = autostart_dir.join("mygestures.desktop");
    let new_entry = autostart_dir.join("gestosd.desktop");
    if let Ok(old) = fs::read_to_string(&old_entry) {
        let written = new_entry.exists()
            || fs::write(&new_entry, migrate_autostart_entry(&old)).is_ok();
        if written {
            let _ = fs::remove_file(&old_entry);
        }
    }
}

pub fn get_default_config_path() -> PathBuf {
    let dir = config_home().join("gestos");
    if let Some(suffix) = get_environment_suffix() {
        dir.join(format!("gestos_{}.yaml", suffix))
    } else {
        dir.join("gestos.yaml")
    }
}

pub fn initialize_user_config_if_missing() -> std::io::Result<PathBuf> {
    let user_path = get_default_config_path();
    if user_path.exists() {
        return Ok(user_path);
    }

    let sys_paths = vec![
        PathBuf::from("/etc/gestos"),
        PathBuf::from("/usr/local/etc/gestos"),
        PathBuf::from("/usr/share/gestos"),
        PathBuf::from("/usr/local/share/gestos"),
        PathBuf::from("."),
    ];

    let suffix = get_environment_suffix();
    let mut default_content = None;

    for base in &sys_paths {
        let path = if let Some(s) = suffix {
            base.join(format!("gestos_{}.yaml", s))
        } else {
            base.join("gestos.yaml")
        };
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                default_content = Some(content);
                break;
            }
        }
    }

    if default_content.is_none() {
        for base in &sys_paths {
            let path = base.join("gestos.yaml");
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    default_content = Some(content);
                    break;
                }
            }
        }
    }

    if let Some(content) = default_content {
        if let Some(parent) = user_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&user_path, &content)?;
        println!(
            "Initialized user configuration from template at: {}",
            user_path.display()
        );
    } else {
        if let Some(parent) = user_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&user_path, "# Gestos Configuration\nglobal:\n")?;
        println!(
            "Initialized empty user configuration at: {}",
            user_path.display()
        );
    }

    Ok(user_path)
}

impl Configuration {
    pub fn load_from_defaults() -> Self {
        let user_path = get_default_config_path();
        let mut config = Configuration {
            gestures: Vec::new(),
            apps: Vec::new(),
            user_config_path: user_path.clone(),
            reduce_notifications: false,
        };

        if user_path.exists() {
            if let Ok(content) = fs::read_to_string(&user_path) {
                config.parse_and_merge(&content, true);
            }
        }

        config
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Option<Self> {
        let content = fs::read_to_string(path.as_ref()).ok()?;
        let mut config = Configuration {
            gestures: Vec::new(),
            apps: Vec::new(),
            user_config_path: path.as_ref().to_path_buf(),
            reduce_notifications: false,
        };
        config.parse_and_merge(&content, true).then_some(config)
    }

    fn parse_and_merge(&mut self, content: &str, is_user: bool) -> bool {
        let parsed: YamlConfig = match serde_yaml::from_str(content) {
            Ok(c) => c,
            Err(e) => {
                log::error!("YAML parse error: {}", e);
                return false;
            }
        };

        if let Some(global) = parsed.global {
            for (key, val) in global {
                let name = match key.as_str() {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                let gest_cfg: GestureConfig = match serde_yaml::from_value(val) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        log::warn!("Failed to parse gesture config for {}: {}", name, e);
                        continue;
                    }
                };
                let mut actions = gest_cfg.action_expr.to_actions();
                // A gesture does one thing. A list is still read, so that an older config loads.
                if actions.len() > 1 {
                    eprintln!(
                        "Gesture '{}' lists {} actions. Only the first one is used.",
                        name,
                        actions.len()
                    );
                    actions.truncate(1);
                }
                let is_abort = actions.contains(&ActionType::Abort);
                let raw_movement = gest_cfg
                    .movement_expr
                    .unwrap_or_else(|| "0,0 0,0".to_string());
                let points = parse_points(&raw_movement);
                let modifiers = gest_cfg
                    .modifiers
                    .as_deref()
                    .map(Modifiers::parse)
                    .unwrap_or_default();

                let matched_pos = if let Some(ref id) = gest_cfg.id {
                    self.gestures.iter().position(|g| &g.id == id)
                } else {
                    self.gestures.iter().position(|g| g.name == name)
                };

                if let Some(pos) = matched_pos {
                    let existing = &mut self.gestures[pos];
                    if is_user {
                        existing.is_modified = true;
                    }
                    if let Some(ref id) = gest_cfg.id {
                        existing.id = id.clone();
                    }
                    existing.name = name;
                    existing.raw_movement = raw_movement;
                    existing.points = points;
                    existing.actions = actions;
                    existing.is_deleted = is_abort;
                    existing.modifiers = modifiers;
                } else {
                    let id = gest_cfg.id.clone().unwrap_or_else(|| {
                        if is_user {
                            generate_unique_id()
                        } else {
                            slugify(&name)
                        }
                    });
                    self.gestures.push(Gesture {
                        id,
                        name,
                        raw_movement,
                        points,
                        actions,
                        is_custom: is_user,
                        is_modified: false,
                        is_deleted: is_abort,
                        modifiers,
                    });
                }
            }
        }

        if let Some(apps) = parsed.apps {
            self.apps = apps;
        }

        self.reduce_notifications = parsed.reduce_notifications;

        true
    }

    /// Whether any gesture needs a modifier held, which is the only reason to open keyboards.
    pub fn uses_modifiers(&self) -> bool {
        self.gestures
            .iter()
            .any(|gesture| !gesture.is_deleted && !gesture.modifiers.is_empty())
    }

    fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        let mut global = serde_yaml::Mapping::new();
        for g in &self.gestures {
            let action_expr = match g.actions.first() {
                Some(action) => ActionsVecOrString::Single(action.to_string()),
                None => ActionsVecOrString::Multiple(Vec::new()),
            };
            let entry = GestureConfig {
                id: Some(g.id.clone()),
                modifiers: (!g.modifiers.is_empty()).then(|| g.modifiers.to_string()),
                movement_expr: Some(g.raw_movement.clone()),
                action_expr,
            };
            global.insert(g.name.clone().into(), serde_yaml::to_value(entry)?);
        }
        let yaml = YamlConfig {
            global: (!global.is_empty()).then_some(global),
            apps: (!self.apps.is_empty()).then(|| self.apps.clone()),
            reduce_notifications: self.reduce_notifications,
        };
        Ok(format!(
            "# Gestos Configuration - Generated by Gestos\n\n{}",
            serde_yaml::to_string(&yaml)?
        ))
    }

    /// Replaces the file in one step, so a daemon reloading at the same moment never reads a
    /// partial config. A symlinked config stays a symlink, and existing permissions are kept.
    pub fn save_to_file(&self) -> std::io::Result<()> {
        if let Some(parent) = self.user_config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = self.to_yaml().map_err(std::io::Error::other)?;

        let target =
            fs::canonicalize(&self.user_config_path).unwrap_or(self.user_config_path.clone());
        let mut temp_name = target.clone().into_os_string();
        temp_name.push(format!(".{}.tmp", std::process::id()));
        let temp_path = PathBuf::from(temp_name);

        let result = fs::write(&temp_path, content)
            .and_then(|_| match fs::metadata(&target) {
                Ok(metadata) => fs::set_permissions(&temp_path, metadata.permissions()),
                Err(_) => Ok(()),
            })
            .and_then(|_| fs::rename(&temp_path, &target));
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}

pub fn generate_unique_id() -> String {
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let mut bytes = [0u8; 8];
        if file.read_exact(&mut bytes).is_ok() {
            return bytes.iter().map(|b| format!("{:02x}", b)).collect();
        }
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let start = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", start)
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempConfig(PathBuf);

    impl TempConfig {
        fn new(test_name: &str, content: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "gestos-{}-{}.yaml",
                test_name,
                std::process::id()
            ));
            fs::write(&path, content).unwrap();
            TempConfig(path)
        }
    }

    impl Drop for TempConfig {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(test_name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "gestos-migrate-{}-{}",
                test_name,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(path.join("mygestures")).unwrap();
            fs::create_dir_all(path.join("autostart")).unwrap();
            TempDir(path)
        }

        fn migrate(&self) {
            migrate_legacy_user_files(&self.0, Some(&self.0.join("autostart")));
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn migration_copies_config_under_the_new_names_and_keeps_the_old_files() {
        let dir = TempDir::new("copy");
        let old = dir.0.join("mygestures/mygestures_gnome.yaml");
        fs::write(&old, "old config").unwrap();
        fs::write(dir.0.join("mygestures/osd_enabled"), "false").unwrap();
        dir.migrate();

        let new = dir.0.join("gestos/gestos_gnome.yaml");
        assert_eq!(fs::read_to_string(new).unwrap(), "old config");
        assert_eq!(fs::read_to_string(dir.0.join("gestos/osd_enabled")).unwrap(), "false");
        assert!(old.exists());
    }

    #[test]
    fn migration_never_overwrites_a_new_config() {
        let dir = TempDir::new("keep");
        fs::write(dir.0.join("mygestures/mygestures.yaml"), "old").unwrap();
        fs::create_dir_all(dir.0.join("gestos")).unwrap();
        fs::write(dir.0.join("gestos/gestos.yaml"), "new").unwrap();
        dir.migrate();
        assert_eq!(fs::read_to_string(dir.0.join("gestos/gestos.yaml")).unwrap(), "new");
    }

    #[test]
    fn migration_rewrites_autostart_and_keeps_custom_arguments() {
        let dir = TempDir::new("autostart");
        let old = dir.0.join("autostart/mygestures.desktop");
        fs::write(
            &old,
            "[Desktop Entry]\nName=MyGestures Daemon\nExec=/usr/local/bin/mygestures -b 8\nIcon=mygestures\n",
        )
        .unwrap();
        dir.migrate();

        let new = fs::read_to_string(dir.0.join("autostart/gestosd.desktop")).unwrap();
        assert!(new.contains("Exec=/usr/local/bin/gestosd -b 8\n"), "{new}");
        assert!(new.contains("Icon=gestos\n"), "{new}");
        assert!(new.contains("Name=Gestos Daemon\n"), "{new}");
        assert!(!new.contains("mygestures"), "{new}");
        assert!(!old.exists());
    }

    #[test]
    fn migration_without_an_old_setup_creates_nothing() {
        let dir = TempDir::new("none");
        dir.migrate();
        assert!(!dir.0.join("gestos").exists());
        assert!(!dir.0.join("autostart/gestosd.desktop").exists());
    }

    #[test]
    fn migration_can_run_twice() {
        let dir = TempDir::new("twice");
        fs::write(dir.0.join("mygestures/mygestures.yaml"), "old").unwrap();
        dir.migrate();
        fs::write(dir.0.join("gestos/gestos.yaml"), "edited").unwrap();
        dir.migrate();
        assert_eq!(fs::read_to_string(dir.0.join("gestos/gestos.yaml")).unwrap(), "edited");
    }

    #[test]
    fn only_sway_and_hyprland_need_a_manual_autostart_line() {
        assert_eq!(manual_autostart_for(Some("sway")).unwrap().2, "exec gestosd");
        assert_eq!(
            manual_autostart_for(Some("hyprland")).unwrap().2,
            "exec-once = gestosd"
        );
        for desktop in [Some("gnome"), Some("kde"), Some("xfce"), None] {
            assert_eq!(manual_autostart_for(desktop), None);
        }
    }

    #[test]
    fn workspace_icons_point_in_their_direction() {
        assert_eq!(ActionType::WorkspaceLeft.get_icon_name(), "go-previous-symbolic");
        assert_eq!(ActionType::WorkspaceRight.get_icon_name(), "go-next-symbolic");
        assert_eq!(ActionType::WorkspaceUp.get_icon_name(), "go-up-symbolic");
        assert_eq!(ActionType::WorkspaceDown.get_icon_name(), "go-down-symbolic");
    }

    #[test]
    fn parse_points_skips_malformed_tokens() {
        let points = parse_points("0,0 bad 1,x 10,20.5");
        assert_eq!(
            points,
            vec![Point2D { x: 0.0, y: 0.0 }, Point2D { x: 10.0, y: 20.5 }]
        );
    }

    #[test]
    fn load_rejects_invalid_yaml() {
        let file = TempConfig::new("invalid", "global: [unclosed\n  nope: : :");
        assert!(Configuration::load_from_file(&file.0).is_none());
    }

    #[test]
    fn load_rejects_a_missing_file() {
        assert!(Configuration::load_from_file("/nonexistent/gestos.yaml").is_none());
    }

    #[test]
    fn load_reads_gestures_and_marks_abort_as_deleted() {
        let file = TempConfig::new(
            "gestures",
            "global:\n  \"Back\":\n    move: \"100,50 0,50\"\n    do: keypress Alt+Left\n  \"Gone\":\n    move: \"0,0 1,1\"\n    do: abort\n",
        );
        let config = Configuration::load_from_file(&file.0).unwrap();
        assert_eq!(config.gestures.len(), 2);

        let back = config.gestures.iter().find(|g| g.name == "Back").unwrap();
        assert_eq!(back.points.len(), 2);
        assert_eq!(back.actions, vec![ActionType::Keypress("Alt+Left".to_string())]);
        assert!(!back.is_deleted);

        let gone = config.gestures.iter().find(|g| g.name == "Gone").unwrap();
        assert!(gone.is_deleted);
    }

    #[test]
    fn saved_config_loads_back_with_the_same_gestures() {
        let file = TempConfig::new(
            "roundtrip",
            "global:\n  \"Back\":\n    move: \"100,50 0,50\"\n    do: keypress Alt+Left\n  \"Close\":\n    move: \"0,0 0,100\"\n    do: kill\n",
        );
        let original = Configuration::load_from_file(&file.0).unwrap();
        original.save_to_file().unwrap();
        let reloaded = Configuration::load_from_file(&file.0).unwrap();

        assert_eq!(reloaded.gestures.len(), original.gestures.len());
        for gesture in &original.gestures {
            let copy = reloaded
                .gestures
                .iter()
                .find(|g| g.name == gesture.name)
                .unwrap();
            assert_eq!(copy.points, gesture.points);
            assert_eq!(copy.actions, gesture.actions);
        }
    }

    fn gesture(name: &str, actions: Vec<ActionType>) -> Gesture {
        Gesture {
            id: format!("id-{}", name.len()),
            name: name.to_string(),
            raw_movement: "0,0 10,10".to_string(),
            points: parse_points("0,0 10,10"),
            actions,
            is_custom: true,
            is_modified: false,
            is_deleted: false,
            modifiers: Modifiers::default(),
        }
    }

    fn save_and_reload(test_name: &str, gestures: Vec<Gesture>) -> Configuration {
        let file = TempConfig::new(test_name, "");
        let config = Configuration {
            gestures,
            apps: Vec::new(),
            user_config_path: file.0.clone(),
            reduce_notifications: false,
        };
        config.save_to_file().unwrap();
        Configuration::load_from_file(&file.0).expect("saved config must be valid YAML")
    }

    #[test]
    fn only_the_first_action_of_a_list_is_used() {
        let file = TempConfig::new(
            "list",
            "global:
  \"Both\":
    move: \"0,0 9,9\"
    do:
      - kill
      - raise
",
        );
        let config = Configuration::load_from_file(&file.0).unwrap();
        assert_eq!(config.gestures[0].actions, vec![ActionType::Kill]);

        config.save_to_file().unwrap();
        let saved = fs::read_to_string(&file.0).unwrap();
        assert!(saved.contains("do: kill"), "{saved}");
        assert!(!saved.contains("raise"), "{saved}");
    }

    #[test]
    fn saving_keeps_a_gesture_with_no_actions() {
        let reloaded = save_and_reload("none", vec![gesture("Empty", Vec::new())]);
        assert_eq!(reloaded.gestures.len(), 1);
        assert!(reloaded.gestures[0].actions.is_empty());
    }

    #[test]
    fn saving_preserves_yaml_special_characters() {
        let name = "Say \"hi\": it's #1";
        let command = r#"echo "a: b" # not a comment \ 'quoted' [x] {y}"#;
        let actions = vec![ActionType::Execute(command.to_string())];
        let reloaded = save_and_reload("special", vec![gesture(name, actions.clone())]);
        assert_eq!(reloaded.gestures[0].name, name);
        assert_eq!(reloaded.gestures[0].actions, actions);
    }

    #[test]
    fn saving_preserves_ids_and_order() {
        let gestures = vec![
            gesture("Zebra", vec![ActionType::Kill]),
            gesture("A", vec![ActionType::Raise]),
        ];
        let reloaded = save_and_reload("ids", gestures.clone());
        let ids: Vec<_> = reloaded.gestures.iter().map(|g| g.id.clone()).collect();
        assert_eq!(ids, vec![gestures[0].id.clone(), gestures[1].id.clone()]);
        assert_eq!(reloaded.gestures[0].name, "Zebra");
    }

    #[test]
    fn saving_an_empty_config_stays_loadable() {
        assert!(save_and_reload("empty", Vec::new()).gestures.is_empty());
    }

    #[test]
    fn saving_keeps_permissions_and_leaves_no_temp_file() {
        use std::os::unix::fs::PermissionsExt;
        let file = TempConfig::new("perms", "");
        fs::set_permissions(&file.0, fs::Permissions::from_mode(0o600)).unwrap();
        let config = Configuration {
            gestures: vec![gesture("One", vec![ActionType::Kill])],
            apps: Vec::new(),
            user_config_path: file.0.clone(),
            reduce_notifications: false,
        };
        config.save_to_file().unwrap();

        let mode = fs::metadata(&file.0).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let mut temp_name = file.0.clone().into_os_string();
        temp_name.push(format!(".{}.tmp", std::process::id()));
        assert!(!Path::new(&temp_name).exists());
    }

    #[test]
    fn saving_through_a_symlink_keeps_the_symlink() {
        let target = TempConfig::new("link-target", "");
        let link = std::env::temp_dir().join(format!("gestos-link-{}", std::process::id()));
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&target.0, &link).unwrap();
        let config = Configuration {
            gestures: vec![gesture("One", vec![ActionType::Kill])],
            apps: Vec::new(),
            user_config_path: link.clone(),
            reduce_notifications: false,
        };
        config.save_to_file().unwrap();

        let is_link = fs::symlink_metadata(&link).unwrap().file_type().is_symlink();
        let saved = fs::read_to_string(&target.0).unwrap();
        fs::remove_file(&link).unwrap();
        assert!(is_link);
        assert!(saved.contains("One"));
    }

    #[test]
    fn saving_keeps_a_gestures_modifiers() {
        let mut with_modifiers = gesture("Held", vec![ActionType::Kill]);
        with_modifiers.modifiers = Modifiers::parse("ctrl+super");
        let reloaded = save_and_reload(
            "modifiers",
            vec![with_modifiers.clone(), gesture("Plain", vec![ActionType::Raise])],
        );
        assert_eq!(reloaded.gestures[0].modifiers, with_modifiers.modifiers);
        assert!(reloaded.gestures[1].modifiers.is_empty());
        assert!(reloaded.uses_modifiers());
    }

    #[test]
    fn desktop_actions_do_not_show_their_internal_keys() {
        let gnome = gesture("Move window", vec![ActionType::Gnome("move-to-side-n".to_string())]);
        assert_eq!(gnome.action_description(), "");
        let kde = gesture("Overview", vec![ActionType::Kde("kwin".into(), "Overview".into())]);
        assert_eq!(kde.action_description(), "");
        let keys = gesture("Back", vec![ActionType::Keypress("Alt+Left".to_string())]);
        assert_eq!(keys.action_description(), "Press keys: Alt+Left");
    }

    #[test]
    fn a_config_without_modifiers_does_not_use_them() {
        let plain = save_and_reload("plain", vec![gesture("Plain", vec![ActionType::Raise])]);
        assert!(!plain.uses_modifiers());
    }

    #[test]
    fn an_older_config_keeps_its_notifications() {
        let file = TempConfig::new(
            "old-notifications",
            "global:\n  \"Back\":\n    move: \"100,50 0,50\"\n    do: keypress Alt+Left\n",
        );
        let config = Configuration::load_from_file(&file.0).unwrap();
        assert!(!config.reduce_notifications);

        config.save_to_file().unwrap();
        let saved = fs::read_to_string(&file.0).unwrap();
        assert!(!saved.contains("reduce_notifications"), "{saved}");
    }

    #[test]
    fn saving_keeps_reduced_notifications_with_or_without_gestures() {
        for (test_name, gestures) in [
            ("reduce", vec![gesture("One", vec![ActionType::Kill])]),
            ("reduce-empty", Vec::new()),
        ] {
            let file = TempConfig::new(test_name, "");
            let config = Configuration {
                gestures,
                apps: Vec::new(),
                user_config_path: file.0.clone(),
                reduce_notifications: true,
            };
            config.save_to_file().unwrap();
            let reloaded = Configuration::load_from_file(&file.0).unwrap();
            assert!(reloaded.reduce_notifications);
            assert_eq!(reloaded.gestures.len(), config.gestures.len());
        }
    }

    #[test]
    fn the_shipped_config_is_valid_and_has_no_gestures() {
        let file = TempConfig::new("shipped", include_str!("../gestos.yaml"));
        let config = Configuration::load_from_file(&file.0).unwrap();
        assert!(config.gestures.is_empty());
    }
}
