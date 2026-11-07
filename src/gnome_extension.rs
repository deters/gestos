//! What GNOME Shell says about the Gestos extension it runs. GNOME only loads extensions as it
//! starts, so the copy running can be older than this Gestos: until the next login after an
//! update, or for good when an old copy in the user's own folder comes first.

use std::collections::HashMap;
use zbus::zvariant::OwnedValue;

pub const UUID: &str = "gestos@deters.github.io";
const SHELL_NAME: &str = "org.gnome.Shell";
// GNOME Shell offers its extensions interface on its main object.
const EXTENSIONS_PATH: &str = "/org/gnome/Shell";
const EXTENSIONS_INTERFACE: &str = "org.gnome.Shell.Extensions";

// GNOME Shell's ExtensionState.
pub const STATE_ACTIVE: f64 = 1.0;
pub const STATE_ERROR: f64 = 3.0;
pub const STATE_OUT_OF_DATE: f64 = 4.0;
pub const STATE_ACTIVATING: f64 = 8.0;

/// The version the extension has when it comes with this Gestos. It's in its `metadata.json`,
/// which GNOME reports for the copy it loaded.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub type ExtensionInfo = HashMap<String, OwnedValue>;

/// What GNOME knows about the extension, from the Shell on the bus name given. It's empty for an
/// extension GNOME doesn't know.
pub fn info_from(conn: &zbus::blocking::Connection, shell: &str) -> zbus::Result<ExtensionInfo> {
    let reply = conn.call_method(
        Some(shell),
        EXTENSIONS_PATH,
        Some(EXTENSIONS_INTERFACE),
        "GetExtensionInfo",
        &(UUID,),
    )?;
    reply.body().deserialize()
}

pub fn info(conn: &zbus::blocking::Connection) -> zbus::Result<ExtensionInfo> {
    info_from(conn, SHELL_NAME)
}

pub fn value<'a, T: TryFrom<&'a zbus::zvariant::Value<'a>>>(
    info: &'a ExtensionInfo,
    key: &str,
) -> Option<T> {
    info.get(key).and_then(|value| T::try_from(value).ok())
}

pub fn is_running(info: &ExtensionInfo) -> bool {
    matches!(value::<f64>(info, "state"), Some(STATE_ACTIVE | STATE_ACTIVATING))
}

/// Whether the copy GNOME loaded is this Gestos's own. Copies from before extensions said their
/// version have none, so they're never current.
pub fn is_current(info: &ExtensionInfo) -> bool {
    value::<String>(info, "version-name").as_deref() == Some(VERSION)
}

/// The version a `metadata.json` says the extension is. JSON is YAML too.
pub fn metadata_version(metadata: &str) -> Option<String> {
    let value: serde_yaml::Value = serde_yaml::from_str(metadata).ok()?;
    value.get("version-name")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_comes_with_this_version() {
        let metadata = include_str!("../extension/metadata.json");
        assert_eq!(metadata_version(metadata).as_deref(), Some(VERSION));
    }

    #[test]
    fn an_extension_without_a_version_is_not_current() {
        assert!(!is_current(&ExtensionInfo::new()));
        assert_eq!(metadata_version("{\"uuid\": \"x\"}"), None);
    }
}
