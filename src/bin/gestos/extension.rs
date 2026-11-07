//! The Gestos GNOME Shell extension, which the window actions that only GNOME Shell can do need.
//! It's optional: other desktops don't have it, and GNOME works without it.

use gestos::gnome_extension::{
    self as shell, STATE_ACTIVATING, STATE_ACTIVE, STATE_ERROR, STATE_OUT_OF_DATE, UUID, VERSION,
};
use gtk::gio;
use gtk::prelude::FileExt;
use gtk4 as gtk;
use std::path::{Path, PathBuf};

const SCHEMA: &str = "org.gnome.shell.extensions.gestos";
const SHELL_NAME: &str = "org.gnome.Shell";
// GNOME Shell offers its extensions interface on its main object.
const EXTENSIONS_PATH: &str = "/org/gnome/Shell";
const EXTENSIONS_INTERFACE: &str = "org.gnome.Shell.Extensions";

#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionStatus {
    /// Not GNOME, where there's no extension to have.
    Unavailable,
    NotInstalled,
    /// Installed since GNOME started, which only finds extensions as it starts.
    NeedsLogin,
    Off,
    Running,
    /// Running, but a copy from an older Gestos, which can lack what this one needs.
    Outdated(OldCopy),
    Unsupported,
    Failed(String),
}

/// Why GNOME runs an old copy of the extension, which says what updates it.
#[derive(Debug, Clone, PartialEq)]
pub enum OldCopy {
    /// The new copy is installed, and GNOME loads it at the next login.
    Replaced,
    /// An old copy in the user's own folder comes before the new one Gestos installed.
    InUserFolder(PathBuf),
    /// No new copy is installed, as when Gestos was installed without its own.
    NotReplaced,
}

impl ExtensionStatus {
    pub fn is_running(&self) -> bool {
        *self == ExtensionStatus::Running
    }

    /// Whether GNOME knows the extension, so it can be turned on and off.
    pub fn can_switch(&self) -> bool {
        matches!(
            self,
            ExtensionStatus::Off
                | ExtensionStatus::Running
                | ExtensionStatus::Outdated(_)
                | ExtensionStatus::Unsupported
                | ExtensionStatus::Failed(_)
        )
    }

    pub fn description(&self) -> String {
        match self {
            ExtensionStatus::Unavailable => "Only available on GNOME".to_string(),
            ExtensionStatus::NotInstalled => "Not installed".to_string(),
            ExtensionStatus::NeedsLogin => "Log out and back in to start it".to_string(),
            // GNOME keeps the choice, so it says what the next login does too.
            ExtensionStatus::Off => "Off, also after logging in".to_string(),
            ExtensionStatus::Running => "Running".to_string(),
            ExtensionStatus::Outdated(OldCopy::Replaced) => {
                "Updated: log out and back in to use the new version".to_string()
            }
            ExtensionStatus::Outdated(OldCopy::InUserFolder(_)) => {
                "Out of date: an old copy in your home folder is used instead".to_string()
            }
            ExtensionStatus::Outdated(OldCopy::NotReplaced) => {
                "Out of date: reinstall Gestos to update it".to_string()
            }
            ExtensionStatus::Unsupported => "Not compatible with this GNOME version".to_string(),
            ExtensionStatus::Failed(error) if error.is_empty() => "Failed".to_string(),
            ExtensionStatus::Failed(error) => format!("Failed: {error}"),
        }
    }
}

/// The copies of the extension installed, in the order GNOME looks for them: the user's own data
/// folder first, then the system's.
fn installed_copies() -> Vec<PathBuf> {
    std::iter::once(gtk::glib::user_data_dir())
        .chain(gtk::glib::system_data_dirs())
        .map(|dir| dir.join("gnome-shell/extensions").join(UUID))
        .filter(|dir| dir.join("metadata.json").exists())
        .collect()
}

/// Where the extension is installed, whether or not GNOME has loaded it: the copy GNOME loads.
fn installed_path() -> Option<PathBuf> {
    installed_copies().into_iter().next()
}

fn copy_version(copy: &Path) -> Option<String> {
    let metadata = std::fs::read_to_string(copy.join("metadata.json")).ok()?;
    shell::metadata_version(&metadata)
}

pub fn status(conn: Option<&zbus::blocking::Connection>) -> ExtensionStatus {
    if gestos::config::get_environment_suffix() != Some("gnome") {
        return ExtensionStatus::Unavailable;
    }
    let info = conn.and_then(|conn| shell::info(conn).ok()).unwrap_or_default();
    let copies: Vec<(PathBuf, Option<String>)> = installed_copies()
        .into_iter()
        .map(|copy| {
            let version = copy_version(&copy);
            (copy, version)
        })
        .collect();
    status_from(
        shell::value::<f64>(&info, "state"),
        shell::value::<String>(&info, "error"),
        shell::is_current(&info),
        &copies,
        &gtk::glib::user_data_dir(),
    )
}

/// What the status is, from what GNOME says about the copy it runs, and the copies installed with
/// their versions, in the order GNOME looks for them.
fn status_from(
    state: Option<f64>,
    error: Option<String>,
    current: bool,
    copies: &[(PathBuf, Option<String>)],
    user_folder: &Path,
) -> ExtensionStatus {
    match state {
        // GNOME doesn't know an extension installed since it started.
        None if !copies.is_empty() => ExtensionStatus::NeedsLogin,
        None => ExtensionStatus::NotInstalled,
        Some(STATE_ACTIVE | STATE_ACTIVATING) if current => ExtensionStatus::Running,
        Some(STATE_ACTIVE | STATE_ACTIVATING) => {
            ExtensionStatus::Outdated(old_copy(copies, user_folder))
        }
        Some(STATE_OUT_OF_DATE) => ExtensionStatus::Unsupported,
        Some(STATE_ERROR) => ExtensionStatus::Failed(error.unwrap_or_default()),
        Some(_) => ExtensionStatus::Off,
    }
}

/// What the next login loads tells how the old copy running gets updated.
fn old_copy(copies: &[(PathBuf, Option<String>)], user_folder: &Path) -> OldCopy {
    let is_new = |(_, version): &&(PathBuf, Option<String>)| version.as_deref() == Some(VERSION);
    match copies.first() {
        Some(next) if is_new(&next) => OldCopy::Replaced,
        Some((next, _)) if next.starts_with(user_folder) && copies.iter().any(|c| is_new(&c)) => {
            OldCopy::InUserFolder(next.clone())
        }
        _ => OldCopy::NotReplaced,
    }
}

/// Moves an old copy of the extension to the trash, so that the one Gestos installed loads at the
/// next login instead.
pub fn trash_copy(copy: &Path) -> Result<(), gtk::glib::Error> {
    gio::File::for_path(copy).trash(None::<&gio::Cancellable>)
}

/// Asks GNOME to log out, which first asks the user to confirm, as its own menu does.
pub fn log_out(conn: &zbus::blocking::Connection) -> zbus::Result<()> {
    conn.call_method(
        Some("org.gnome.SessionManager"),
        "/org/gnome/SessionManager",
        Some("org.gnome.SessionManager"),
        "Logout",
        &(0u32,),
    )?;
    Ok(())
}

/// Turns the extension on or off, as the Extensions app does.
pub fn set_enabled(conn: &zbus::blocking::Connection, enabled: bool) -> zbus::Result<()> {
    conn.call_method(
        Some(SHELL_NAME),
        EXTENSIONS_PATH,
        Some(EXTENSIONS_INTERFACE),
        if enabled { "EnableExtension" } else { "DisableExtension" },
        &(UUID,),
    )?;
    Ok(())
}

/// The extension's own settings. Extensions keep their settings schema with them, so this is only
/// there while it's installed.
pub fn settings() -> Option<gio::Settings> {
    let schemas = installed_path()?.join("schemas");
    let source = gio::SettingsSchemaSource::from_directory(
        &schemas,
        gio::SettingsSchemaSource::default().as_ref(),
        false,
    )
    .ok()?;
    let schema = source.lookup(SCHEMA, false)?;
    Some(gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> PathBuf {
        PathBuf::from("/home/me/.local/share")
    }

    fn copy(dir: &str, version: Option<&str>) -> (PathBuf, Option<String>) {
        (PathBuf::from(dir).join(UUID), version.map(str::to_string))
    }

    fn installed() -> Vec<(PathBuf, Option<String>)> {
        vec![copy("/usr/share/gnome-shell/extensions", Some(VERSION))]
    }

    #[test]
    fn status_follows_what_gnome_says() {
        let status = |state, error: Option<&str>| {
            status_from(state, error.map(str::to_string), true, &installed(), &user())
        };
        assert_eq!(status(Some(1.0), None), ExtensionStatus::Running);
        assert_eq!(status(Some(2.0), None), ExtensionStatus::Off);
        assert_eq!(status(Some(4.0), None), ExtensionStatus::Unsupported);
        assert_eq!(status(Some(3.0), Some("oops")), ExtensionStatus::Failed("oops".to_string()));
    }

    #[test]
    fn an_extension_gnome_doesnt_know_needs_a_login_if_installed() {
        assert_eq!(
            status_from(None, None, false, &installed(), &user()),
            ExtensionStatus::NeedsLogin
        );
        assert_eq!(status_from(None, None, false, &[], &user()), ExtensionStatus::NotInstalled);
    }

    #[test]
    fn an_old_copy_running_says_what_updates_it() {
        let outdated = |copies: &[(PathBuf, Option<String>)]| {
            status_from(Some(1.0), None, false, copies, &user())
        };
        assert_eq!(outdated(&installed()), ExtensionStatus::Outdated(OldCopy::Replaced));
        let mine = copy("/home/me/.local/share/gnome-shell/extensions", None);
        let shadowed = [mine.clone(), installed()[0].clone()];
        assert_eq!(outdated(&shadowed), ExtensionStatus::Outdated(OldCopy::InUserFolder(mine.0)));
        let old = [copy("/usr/share/gnome-shell/extensions", Some("1.0"))];
        assert_eq!(outdated(&old), ExtensionStatus::Outdated(OldCopy::NotReplaced));
    }

    #[test]
    fn only_a_known_extension_can_be_switched() {
        assert!(ExtensionStatus::Off.can_switch());
        assert!(ExtensionStatus::Running.can_switch());
        assert!(!ExtensionStatus::NotInstalled.can_switch());
        assert!(!ExtensionStatus::NeedsLogin.can_switch());
    }
}
