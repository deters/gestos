//! Builds in the fingerprint of the GNOME extension that comes with this Gestos, which tells
//! whether the copy GNOME runs is that one. It's empty where the fingerprint can't be made, and
//! Gestos then goes by the extension's version instead. Also the version shown to people, which
//! a beta build names in `packaging/beta-version`, like `6.10.0 beta 4`.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=extension");
    let fingerprint = Command::new("sh")
        .arg("extension/fingerprint.sh")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();
    if fingerprint.is_empty() {
        println!("cargo:warning=No GNOME extension fingerprint, so its version tells instead");
    }
    println!("cargo:rustc-env=GESTOS_EXTENSION_FINGERPRINT={fingerprint}");

    println!("cargo:rerun-if-changed=packaging");
    let shown = std::fs::read_to_string("packaging/beta-version")
        .map(|text| text.trim().to_string())
        .ok()
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    println!("cargo:rustc-env=GESTOS_SHOWN_VERSION={shown}");
}
