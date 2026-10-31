use std::process::Command;

pub fn get_autostart_file_path() -> Option<std::path::PathBuf> {
    gestos::config::autostart_dir().map(|dir| dir.join("gestosd.desktop"))
}

pub fn get_overlay_autostart_file_path() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(|home| {
        std::path::PathBuf::from(home)
            .join(".config")
            .join("autostart")
            .join("gestos-overlay.desktop")
    })
}

pub fn set_autostart_enabled(enabled: bool) -> std::io::Result<()> {
    let Some(path) = get_autostart_file_path() else {
        return Err(std::io::Error::other("HOME is not set"));
    };
    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, gestos::config::daemon_autostart_entry())
    } else if path.exists() {
        std::fs::remove_file(&path)
    } else {
        Ok(())
    }
}

fn autostart_decided_path() -> std::path::PathBuf {
    gestos::config::config_home().join("gestos").join("autostart_decided")
}

/// Records that login startup is the user's own choice from now on, so that it's never changed
/// for them again.
pub fn mark_autostart_decided() -> std::io::Result<()> {
    let path = autostart_decided_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, "")
}

/// Whether a first start should also turn on login startup: only for someone new. Anyone who
/// has chosen, or who used MyGestures and may have left it off on purpose, keeps their setting.
fn is_new_to_autostart(
    decided: bool,
    autostart_enabled: bool,
    used_mygestures: bool,
    needs_manual_setup: bool,
) -> bool {
    !decided && !autostart_enabled && !used_mygestures && !needs_manual_setup
}

pub fn should_enable_autostart_on_first_start() -> bool {
    is_new_to_autostart(
        autostart_decided_path().exists(),
        get_autostart_file_path().is_some_and(|path| path.exists()),
        gestos::config::config_home().join("mygestures").exists(),
        gestos::config::manual_autostart_for(gestos::config::get_environment_suffix()).is_some(),
    )
}

pub fn stop_overlay() {
    let _ = Command::new("pkill").args(["-f", "gestos --overlay"]).status();
}

/// Starts the overlay if the user has it enabled. Only the daemon's signals drive it, so it's
/// stopped and started along with the daemon.
pub fn start_overlay_if_enabled() {
    if get_overlay_autostart_file_path().is_some_and(|path| path.exists()) {
        let exe = std::env::current_exe().unwrap_or_else(|_| "gestos".into());
        let _ = Command::new(exe).arg("--overlay").spawn();
    }
}

pub fn set_overlay_enabled(enabled: bool) {
    if let Some(path) = get_overlay_autostart_file_path() {
        if enabled {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let content = "[Desktop Entry]\n\
                           Type=Application\n\
                           Name=Gestos Overlay\n\
                           Comment=Gesture visualization overlay\n\
                           Exec=gestos --overlay\n\
                           Icon=gestos\n\
                           Terminal=false\n\
                           X-GNOME-Autostart-enabled=true\n";
            let _ = std::fs::write(&path, content);

            // Start the overlay process immediately
            let _ = Command::new("gestos").arg("--overlay").spawn();
        } else {
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
            // Kill any running overlay instance
            stop_overlay();
        }
    }
}

pub fn get_osd_enabled_file_path() -> std::path::PathBuf {
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        std::path::PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home).join(".config")
    } else {
        std::path::PathBuf::from(".")
    };
    base.join("gestos").join("osd_enabled")
}

pub fn is_osd_enabled() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--no-osd") {
        return false;
    }
    if args.iter().any(|arg| arg == "--osd") {
        return true;
    }
    let path = get_osd_enabled_file_path();
    if !path.exists() {
        true
    } else {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        content.trim() == "true"
    }
}

pub fn set_osd_enabled(enabled: bool) {
    let path = get_osd_enabled_file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, if enabled { "true" } else { "false" });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_someone_new_gets_autostart_on_first_start() {
        assert!(is_new_to_autostart(false, false, false, false));

        assert!(!is_new_to_autostart(true, false, false, false));
        assert!(!is_new_to_autostart(false, true, false, false));
        assert!(!is_new_to_autostart(false, false, true, false));
        assert!(!is_new_to_autostart(false, false, false, true));
    }
}
