//! Action templates: advanced actions written out in YAML files, which Gestos offers among its
//! actions. A gesture made from one keeps only the template's ID, and its steps are read when it
//! runs, so a template that's updated changes what the gesture does. A gesture whose steps the
//! user customizes gets a copy of them, and no longer follows the template.
//!
//! Gestos's own are files in `gestos/action-templates` in the system's data folders, where it
//! installs them. The user's own, imported ones are kept in their configuration, with their
//! gestures. Each has a short ID of its own, like `convert-to-pdf`, whatever its file is called,
//! and the user's takes the place of one of Gestos's with the same ID. A file is only read if it says it's
//! one, with `gestos-action-template: 1`, if only its owner or root can change it and its folder,
//! and if it's small. One with anything Gestos doesn't know is left out whole, rather than run with
//! a part missing.
//!
//! ```yaml
//! gestos-action-template: 1
//! id: convert-to-pdf
//! name: Convert to PDF
//! description: Converts the selected documents to PDF, next to them
//! icon: x-office-document-symbolic
//! apps: [FileManager]
//! needs:
//!   - program: libreoffice
//!     package: LibreOffice
//! steps:
//!   - copy: files
//!   - run: for f in "$@"; do libreoffice --headless --convert-to pdf "$f"; done
//! ```

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::command_variables::ShellCommand;
use crate::key_steps::{CopyKind, KeyStep, MAX_WAIT_MILLIS};

/// The version of the format this Gestos reads.
const FORMAT: u32 = 1;
/// The largest template read, which is far more than steps need.
const MAX_SIZE: u64 = 64 * 1024;

/// A template as Gestos offers it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionTemplate {
    /// Its own short ID, which gestures made from it name.
    pub id: String,
    pub name: String,
    pub description: String,
    /// The name of the icon it's shown with, from the icon theme, if it has one.
    pub icon: Option<String>,
    /// Desktop file categories, like `FileManager`, or app IDs, it's offered for. None is any app.
    pub apps: Vec<String>,
    pub needs: Vec<Need>,
    pub steps: Vec<KeyStep>,
}

/// A program a template's steps run, and the package it's installed with, for when it isn't.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Need {
    pub program: String,
    pub package: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct TemplateFile {
    // Read already, and only here so that it isn't an unknown field.
    #[serde(rename = "gestos-action-template")]
    _version: u32,
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    apps: Vec<String>,
    #[serde(default)]
    needs: Vec<Need>,
    // Each step as one key and its value, like `copy: files`, rather than YAML's own `!copy files`.
    #[serde(with = "serde_yaml::with::singleton_map_recursive")]
    steps: Vec<StepFile>,
}

/// Only the version, read first, so that a newer format is told apart from a broken file.
#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Version {
    gestos_action_template: u32,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
enum StepFile {
    Copy(CopyFile),
    Run(String),
    Press(String),
    Type(String),
    Wait(u32),
    Open(String),
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum CopyFile {
    Files,
    Text,
}

impl From<StepFile> for KeyStep {
    fn from(step: StepFile) -> Self {
        match step {
            StepFile::Copy(CopyFile::Files) => KeyStep::Copy(CopyKind::Files),
            StepFile::Copy(CopyFile::Text) => KeyStep::Copy(CopyKind::Text),
            // A template's commands are given what's copied and selected, as that's what they're for.
            // and what they print is their result.
            StepFile::Run(command) => KeyStep::Run(ShellCommand::new(command.trim(), true).showing_output()),
            StepFile::Press(keys) => KeyStep::Press(keys),
            StepFile::Type(text) => KeyStep::Type(text),
            StepFile::Wait(millis) => KeyStep::Wait(millis.min(MAX_WAIT_MILLIS)),
            StepFile::Open(app) => KeyStep::Open(app),
        }
    }
}

/// The folders the templates that come with Gestos are read from: the system's, where Gestos
/// installs them.
fn template_dirs() -> Vec<PathBuf> {
    gio::glib::system_data_dirs().into_iter().map(|dir| dir.join("gestos").join("action-templates")).collect()
}

/// The user's own templates, each as its file's text, which their configuration keeps by ID. Set
/// from the configuration as it's loaded or saved, as gestures' names and icons are looked up
/// where it isn't at hand.
static OWN: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// Makes these the user's own templates, from their configuration.
pub fn set_own(own: &std::collections::BTreeMap<String, String>) {
    if let Ok(mut texts) = OWN.write() {
        *texts = own.values().cloned().collect();
    }
}

fn own() -> Vec<String> {
    OWN.read().map(|own| own.clone()).unwrap_or_default()
}

/// Where a template Gestos offers comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub template: ActionTemplate,
    /// Its text, as it was imported, or as its file has it.
    pub text: String,
    /// Whether it's the user's own, imported, rather than one that comes with Gestos.
    pub own: bool,
    /// Whether it's the user's own in place of one that comes with Gestos, with the same ID.
    pub replaces: bool,
}

/// The templates offered, by name, each with where it comes from.
pub fn list() -> Vec<Listed> {
    let mut listed = list_from(&own(), &template_dirs());
    listed.sort_by(|a, b| a.template.name.to_lowercase().cmp(&b.template.name.to_lowercase()));
    listed
}

fn list_from(own: &[String], dirs: &[PathBuf]) -> Vec<Listed> {
    let installed = read_dirs(dirs);
    let own = own.iter().filter_map(|text| match parse(text) {
        Ok(template) => Some((template, text.clone())),
        Err(why) => {
            eprintln!("gestos: One of your action templates is left out: {why}");
            None
        }
    });
    let mut listed: Vec<Listed> = Vec::new();
    // The user's first, in place of one that comes with Gestos with the same ID, and among those
    // that come with Gestos, the first folder's.
    let all = own.map(|(template, text)| (template, text, true))
        .chain(installed.iter().cloned().map(|(template, text)| (template, text, false)));
    for (template, text, is_own) in all {
        if listed.iter().any(|listed| listed.template.id == template.id) {
            continue;
        }
        listed.push(Listed {
            replaces: is_own && installed.iter().any(|(other, _)| other.id == template.id),
            template,
            text,
            own: is_own,
        });
    }
    listed
}

/// Every template that can be read in the folders, in order, with its text. Those that can't be
/// read are logged and left out.
fn read_dirs(dirs: &[PathBuf]) -> Vec<(ActionTemplate, String)> {
    let mut read_all = Vec::new();
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        if let Err(why) = trusted(dir) {
            eprintln!("gestos: Action templates in {} are left out: {why}", dir.display());
            continue;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
            .collect();
        paths.sort();
        for path in paths {
            match read(&path) {
                Ok(read) => read_all.push(read),
                Err(why) => eprintln!("gestos: The action template {} is left out: {why}", path.display()),
            }
        }
    }
    read_all
}

/// Whether an ID is a short one, like `convert-to-pdf`: letters, digits, `-`, `_` and `.`, not
/// starting with a dot.
pub fn is_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('.')
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Reads a template from a file, to import it into the user's configuration: the template, and
/// its text, as the configuration keeps it.
pub fn read_to_import(from: &Path) -> Result<(ActionTemplate, String), String> {
    let size = std::fs::metadata(from).map_err(|e| e.to_string())?.len();
    if size > MAX_SIZE {
        return Err(format!("It's larger than {} KB", MAX_SIZE / 1024));
    }
    let text = std::fs::read_to_string(from).map_err(|e| e.to_string())?;
    let template = parse(&text).map_err(|why| format!("It isn't an action Gestos can read: {why}"))?;
    Ok((template, text))
}

/// A template from its file's text, as the user's configuration keeps it.
pub fn from_text(text: &str) -> Result<ActionTemplate, String> {
    parse(text)
}

/// The templates offered, the user's in place of those with the same ID that come with Gestos.
fn load_from(own: &[String], dirs: &[PathBuf]) -> Vec<ActionTemplate> {
    list_from(own, dirs).into_iter().map(|listed| listed.template).collect()
}

/// The templates offered, by name. Those that can't be read are logged and left out.
pub fn load() -> Vec<ActionTemplate> {
    list().into_iter().map(|listed| listed.template).collect()
}

/// The template with this ID, the user's if they have one, read now so that it's as installed.
pub fn find(id: &str) -> Option<ActionTemplate> {
    find_in(&own(), &template_dirs(), id)
}

fn find_in(own: &[String], dirs: &[PathBuf], id: &str) -> Option<ActionTemplate> {
    if !is_id(id) {
        return None;
    }
    load_from(own, dirs).into_iter().find(|template| template.id == id)
}

/// Whether only the user or root can change a file or folder, as another account could otherwise
/// put commands in it for the user to run.
fn trusted(path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let owner = meta.uid();
    if owner != 0 && owner != nix::unistd::getuid().as_raw() {
        return Err("it belongs to another user".to_string());
    }
    if meta.mode() & 0o022 != 0 {
        return Err("other users can change it".to_string());
    }
    Ok(())
}

fn read(path: &Path) -> Result<(ActionTemplate, String), String> {
    trusted(path)?;
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_SIZE {
        return Err(format!("it's larger than {} KB", MAX_SIZE / 1024));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok((parse(&text)?, text))
}

fn parse(text: &str) -> Result<ActionTemplate, String> {
    let version: Version = serde_yaml::from_str(text)
        .map_err(|_| "it doesn't start with gestos-action-template: 1".to_string())?;
    if version.gestos_action_template != FORMAT {
        return Err(format!(
            "it's version {} of the format, and this Gestos reads version {FORMAT}",
            version.gestos_action_template
        ));
    }
    let file: TemplateFile = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    let id = file.id.trim().to_string();
    if !is_id(&id) {
        return Err(format!("its ID, \"{id}\", has to be letters, digits, -, _ and dots, like convert-to-pdf"));
    }
    if file.name.trim().is_empty() {
        return Err("it has no name".to_string());
    }
    if file.steps.is_empty() {
        return Err("it has no steps".to_string());
    }
    let icon = file.icon.map(|icon| icon.trim().to_string()).filter(|icon| !icon.is_empty());
    if let Some(icon) = &icon {
        // A name in the icon theme, never a path to a file.
        if !icon.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) || icon.starts_with('.') {
            return Err(format!("{icon} isn't the name of an icon"));
        }
    }
    let steps: Vec<KeyStep> = file.steps.into_iter().map(KeyStep::from).collect();
    for step in &steps {
        if let KeyStep::Run(command) = step {
            if let Some(variable) = unquoted_variable(command) {
                eprintln!("gestos: In the action template {id}, {variable} isn't in double quotes");
            }
        }
    }
    Ok(ActionTemplate {
        id,
        name: file.name.trim().to_string(),
        description: file.description.trim().to_string(),
        icon,
        apps: file.apps,
        needs: file.needs,
        steps,
    })
}

/// A variable with what's copied or selected that a command uses outside double quotes, where
/// spaces and wildcards in it would split it or expand.
fn unquoted_variable(command: &str) -> Option<&'static str> {
    const VARIABLES: [&str; 6] =
        ["$@", "$GESTOS_FILES", "$GESTOS_SELECTION", "$GESTOS_CLIPBOARD", "$1", "$*"];
    let mut quoted = false;
    let mut single = false;
    let mut escaped = false;
    // Whether each command substitution, `$(…)`, is in double quotes: its own quotes start over.
    let mut outer: Vec<bool> = Vec::new();
    for (i, c) in command.char_indices() {
        let rest = &command[i..];
        match c {
            _ if escaped => escaped = false,
            '\\' if !single => escaped = true,
            '\'' if !quoted => single = !single,
            '"' if !single => quoted = !quoted,
            '$' if !single && rest.starts_with("$(") => {
                outer.push(quoted);
                quoted = false;
            }
            ')' if !single && !quoted && !outer.is_empty() => quoted = outer.pop().unwrap_or(false),
            '$' if !quoted && !single => {
                if let Some(variable) = VARIABLES.iter().find(|variable| {
                    rest.starts_with(*variable)
                        && !rest[variable.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
                }) {
                    return Some(variable);
                }
            }
            _ => {}
        }
    }
    None
}

impl ActionTemplate {
    /// The first program it needs that isn't installed, by its package.
    pub fn missing_package(&self) -> Option<&str> {
        self.needs
            .iter()
            .find(|need| gio::glib::find_program_in_path(&need.program).is_none())
            .map(|need| need.package.as_str())
    }

    /// Whether it's offered for an app, by the app's desktop file ID and categories.
    pub fn is_for(&self, app_id: &str, categories: &str) -> bool {
        let id = app_id.trim_end_matches(".desktop");
        self.apps.iter().any(|app| {
            app.trim_end_matches(".desktop").eq_ignore_ascii_case(id)
                || categories.split(';').any(|category| !category.is_empty() && category == app)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const PDF: &str = r#"gestos-action-template: 1
id: convert-to-pdf
name: Convert to PDF
description: Converts the selected documents
apps: [FileManager]
needs:
  - program: libreoffice
    package: LibreOffice
steps:
  - copy: files
  - run: |
      for f in "$@"; do libreoffice --headless --convert-to pdf "$f"; done
"#;

    #[test]
    fn a_template_is_read_with_its_steps() {
        let template = parse(PDF).unwrap();
        assert_eq!(template.name, "Convert to PDF");
        assert_eq!(template.apps, ["FileManager"]);
        assert_eq!(template.needs[0].package, "LibreOffice");
        assert_eq!(
            template.steps,
            [
                KeyStep::Copy(CopyKind::Files),
                KeyStep::Run(
                    ShellCommand::new(
                        r#"for f in "$@"; do libreoffice --headless --convert-to pdf "$f"; done"#,
                        true
                    )
                    .showing_output()
                ),
            ]
        );
    }

    #[test]
    fn every_kind_of_step_is_read() {
        let text = "gestos-action-template: 1\nid: test\nname: All\nsteps:\n  - open: org.gnome.TextEditor\n  - press: Control+N\n  - type: hi\n  - wait: 99999999\n  - copy: text\n";
        let steps = parse(text).unwrap().steps;
        assert_eq!(
            steps,
            [
                KeyStep::Open("org.gnome.TextEditor".into()),
                KeyStep::Press("Control+N".into()),
                KeyStep::Type("hi".into()),
                KeyStep::Wait(MAX_WAIT_MILLIS),
                KeyStep::Copy(CopyKind::Text),
            ]
        );
    }

    #[test]
    fn a_file_that_isnt_a_template_or_is_newer_is_left_out() {
        assert!(parse("name: Notes\nsteps: []\n").unwrap_err().contains("doesn't start"));
        let newer = PDF.replace("gestos-action-template: 1", "gestos-action-template: 2");
        assert!(parse(&newer).unwrap_err().contains("version 2"));
    }

    #[test]
    fn a_template_has_the_icon_it_names() {
        let with_icon = PDF.replace("apps:", "icon: x-office-document-symbolic\napps:");
        assert_eq!(parse(&with_icon).unwrap().icon.as_deref(), Some("x-office-document-symbolic"));
        assert_eq!(parse(PDF).unwrap().icon, None);
        for bad in ["/usr/share/icons/a.svg", "../a", "a b"] {
            assert!(parse(&PDF.replace("apps:", &format!("icon: '{bad}'\napps:"))).is_err(), "{bad}");
        }
    }

    #[test]
    fn anything_unknown_leaves_the_whole_template_out() {
        assert!(parse(&PDF.replace("  - copy: files", "  - delete: files")).is_err());
        assert!(parse(&PDF.replace("description:", "author: me\ndescription:")).is_err());
        assert!(parse(&PDF.replace("copy: files", "copy: images")).is_err());
        assert!(parse("gestos-action-template: 1\nid: test\nname: Empty\nsteps: []\n").is_err());
        assert!(parse("gestos-action-template: 1\nid: test\nname: ' '\nsteps:\n  - wait: 1\n").is_err());
    }

    #[test]
    fn variables_outside_double_quotes_are_found() {
        assert_eq!(unquoted_variable(r#"zip a.zip $@"#), Some("$@"));
        assert_eq!(unquoted_variable(r#"echo $GESTOS_SELECTION"#), Some("$GESTOS_SELECTION"));
        assert_eq!(unquoted_variable(r#"zip a.zip "$@""#), None);
        assert_eq!(unquoted_variable(r#"echo '$1' "$1" $10x"#), None);
        assert_eq!(unquoted_variable(r#"echo \$@"#), None);
        assert_eq!(unquoted_variable(r#"cd "$(dirname "$1")""#), None);
        assert_eq!(unquoted_variable(r#"cd $(dirname $1)"#), Some("$1"));
    }

    #[test]
    fn templates_are_offered_for_their_apps_by_id_or_category() {
        let template = parse(PDF).unwrap();
        assert!(template.is_for("org.gnome.Nautilus", "GTK;Utility;Core;FileManager;"));
        assert!(!template.is_for("org.gnome.Ptyxis", "System;TerminalEmulator;"));
        let mut for_one = template.clone();
        for_one.apps = vec!["org.gnome.Nautilus.desktop".into()];
        assert!(for_one.is_for("org.gnome.Nautilus", ""));
    }

    /// A folder of its own for a test, removed when it ends.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("gestos-templates-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            TempDir(dir)
        }
        fn write(&self, name: &str, text: &str, mode: u32) {
            let path = self.0.join(name);
            std::fs::write(&path, text).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_users_template_takes_the_place_of_the_systems() {
        let system = TempDir::new("system");
        system.write("convert-to-pdf.yaml", PDF, 0o644);
        system.write("notes.txt", "not a template", 0o644);
        let dirs = [system.0.clone()];
        let mine = vec![PDF.replace("Convert to PDF", "Mine")];
        let listed = list_from(&mine, &dirs);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].template.name, "Mine");
        assert!(listed[0].own && listed[0].replaces);
        assert_eq!(listed[0].text, mine[0]);
        let installed = list_from(&[], &dirs);
        assert!(!installed[0].own && !installed[0].replaces);
        assert_eq!(installed[0].text, PDF);
    }

    #[test]
    fn a_template_is_found_by_its_id_the_users_first() {
        let system = TempDir::new("find-system");
        system.write("convert-to-pdf.yaml", PDF, 0o644);
        let dirs = [system.0.clone()];
        assert_eq!(find_in(&[], &dirs, "convert-to-pdf").unwrap().name, "Convert to PDF");
        let mine = [PDF.replace("Convert to PDF", "Mine")];
        assert_eq!(find_in(&mine, &dirs, "convert-to-pdf").unwrap().name, "Mine");
        assert!(find_in(&mine, &dirs, "missing").is_none());
        assert!(find_in(&mine, &dirs, "../find-system/convert-to-pdf").is_none());
    }

    #[test]
    fn only_a_readable_template_is_imported_with_its_text() {
        let elsewhere = TempDir::new("import-from");
        elsewhere.write("broken.yaml", "gestos-action-template: 1\nid: test\nname: Broken\n", 0o644);
        elsewhere.write("anything.txt", PDF, 0o666);
        assert!(read_to_import(&elsewhere.0.join("broken.yaml")).is_err());
        let (template, text) = read_to_import(&elsewhere.0.join("anything.txt")).unwrap();
        assert_eq!(template.id, "convert-to-pdf");
        assert_eq!(text, PDF);
    }

    #[test]
    fn a_template_is_known_by_its_id_whatever_its_file_is_called() {
        let system = TempDir::new("id-system");
        system.write("my-pdf.yaml", PDF, 0o644);
        system.write("convert-to-pdf.yaml", &PDF.replace("id: convert-to-pdf", "id: other").replace("Convert to PDF", "Other"), 0o644);
        let dirs = [system.0.clone()];
        assert_eq!(find_in(&[], &dirs, "convert-to-pdf").unwrap().name, "Convert to PDF");
        assert_eq!(find_in(&[], &dirs, "other").unwrap().name, "Other");
        assert_eq!(load_from(&[], &dirs).len(), 2);
    }

    #[test]
    fn a_template_needs_an_id_that_can_be_a_file_name() {
        assert!(parse(&PDF.replace("id: convert-to-pdf\n", "")).is_err());
        for bad in ["../x", ".hidden", "a/b", "with space", "''"] {
            assert!(parse(&PDF.replace("id: convert-to-pdf", &format!("id: {bad}"))).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_users_template_that_cant_be_read_leaves_the_systems_offered() {
        let system = TempDir::new("broken-system");
        system.write("convert-to-pdf.yaml", PDF, 0o644);
        let broken = ["gestos-action-template: 1\nid: convert-to-pdf\nname: Mine\n".to_string()];
        assert_eq!(find_in(&broken, &[system.0.clone()], "convert-to-pdf").unwrap().name, "Convert to PDF");
    }

    #[test]
    fn templates_others_can_change_or_too_large_are_left_out() {
        let dir = TempDir::new("unsafe");
        let with_id = |id: &str| PDF.replace("id: convert-to-pdf", &format!("id: {id}"));
        dir.write("shared.yaml", &with_id("shared"), 0o666);
        dir.write("large.yaml", &format!("{}#{}\n", with_id("large"), "x".repeat(MAX_SIZE as usize)), 0o644);
        dir.write("fine.yaml", &with_id("fine"), 0o644);
        let ids: Vec<String> = load_from(&[], &[dir.0.clone()]).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["fine"]);
        std::fs::set_permissions(&dir.0, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(load_from(&[], &[dir.0.clone()]).is_empty());
    }

    /// An installed template's command, by its file name.
    fn installed_command(id: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("action-templates").join(format!("{id}.yaml"));
        let template = parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        template
            .steps
            .into_iter()
            .find_map(|step| match step {
                KeyStep::Run(command) => Some(command.text),
                _ => None,
            })
            .unwrap()
    }

    /// Runs a command as Gestos does, with the files as its arguments, and the text copied.
    fn run(command: &str, files: &[&Path], copied: &str) -> std::process::Output {
        std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .arg("gestos")
            .args(files)
            .env("GESTOS_CLIPBOARD", copied)
            .output()
            .unwrap()
    }

    #[test]
    fn a_zip_keeps_the_files_names_however_odd() {
        if std::process::Command::new("zip").arg("-v").output().is_err() {
            return;
        }
        let dir = TempDir::new("zip");
        dir.write("a 'b' $c.txt", "1", 0o644);
        dir.write("d.txt", "2", 0o644);
        let files = [dir.0.join("a 'b' $c.txt"), dir.0.join("d.txt")];
        let files: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        assert!(run(&installed_command("compress-to-zip"), &files, "").status.success());
        let archive = std::fs::read_dir(&dir.0)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "zip"))
            .unwrap();
        let listing = std::process::Command::new("unzip").arg("-Z1").arg(&archive).output().unwrap();
        let names = String::from_utf8_lossy(&listing.stdout);
        assert_eq!(names.lines().collect::<Vec<_>>(), ["a 'b' $c.txt", "d.txt"]);
    }

    #[test]
    fn the_selection_is_encoded_in_the_address() {
        if std::process::Command::new("python3").arg("-V").output().is_err() {
            return;
        }
        // The address it would open, rather than opening it.
        let shown = installed_command("search-the-web").replacen("xdg-open", "printf %s", 1);
        let output = run(&shown, &[], "a & b?");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "https://duckduckgo.com/?q=a%20%26%20b%3F");
    }

    /// What an installed template's command shows: what it printed if it worked, or why it didn't.
    fn shown(id: &str, files: &[&Path], copied: &str) -> Result<String, String> {
        let output = run(&installed_command(id), files, copied);
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_string();
        if output.status.success() { Ok(text(&output.stdout)) } else { Err(text(&output.stderr)) }
    }

    #[test]
    fn templates_say_why_they_did_nothing() {
        let dir = TempDir::new("messages");
        for name in ["photo.png", "only.pdf", "notes.txt"] {
            dir.write(name, "", 0o644);
        }
        let file = |name: &str| dir.0.join(name);
        assert_eq!(
            shown("convert-images-to-png", &[&file("photo.png")], ""),
            Err("The selected images are PNGs already".into())
        );
        assert_eq!(shown("merge-pdfs", &[&file("only.pdf")], ""), Err("Select at least two PDFs to merge".into()));
        assert_eq!(
            shown("merge-pdfs", &[&file("only.pdf"), &file("notes.txt")], ""),
            Err("notes.txt isn't a PDF".into())
        );
        assert_eq!(shown("extract-zip-files", &[&file("notes.txt")], ""), Err("notes.txt isn't a ZIP file".into()));
        assert_eq!(
            shown("show-checksums", &[&dir.0], ""),
            Err(format!("{} is a folder, and only files have a checksum", dir.0.file_name().unwrap().to_string_lossy()))
        );
        for id in ["search-the-web", "translate", "open-link", "read-aloud", "calculate", "count-words"] {
            assert_eq!(shown(id, &[], ""), Err("Select some text first".into()), "{id}");
        }
        assert_eq!(shown("open-link", &[], "/etc/passwd"), Err("The selected text isn't a link".into()));
    }

    #[test]
    fn templates_show_their_results() {
        let dir = TempDir::new("results");
        dir.write("a.txt", "", 0o644);
        assert_eq!(
            shown("show-checksums", &[&dir.0.join("a.txt")], ""),
            Ok("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  a.txt".into())
        );
        assert_eq!(shown("count-words", &[], "one"), Ok("1 word".into()));
        assert_eq!(shown("count-words", &[], "one two three"), Ok("3 words".into()));
        if std::process::Command::new("bc").arg("-v").output().is_ok() {
            assert_eq!(shown("calculate", &[], "2*(3+4)"), Ok("2*(3+4) = 14".into()));
            assert_eq!(shown("calculate", &[], "hello"), Err("The selected text isn't a calculation".into()));
        }
    }

    fn installed(program: &str) -> bool {
        gio::glib::find_program_in_path(program).is_some()
    }

    /// A PDF of some pages, made by Ghostscript.
    fn write_pdf(path: &Path, pages: usize) {
        let page = "/Helvetica findfont 40 scalefont setfont 72 700 moveto (Some text on a page) show showpage\n";
        let status = std::process::Command::new("gs")
            .args(["-q", "-dNOPAUSE", "-dBATCH", "-dSAFER", "-sDEVICE=pdfwrite"])
            .arg(format!("-sOutputFile={}", path.display()))
            .args(["-c", &page.repeat(pages)])
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_pdf_templates_take_only_pdfs() {
        let dir = TempDir::new("only-pdfs");
        dir.write("notes.txt", "", 0o644);
        for id in ["compress-pdfs", "split-pdf-into-pages", "convert-pdf-to-images"] {
            assert_eq!(shown(id, &[&dir.0.join("notes.txt")], ""), Err("notes.txt isn't a PDF".into()), "{id}");
        }
    }

    #[test]
    fn a_pdf_is_split_into_its_pages_and_images() {
        if !installed("gs") || !installed("pdfseparate") || !installed("pdftoppm") {
            return;
        }
        let dir = TempDir::new("split");
        let pdf = dir.0.join("Report.pdf");
        write_pdf(&pdf, 2);
        assert_eq!(shown("split-pdf-into-pages", &[&pdf], ""), Ok("Saved 2 pages in Report (pages)".into()));
        assert_eq!(names_in(&dir.0.join("Report (pages)")), ["Report page 1.pdf", "Report page 2.pdf"]);
        assert_eq!(shown("split-pdf-into-pages", &[&pdf], ""), Err("Report (pages) is there already".into()));
        assert_eq!(shown("convert-pdf-to-images", &[&pdf], ""), Ok("Saved 2 images in Report (images)".into()));
        assert_eq!(names_in(&dir.0.join("Report (images)")), ["Report page-1.png", "Report page-2.png"]);
    }

    #[test]
    fn a_pdf_is_compressed_only_when_that_makes_it_smaller() {
        if !installed("gs") {
            return;
        }
        let dir = TempDir::new("compress");
        let pdf = dir.0.join("Small.pdf");
        write_pdf(&pdf, 1);
        // A PDF Ghostscript just wrote is as small as it makes it.
        assert_eq!(shown("compress-pdfs", &[&pdf], ""), Err("The selected PDFs can't be made smaller".into()));
        assert_eq!(names_in(&dir.0), ["Small.pdf"]);
        // One padded with what it doesn't need is made smaller.
        let large = dir.0.join("Large.pdf");
        let mut padded = std::fs::read(&pdf).unwrap();
        padded.extend(format!("%{}\n", "x".repeat(200_000)).bytes());
        std::fs::write(&large, padded).unwrap();
        let result = shown("compress-pdfs", &[&large], "").unwrap();
        assert!(result.starts_with("Large.pdf: ") && result.contains(" → "), "{result}");
        assert!(dir.0.join("Large (compressed).pdf").exists());
    }

    #[test]
    fn a_dated_copy_has_the_date_before_the_extension() {
        let dir = TempDir::new("dated");
        dir.write("report.odt", "1", 0o644);
        dir.write("photos.tar.gz", "2", 0o644);
        dir.write("v1.2 notes.txt", "3", 0o644);
        std::fs::create_dir(dir.0.join("My.Project")).unwrap();
        dir.write("My.Project/inside.txt", "4", 0o644);
        let today = String::from_utf8(std::process::Command::new("date").arg("+%F").output().unwrap().stdout).unwrap();
        let today = today.trim();
        let files = ["report.odt", "photos.tar.gz", "v1.2 notes.txt", "My.Project"].map(|name| dir.0.join(name));
        let files: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        assert_eq!(shown("make-a-dated-copy", &files, ""), Ok("Made 4 dated copies".into()));
        assert!(dir.0.join(format!("report {today}.odt")).exists());
        assert!(dir.0.join(format!("photos {today}.tar.gz")).exists());
        assert!(dir.0.join(format!("v1.2 notes {today}.txt")).exists());
        assert!(dir.0.join(format!("My.Project {today}/inside.txt")).exists());
        // A second copy the same day has the time too.
        let again = shown("make-a-dated-copy", &[&dir.0.join("report.odt")], "").unwrap();
        assert!(again.starts_with(&format!("Saved report {today} ")) && again.ends_with(".odt"), "{again}");
    }

    #[test]
    fn a_qr_code_is_made_of_the_selected_text() {
        assert_eq!(shown("show-qr-code", &[], ""), Err("Select some text first".into()));
        if !installed("qrencode") {
            return;
        }
        let dir = TempDir::new("qr");
        // Where it'd be shown, rather than showing it.
        let command = installed_command("show-qr-code").replace("xdg-open", "true");
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .env("GESTOS_CLIPBOARD", "https://example.com")
            .env("XDG_RUNTIME_DIR", &dir.0)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(std::fs::read(dir.0.join("gestos-qr-code.png")).unwrap().starts_with(b"\x89PNG"));
        let too_long = "x".repeat(8000);
        assert_eq!(shown("show-qr-code", &[], &too_long), Err("The selected text is too long for a QR code".into()));
    }

    #[test]
    fn the_templates_gestos_installs_are_all_read() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("action-templates");
        let installed = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).count();
        let mut read = 0;
        for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            let id = path.file_stem().unwrap().to_string_lossy().into_owned();
            let template = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
            // As gestures made before templates had IDs named them by their files.
            assert_eq!(template.id, id);
            assert!(template.icon.is_some(), "{id} has no icon");
            for (i, step) in template.steps.iter().enumerate() {
                if let KeyStep::Run(command) = step {
                    // The selected text is copied first, as not every app reports what's selected.
                    assert!(!command.text.contains("GESTOS_SELECTION"), "{id}");
                    if command.text.contains("GESTOS_CLIPBOARD") {
                        assert!(template.steps[..i].contains(&KeyStep::Copy(CopyKind::Text)), "{id}");
                    }
                    assert_eq!(unquoted_variable(command), None, "{id}");
                    let status = std::process::Command::new("sh").arg("-n").arg("-c").arg(&command.text).status().unwrap();
                    assert!(status.success(), "{id}");
                }
            }
            read += 1;
        }
        assert_eq!(read, installed);
        assert!(read > 0);
    }
}
