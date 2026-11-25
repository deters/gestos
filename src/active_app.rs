//! Which app is in use, for gestures limited to some apps, on desktops without the GNOME
//! extension. On X11 any program can ask the X server which window is active, on KDE KWin runs a
//! script that says, and Sway and Hyprland say when asked. Apps are named by their desktop file ID, as on GNOME, so a gesture
//! works wherever the app can be told.

use gio::prelude::*;

/// Whether this session is X11, where the active window can be asked for.
pub fn is_x11_session() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|session| session == "x11")
        && std::env::var_os("DISPLAY").is_some()
}

/// Whether this is KDE's Plasma, whose KWin can say which window is active.
pub fn is_kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| desktop.to_uppercase().contains("KDE"))
}

/// Whether this is Sway or Hyprland, which say which window has focus.
pub fn is_sway_or_hyprland() -> bool {
    std::env::var_os("SWAYSOCK").is_some() || std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

/// Whether the app in use can be told here without the GNOME extension.
pub fn can_tell_without_extension() -> bool {
    is_kde() || is_x11_session() || is_sway_or_hyprland()
}

/// The app of KWin's active window, by its desktop file ID. `None` means KWin couldn't be asked;
/// `Some(None)` that no window is active, or its app isn't installed.
pub fn kde_active_app() -> Option<Option<String>> {
    let (app, pid) = kwin::active_app().ok()?;
    if app.is_empty() {
        return Some(None);
    }
    // KWin names a window by its desktop file, if it knows it, or else by its class.
    let window = WindowIds { gtk_app_id: Some(app.clone()), classes: vec![app], pid };
    Some(app_for(&window))
}

/// The app GNOME's extension says is in use: its desktop file ID, or for a window no installed app
/// has, its class, which may be a program's.
pub fn gnome_app(reported: String) -> String {
    let window = WindowIds { gtk_app_id: None, classes: vec![reported.clone()], pid: None };
    crate::programs::matching(&window).unwrap_or(reported)
}

/// The app a window is, as gestures name it: a program turned on that it is, first, as the user
/// chose that, or else the installed app it belongs to.
fn app_for(window: &WindowIds) -> Option<String> {
    crate::programs::matching(window).or_else(|| desktop_id_for(window, &installed_apps()))
}

/// The app of the active window on X11, by its desktop file ID. `None` means the X server
/// couldn't be asked; `Some(None)` that no window is active, or its app isn't installed.
pub fn x11_active_app() -> Option<Option<String>> {
    let window = x11::active_window().ok()?;
    Some(window.and_then(|window| app_for(&window)))
}

/// Calls `changed` with the app of the active window on X11 when watching starts, and then each
/// time another becomes active, in a thread of its own, for as long as the X server runs.
pub fn watch_x11_active_app(changed: impl Fn(Option<String>) + Send + 'static) {
    std::thread::spawn(move || {
        if let Err(e) = x11::watch_active_window(&|| changed(x11_active_app().flatten())) {
            eprintln!("gestos: Stopped watching the active window: {e}");
        }
    });
}

/// The app of Sway's focused window, from its tree as `swaymsg -t get_tree` prints it. `None` means
/// the tree couldn't be read; `Some(None)` that no window has focus, or its app isn't installed.
pub fn sway_active_app(tree: &str) -> Option<Option<String>> {
    Some(sway_window(tree)?.and_then(|window| app_for(&window)))
}

fn sway_window(tree: &str) -> Option<Option<WindowIds>> {
    let tree: serde_json::Value = serde_json::from_str(tree).ok()?;
    Some(focused_sway_node(&tree).map(|node| {
        let properties = &node["window_properties"];
        window_ids(node["app_id"].as_str(), &[&properties["instance"], &properties["class"]], &node["pid"])
    }))
}

/// The window in Sway's tree that has focus, tiled or floating.
fn focused_sway_node(node: &serde_json::Value) -> Option<&serde_json::Value> {
    let window = node["type"] == "con" || node["type"] == "floating_con";
    if window && node["focused"].as_bool() == Some(true) {
        return Some(node);
    }
    ["nodes", "floating_nodes"]
        .iter()
        .filter_map(|children| node[children].as_array())
        .flatten()
        .find_map(focused_sway_node)
}

/// The app of Hyprland's active window, from `hyprctl activewindow -j`. `None` means it couldn't
/// be read; `Some(None)` that no window is active, or its app isn't installed.
pub fn hyprland_active_app(window: &str) -> Option<Option<String>> {
    Some(hyprland_window(window)?.and_then(|window| app_for(&window)))
}

fn hyprland_window(window: &str) -> Option<Option<WindowIds>> {
    // With no window active, Hyprland prints `{}`, or "Invalid" in older versions.
    let Ok(window) = serde_json::from_str::<serde_json::Value>(window) else {
        return window.trim().eq_ignore_ascii_case("invalid").then_some(None);
    };
    let class = window["class"].as_str().filter(|class| !class.is_empty());
    Some(class.map(|class| window_ids(Some(class), &[&window["initialClass"]], &window["pid"])))
}

/// A Wayland window's app ID is its desktop file ID, as a GTK app's is, and its class as well.
fn window_ids(app_id: Option<&str>, classes: &[&serde_json::Value], pid: &serde_json::Value) -> WindowIds {
    let app_id = app_id.filter(|id| !id.is_empty()).map(str::to_string);
    let classes = app_id
        .iter()
        .cloned()
        .chain(classes.iter().filter_map(|class| class.as_str()).map(str::to_string))
        .collect();
    let pid = pid.as_i64().and_then(|pid| i32::try_from(pid).ok()).filter(|pid| *pid > 0);
    WindowIds { gtk_app_id: app_id, classes, pid }
}

/// What an X11 window says about the app it belongs to.
#[derive(Debug, Default, PartialEq)]
pub struct WindowIds {
    /// A GTK app's own ID, which is its desktop file ID.
    pub gtk_app_id: Option<String>,
    /// Its WM_CLASS: the instance, then the class.
    pub classes: Vec<String>,
    /// Its process, as seen from outside any sandbox, if that's known.
    pub pid: Option<i32>,
}

/// An installed app, as far as telling its windows goes.
#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub id: String,
    pub startup_wm_class: Option<String>,
    pub program: Option<String>,
}

/// Kept until an app is installed or removed, as the daemon asks for every gesture.
fn installed_apps() -> Vec<InstalledApp> {
    static APPS: crate::app_cache::AppCache<Vec<InstalledApp>> = crate::app_cache::AppCache::new();
    APPS.get(read_installed_apps)
}

fn read_installed_apps() -> Vec<InstalledApp> {
    gio::AppInfo::all()
        .into_iter()
        .filter_map(|info| {
            let id = info.id()?.trim_end_matches(".desktop").to_string();
            let startup_wm_class = info
                .downcast_ref::<gio::DesktopAppInfo>()
                .and_then(|desktop| desktop.startup_wm_class())
                .map(|class| class.to_string());
            // Not executable(), which panics for an app that doesn't have one.
            let program = info.commandline().and_then(|line| {
                let line = line.to_string_lossy().into_owned();
                let program = line.split_whitespace().next()?;
                Some(program.rsplit('/').next().unwrap_or(program).to_string())
            });
            Some(InstalledApp { id, startup_wm_class, program })
        })
        .collect()
}

/// The installed app a window belongs to, by the most certain of what it says: its GTK app ID,
/// then the class its desktop file says its windows have, then its desktop file ID or program
/// named like its class.
pub fn desktop_id_for(window: &WindowIds, apps: &[InstalledApp]) -> Option<String> {
    if let Some(app) = &window.gtk_app_id {
        if apps.iter().any(|installed| &installed.id == app) {
            return Some(app.clone());
        }
    }
    let named = |name: &str| window.classes.iter().any(|class| class.eq_ignore_ascii_case(name));
    let by = |matches: &dyn Fn(&InstalledApp) -> bool| {
        apps.iter().find(|app| matches(app)).map(|app| app.id.clone())
    };
    by(&|app| app.startup_wm_class.as_deref().is_some_and(named))
        .or_else(|| by(&|app| named(&app.id) || app.id.rsplit('.').next().is_some_and(named)))
        .or_else(|| by(&|app| app.program.as_deref().is_some_and(named)))
}

mod x11 {
    use super::WindowIds;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};

    type Error = Box<dyn std::error::Error>;

    /// What the active window says about its app, or `None` with no window active.
    pub fn active_window() -> Result<Option<WindowIds>, Error> {
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let atom = |name: &str| -> Result<u32, Error> {
            Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
        };
        let active = conn
            .get_property(false, root, atom("_NET_ACTIVE_WINDOW")?, AtomEnum::WINDOW, 0, 1)?
            .reply()?
            .value32()
            .and_then(|mut values| values.next())
            .filter(|window| *window != 0);
        let Some(window) = active else { return Ok(None) };
        let text = |window: Window, property: u32, kind: u32| -> Result<Vec<u8>, Error> {
            Ok(conn.get_property(false, window, property, kind, 0, 1024)?.reply()?.value)
        };
        let gtk_app_id = text(window, atom("_GTK_APPLICATION_ID")?, atom("UTF8_STRING")?)?;
        let classes = text(window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())?;
        // The X server's own answer first, as one in a sandbox, like a Flatpak's, numbers its
        // process as the sandbox does.
        let pid = server_pid(&conn, window).or_else(|| {
            conn.get_property(false, window, atom("_NET_WM_PID").ok()?, AtomEnum::CARDINAL, 0, 1)
                .ok()?
                .reply()
                .ok()?
                .value32()?
                .next()
                .and_then(|pid| i32::try_from(pid).ok())
                .filter(|pid| *pid > 0)
        });
        Ok(Some(WindowIds {
            pid,
            gtk_app_id: (!gtk_app_id.is_empty())
                .then(|| String::from_utf8_lossy(&gtk_app_id).into_owned()),
            classes: classes
                .split(|byte| *byte == 0)
                .filter(|class| !class.is_empty())
                .map(|class| String::from_utf8_lossy(class).into_owned())
                .collect(),
        }))
    }

    /// Calls `changed` once, and then each time the root window says another window is active.
    pub fn watch_active_window(changed: &dyn Fn()) -> Result<(), Error> {
        use x11rb::protocol::xproto::{ChangeWindowAttributesAux, EventMask};
        use x11rb::protocol::Event;
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let active = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?.atom;
        conn.change_window_attributes(root, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE))?
            .check()?;
        changed();
        loop {
            if let Event::PropertyNotify(event) = conn.wait_for_event()? {
                if event.atom == active {
                    changed();
                }
            }
        }
    }

    /// The process of the client that made a window, as the X server knows it from its
    /// connection, where it has the X-Resource extension.
    fn server_pid(conn: &impl Connection, window: Window) -> Option<i32> {
        use x11rb::protocol::res::{ClientIdMask, ClientIdSpec, ConnectionExt as _};
        let spec = ClientIdSpec { client: window, mask: ClientIdMask::LOCAL_CLIENT_PID };
        let reply = conn.res_query_client_ids(&[spec]).ok()?.reply().ok()?;
        let id = reply.ids.into_iter().find(|id| id.spec.mask.contains(ClientIdMask::LOCAL_CLIENT_PID))?;
        id.value.first().and_then(|pid| i32::try_from(*pid).ok()).filter(|pid| *pid > 0)
    }
}

pub(crate) mod kwin {
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::time::Duration;

    type Error = Box<dyn std::error::Error>;

    const KWIN: &str = "org.kde.KWin";
    const SCRIPTING_PATH: &str = "/Scripting";
    const SCRIPTING: &str = "org.kde.kwin.Scripting";
    const REPORT_PATH: &str = "/io/github/deters/Gestos/ActiveApp";
    const REPORT_INTERFACE: &str = "io.github.deters.Gestos.ActiveApp";
    // How long KWin gets to run the script and say.
    const ANSWER_TIMEOUT: Duration = Duration::from_millis(500);

    /// Where the script's answer arrives.
    struct Report(Mutex<mpsc::Sender<String>>);

    #[zbus::interface(name = "io.github.deters.Gestos.ActiveApp")]
    impl Report {
        fn report(&self, app: String) {
            let _ = self.0.lock().map(|sender| sender.send(app));
        }
    }

    /// The script, which says the active window's desktop file, or its class, and on the next line
    /// its process, to this connection. Plasma 6 calls windows what Plasma 5 called clients.
    fn script(reply_to: &str) -> String {
        format!(
            "const window = workspace.activeWindow || workspace.activeClient;\n\
             const app = window ? (window.desktopFileName || window.resourceClass || \"\") : \"\";\n\
             const pid = window && window.pid > 0 ? window.pid : \"\";\n\
             callDBus(\"{reply_to}\", \"{REPORT_PATH}\", \"{REPORT_INTERFACE}\", \"Report\", String(app) + \"\\n\" + pid);\n"
        )
    }

    /// What KWin says the active window's app is, or an empty name for none, and its process.
    pub fn active_app() -> Result<(String, Option<i32>), Error> {
        let conn = zbus::blocking::Connection::session()?;
        let reply_to = conn.unique_name().ok_or("no name on the bus")?.to_string();
        let (sender, answers) = mpsc::channel();
        conn.object_server().at(REPORT_PATH, Report(Mutex::new(sender)))?;

        let name = format!("gestos-active-app-{}", std::process::id());
        let answer = load_script(&conn, &name, &script(&reply_to))
            .and_then(|_| Ok(answers.recv_timeout(ANSWER_TIMEOUT)?));
        unload_script(&conn, &name);
        Ok(parse_answer(&answer?))
    }

    /// The app and process the script says, on a line each.
    fn parse_answer(answer: &str) -> (String, Option<i32>) {
        let (app, pid) = answer.split_once('\n').unwrap_or((answer, ""));
        (app.to_string(), pid.trim().parse().ok().filter(|pid| *pid > 0))
    }

    /// Runs a script in KWin, by a name of its own, until it's unloaded.
    pub(crate) fn load_script(conn: &zbus::blocking::Connection, name: &str, script: &str) -> Result<(), Error> {
        let path = script_path(name)?;
        std::fs::write(&path, script)?;
        run_script(conn, &path, name)
    }

    /// Stops a script, and removes its file, which KWin may read only once it runs.
    pub(crate) fn unload_script(conn: &zbus::blocking::Connection, name: &str) {
        let _ = conn.call_method(Some(KWIN), SCRIPTING_PATH, Some(SCRIPTING), "unloadScript", &(name,));
        if let Ok(path) = script_path(name) {
            let _ = std::fs::remove_file(path);
        }
    }

    fn script_path(name: &str) -> Result<std::path::PathBuf, Error> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or("no runtime folder")?;
        Ok(std::path::Path::new(&dir).join(format!("{name}.js")))
    }

    fn run_script(
        conn: &zbus::blocking::Connection,
        path: &std::path::Path,
        name: &str,
    ) -> Result<(), Error> {
        let path = path.to_string_lossy();
        // A script left loaded by a run that stopped halfway would keep the name taken.
        let _ = conn.call_method(Some(KWIN), SCRIPTING_PATH, Some(SCRIPTING), "unloadScript", &(name,));
        let id: i32 = conn
            .call_method(Some(KWIN), SCRIPTING_PATH, Some(SCRIPTING), "loadScript", &(&*path, name))?
            .body()
            .deserialize()?;
        // Plasma 6 has each script at its own path under Scripting, Plasma 5 at the top.
        for script_path in [format!("{SCRIPTING_PATH}/Script{id}"), format!("/{id}")] {
            if conn
                .call_method(Some(KWIN), script_path.as_str(), Some("org.kde.kwin.Script"), "run", &())
                .is_ok()
            {
                return Ok(());
            }
        }
        Err("KWin didn't run the script".into())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn the_script_answers_this_connection() {
            let script = super::script(":1.42");
            assert!(script.contains("callDBus(\":1.42\", \"/io/github/deters/Gestos/ActiveApp\""));
            assert!(script.contains("workspace.activeWindow || workspace.activeClient"));
        }

        #[test]
        fn the_answer_is_the_app_then_its_process() {
            assert_eq!(super::parse_answer("org.kde.dolphin\n4242"), ("org.kde.dolphin".to_string(), Some(4242)));
            assert_eq!(super::parse_answer("steam_app_5\n"), ("steam_app_5".to_string(), None));
            assert_eq!(super::parse_answer("\n"), (String::new(), None));
            assert_eq!(super::parse_answer("old"), ("old".to_string(), None));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, startup_wm_class: Option<&str>, program: &str) -> InstalledApp {
        InstalledApp {
            id: id.to_string(),
            startup_wm_class: startup_wm_class.map(str::to_string),
            program: Some(program.to_string()),
        }
    }

    fn window(gtk_app_id: Option<&str>, classes: &[&str]) -> WindowIds {
        WindowIds {
            gtk_app_id: gtk_app_id.map(str::to_string),
            classes: classes.iter().map(|class| class.to_string()).collect(),
            pid: None,
        }
    }

    #[test]
    fn a_window_is_told_by_what_it_says_most_certainly() {
        let apps = [
            app("org.gnome.Nautilus", None, "nautilus"),
            app("nemo", None, "nemo"),
            app("google-chrome", Some("Google-chrome"), "google-chrome-stable"),
            app("org.mozilla.firefox", None, "firefox"),
            app("code", Some("Code"), "code"),
        ];
        let id = |window: WindowIds| desktop_id_for(&window, &apps);
        assert_eq!(id(window(Some("org.gnome.Nautilus"), &["nautilus"])).as_deref(), Some("org.gnome.Nautilus"));
        assert_eq!(id(window(None, &["nemo", "Nemo"])).as_deref(), Some("nemo"));
        assert_eq!(id(window(None, &["google-chrome", "Google-chrome"])).as_deref(), Some("google-chrome"));
        assert_eq!(id(window(None, &["Navigator", "firefox"])).as_deref(), Some("org.mozilla.firefox"));
        assert_eq!(id(window(None, &["code", "Code"])).as_deref(), Some("code"));
        assert_eq!(id(window(None, &["xterm", "XTerm"])), None);
    }

    #[test]
    fn sways_focused_window_is_found_tiled_or_floating() {
        let tree = r#"{"type": "root", "focused": false, "nodes": [{"type": "output", "nodes": [
            {"type": "workspace", "focused": false,
             "nodes": [{"type": "con", "focused": false, "app_id": "foot"}],
             "floating_nodes": [{"type": "floating_con", "focused": true, "app_id": null,
                                 "window_properties": {"class": "Google-chrome", "instance": "google-chrome"}}]}]}]}"#;
        assert_eq!(sway_window(tree), Some(Some(window(None, &["google-chrome", "Google-chrome"]))));
        let wayland = tree.replace(r#""focused": true"#, r#""focused": false"#).replacen(
            r#""focused": false, "app_id": "foot""#,
            r#""focused": true, "app_id": "foot""#,
            1,
        );
        assert_eq!(sway_window(&wayland), Some(Some(window(Some("foot"), &["foot"]))));
    }

    #[test]
    fn a_focused_sway_workspace_is_no_window() {
        let tree = r#"{"type": "root", "nodes": [{"type": "output", "nodes": [{"type": "workspace", "focused": true, "nodes": []}]}]}"#;
        assert_eq!(sway_window(tree), Some(None));
        assert_eq!(sway_window("not json"), None);
    }

    #[test]
    fn hyprlands_active_window_is_told_by_its_class() {
        let window_json = r#"{"class": "org.gnome.Nautilus", "initialClass": "org.gnome.Nautilus", "title": "Home"}"#;
        assert_eq!(
            hyprland_window(window_json),
            Some(Some(window(Some("org.gnome.Nautilus"), &["org.gnome.Nautilus", "org.gnome.Nautilus"])))
        );
        assert_eq!(hyprland_window("{}"), Some(None));
        assert_eq!(hyprland_window("Invalid\n"), Some(None));
        assert_eq!(hyprland_window("HYPRLAND_INSTANCE_SIGNATURE not set"), None);
    }

    #[test]
    fn sway_and_hyprland_say_a_windows_process() {
        let game = r#"{"class": "steam_app_5", "initialClass": "steam_app_5", "pid": 4242}"#;
        assert_eq!(hyprland_window(game).flatten().and_then(|window| window.pid), Some(4242));
        let unknown = r#"{"class": "foot", "pid": -1}"#;
        assert_eq!(hyprland_window(unknown).flatten().and_then(|window| window.pid), None);
        let tree = r#"{"type": "con", "focused": true, "app_id": "foot", "pid": 77}"#;
        assert_eq!(sway_window(tree).flatten().and_then(|window| window.pid), Some(77));
    }
}
