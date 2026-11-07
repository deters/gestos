//! Builds in the fingerprint of the GNOME extension that comes with this Gestos, which tells
//! whether the copy GNOME runs is that one. It's empty where the fingerprint can't be made, and
//! Gestos then goes by the extension's version instead.

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
}
