//! The Gestos GNOME Shell extension, which the window actions that only GNOME Shell can do need.
//! It's optional: other desktops don't have it, and GNOME works without it.

use gtk::gio;
use gtk4 as gtk;
use std::collections::HashMap;
use std::path::PathBuf;
use zbus::zvariant::OwnedValue;

const UUID: &str = "gestos@deters.github.io";
const SCHEMA: &str = "org.gnome.shell.extensions.gestos";
const SHELL_NAME: &str = "org.gnome.Shell";
// GNOME Shell offers its extensions interface on its main object.
const EXTENSIONS_PATH: &str = "/org/gnome/Shell";
const EXTENSIONS_INTERFACE: &str = "org.gnome.Shell.Extensions";

// GNOME Shell's ExtensionState.
const STATE_ACTIVE: f64 = 1.0;
const STATE_ERROR: f64 = 3.0;
const STATE_OUT_OF_DATE: f64 = 4.0;
const STATE_ACTIVATING: f64 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionStatus {
    /// Not GNOME, where there's no extension to have.
    Unavailable,
    NotInstalled,
    /// Installed since GNOME started, which only finds extensions as it starts.
    NeedsLogin,
    Off,
    Running,
    Unsupported,
    Failed(String),
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
            ExtensionStatus::Unsupported => "Not compatible with this GNOME version".to_string(),
            ExtensionStatus::Failed(error) if error.is_empty() => "Failed".to_string(),
            ExtensionStatus::Failed(error) => format!("Failed: {error}"),
        }
    }
}

fn extension_info(conn: &zbus::blocking::Connection) -> zbus::Result<HashMap<String, OwnedValue>> {
    let reply = conn.call_method(
        Some(SHELL_NAME),
        EXTENSIONS_PATH,
        Some(EXTENSIONS_INTERFACE),
        "GetExtensionInfo",
        &(UUID,),
    )?;
    reply.body().deserialize()
}

fn info_value<'a, T: TryFrom<&'a zbus::zvariant::Value<'a>>>(
    info: &'a HashMap<String, OwnedValue>,
    key: &str,
) -> Option<T> {
    info.get(key).and_then(|value| T::try_from(value).ok())
}

/// Where the extension is installed, whether or not GNOME has loaded it. A copy in the user's data
/// folder comes first, as it does for GNOME.
fn installed_path() -> Option<PathBuf> {
    let user = gtk::glib::user_data_dir();
    std::iter::once(user)
        .chain(gtk::glib::system_data_dirs())
        .map(|dir| dir.join("gnome-shell/extensions").join(UUID))
        .find(|dir| dir.join("metadata.json").exists())
}

pub fn status(conn: Option<&zbus::blocking::Connection>) -> ExtensionStatus {
    if gestos::config::get_environment_suffix() != Some("gnome") {
        return ExtensionStatus::Unavailable;
    }
    let info = conn.and_then(|conn| extension_info(conn).ok()).unwrap_or_default();
    status_from(
        info_value::<f64>(&info, "state"),
        info_value::<String>(&info, "error"),
        installed_path().is_some(),
    )
}

fn status_from(state: Option<f64>, error: Option<String>, installed: bool) -> ExtensionStatus {
    match state {
        // GNOME doesn't know an extension installed since it started.
        None if installed => ExtensionStatus::NeedsLogin,
        None => ExtensionStatus::NotInstalled,
        Some(STATE_ACTIVE | STATE_ACTIVATING) => ExtensionStatus::Running,
        Some(STATE_OUT_OF_DATE) => ExtensionStatus::Unsupported,
        Some(STATE_ERROR) => ExtensionStatus::Failed(error.unwrap_or_default()),
        Some(_) => ExtensionStatus::Off,
    }
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

    #[test]
    fn status_follows_what_gnome_says() {
        assert_eq!(status_from(Some(1.0), None, true), ExtensionStatus::Running);
        assert_eq!(status_from(Some(2.0), None, true), ExtensionStatus::Off);
        assert_eq!(status_from(Some(4.0), None, true), ExtensionStatus::Unsupported);
        assert_eq!(
            status_from(Some(3.0), Some("oops".to_string()), true),
            ExtensionStatus::Failed("oops".to_string())
        );
    }

    #[test]
    fn an_extension_gnome_doesnt_know_needs_a_login_if_installed() {
        assert_eq!(status_from(None, None, true), ExtensionStatus::NeedsLogin);
        assert_eq!(status_from(None, None, false), ExtensionStatus::NotInstalled);
    }

    #[test]
    fn only_a_known_extension_can_be_switched() {
        assert!(ExtensionStatus::Off.can_switch());
        assert!(ExtensionStatus::Running.can_switch());
        assert!(!ExtensionStatus::NotInstalled.can_switch());
        assert!(!ExtensionStatus::NeedsLogin.can_switch());
    }
}
