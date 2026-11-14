//! Which app is in use, for gestures limited to some apps, on desktops without the GNOME
//! extension. On X11 any program can ask the X server which window is active, and on KDE KWin
//! runs a script that says. Apps are named by their desktop file ID, as on GNOME, so a gesture
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

/// Whether the app in use can be told here without the GNOME extension.
pub fn can_tell_without_extension() -> bool {
    is_kde() || is_x11_session()
}

/// The app of KWin's active window, by its desktop file ID. `None` means KWin couldn't be asked;
/// `Some(None)` that no window is active, or its app isn't installed.
pub fn kde_active_app() -> Option<Option<String>> {
    let app = kwin::active_app().ok()?;
    if app.is_empty() {
        return Some(None);
    }
    let apps = installed_apps();
    // KWin names a window by its desktop file, if it knows it, or else by its class.
    let window = WindowIds { gtk_app_id: Some(app.clone()), classes: vec![app] };
    Some(desktop_id_for(&window, &apps))
}

/// The app of the active window on X11, by its desktop file ID. `None` means the X server
/// couldn't be asked; `Some(None)` that no window is active, or its app isn't installed.
pub fn x11_active_app() -> Option<Option<String>> {
    let window = x11::active_window().ok()?;
    Some(window.and_then(|window| desktop_id_for(&window, &installed_apps())))
}

/// What an X11 window says about the app it belongs to.
#[derive(Debug, Default, PartialEq)]
pub struct WindowIds {
    /// A GTK app's own ID, which is its desktop file ID.
    pub gtk_app_id: Option<String>,
    /// Its WM_CLASS: the instance, then the class.
    pub classes: Vec<String>,
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
        Ok(Some(WindowIds {
            gtk_app_id: (!gtk_app_id.is_empty())
                .then(|| String::from_utf8_lossy(&gtk_app_id).into_owned()),
            classes: classes
                .split(|byte| *byte == 0)
                .filter(|class| !class.is_empty())
                .map(|class| String::from_utf8_lossy(class).into_owned())
                .collect(),
        }))
    }
}

mod kwin {
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

    /// The script, which says the active window's desktop file, or its class, to this
    /// connection. Plasma 6 calls windows what Plasma 5 called clients.
    fn script(reply_to: &str) -> String {
        format!(
            "const window = workspace.activeWindow || workspace.activeClient;\n\
             const app = window ? (window.desktopFileName || window.resourceClass || \"\") : \"\";\n\
             callDBus(\"{reply_to}\", \"{REPORT_PATH}\", \"{REPORT_INTERFACE}\", \"Report\", String(app));\n"
        )
    }

    /// What KWin says the active window's app is, or an empty name for none.
    pub fn active_app() -> Result<String, Error> {
        let conn = zbus::blocking::Connection::session()?;
        let reply_to = conn.unique_name().ok_or("no name on the bus")?.to_string();
        let (sender, answers) = mpsc::channel();
        conn.object_server().at(REPORT_PATH, Report(Mutex::new(sender)))?;

        let name = format!("gestos-active-app-{}", std::process::id());
        let dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or("no runtime folder")?;
        let path = std::path::Path::new(&dir).join(format!("{name}.js"));
        std::fs::write(&path, script(&reply_to))?;
        let answer = run_script(&conn, &path, &name)
            .and_then(|_| Ok(answers.recv_timeout(ANSWER_TIMEOUT)?));
        let _ = conn.call_method(Some(KWIN), SCRIPTING_PATH, Some(SCRIPTING), "unloadScript", &(&name,));
        let _ = std::fs::remove_file(&path);
        answer
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
}
