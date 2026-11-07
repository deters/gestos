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

/// A fingerprint of the code of the extension that comes with this Gestos, which only changes when
/// the extension does. Installed copies have it in their `metadata.json`. It's empty if the build
/// couldn't make one.
pub const FINGERPRINT: &str = env!("GESTOS_EXTENSION_FINGERPRINT");

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

/// Whether the copy GNOME loaded has the code this Gestos's own has, even if it came with an older
/// Gestos, which doesn't need updating then.
pub fn is_current(info: &ExtensionInfo) -> bool {
    has_this_code(
        value::<String>(info, "gestos-fingerprint").as_deref(),
        value::<String>(info, "version-name").as_deref(),
    )
}

/// The same, for a copy's `metadata.json`. JSON is YAML too.
pub fn metadata_is_current(metadata: &str) -> bool {
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(metadata) else {
        return false;
    };
    let field = |key: &str| value.get(key).and_then(|field| field.as_str());
    has_this_code(field("gestos-fingerprint"), field("version-name"))
}

/// Copies from before the fingerprint have none, and copies from before the version have neither,
/// so they're never current. Without a fingerprint of its own, Gestos goes by the version.
fn has_this_code(fingerprint: Option<&str>, version: Option<&str>) -> bool {
    has_this_code_as(FINGERPRINT, fingerprint, version)
}

fn has_this_code_as(own: &str, fingerprint: Option<&str>, version: Option<&str>) -> bool {
    if own.is_empty() {
        version == Some(VERSION)
    } else {
        fingerprint == Some(own)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_comes_with_this_version() {
        let metadata: serde_yaml::Value =
            serde_yaml::from_str(include_str!("../extension/metadata.json")).unwrap();
        assert_eq!(metadata["version-name"].as_str(), Some(VERSION));
    }

    #[test]
    fn the_fingerprint_is_built_in() {
        assert_eq!(FINGERPRINT.len(), 16, "{FINGERPRINT:?}");
    }

    #[test]
    fn the_same_code_is_current_whatever_the_version() {
        assert!(has_this_code_as("abc", Some("abc"), Some("1.0")));
        assert!(!has_this_code_as("abc", Some("def"), Some(VERSION)));
        // A copy from before the fingerprint.
        assert!(!has_this_code_as("abc", None, Some(VERSION)));
    }

    #[test]
    fn without_a_fingerprint_the_version_tells() {
        assert!(has_this_code_as("", None, Some(VERSION)));
        assert!(!has_this_code_as("", None, Some("1.0")));
    }

    #[test]
    fn a_copy_says_whether_it_is_current() {
        let current = format!("{{\"gestos-fingerprint\": \"{FINGERPRINT}\"}}");
        assert!(metadata_is_current(&current));
        assert!(!metadata_is_current("{\"uuid\": \"x\"}"));
        assert!(!is_current(&ExtensionInfo::new()));
    }
}
