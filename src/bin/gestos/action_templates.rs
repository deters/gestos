//! Action templates: advanced actions written out in YAML files, which Gestos offers among its
//! actions. Picking one makes a gesture with its steps, which then belong to the gesture, so a
//! template that changes later doesn't change what a gesture does.
//!
//! They're read from `gestos/action-templates` in the user's data folder, then in the system's,
//! where Gestos installs its own. A user's template takes the place of a system one with the same
//! file name. A file is only read if it says it's one, with `gestos-action-template: 1`, if only
//! its owner or root can change it and its folder, and if it's small. One with anything Gestos
//! doesn't know is left out whole, rather than run with a part missing.
//!
//! ```yaml
//! gestos-action-template: 1
//! name: Convert to PDF
//! description: Converts the selected documents to PDF, next to them
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

use gestos::command_variables::ShellCommand;
use gestos::key_steps::{CopyKind, KeyStep, MAX_WAIT_MILLIS};

/// The version of the format this Gestos reads.
const FORMAT: u32 = 1;
/// The largest template read, which is far more than steps need.
const MAX_SIZE: u64 = 64 * 1024;

/// A template as Gestos offers it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionTemplate {
    /// Its file name without `.yaml`.
    pub id: String,
    pub name: String,
    pub description: String,
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
    name: String,
    #[serde(default)]
    description: String,
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
            StepFile::Run(command) => KeyStep::Run(ShellCommand::new(command.trim(), true)),
            StepFile::Press(keys) => KeyStep::Press(keys),
            StepFile::Type(text) => KeyStep::Type(text),
            StepFile::Wait(millis) => KeyStep::Wait(millis.min(MAX_WAIT_MILLIS)),
            StepFile::Open(app) => KeyStep::Open(app),
        }
    }
}

/// The folders templates are read from, the user's first.
fn template_dirs() -> Vec<PathBuf> {
    std::iter::once(gtk4::glib::user_data_dir())
        .chain(gtk4::glib::system_data_dirs())
        .map(|dir| dir.join("gestos").join("action-templates"))
        .collect()
}

/// The templates in the folders, by name. Those that can't be read are logged and left out.
pub fn load() -> Vec<ActionTemplate> {
    let mut templates = load_from(&template_dirs());
    templates.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    templates
}

fn load_from(dirs: &[PathBuf]) -> Vec<ActionTemplate> {
    let mut templates: Vec<ActionTemplate> = Vec::new();
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
            let Some(id) = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()) else { continue };
            // An earlier folder's, the user's, takes its place.
            if templates.iter().any(|template| template.id == id) {
                continue;
            }
            match read(&path, &id) {
                Ok(template) => templates.push(template),
                Err(why) => eprintln!("gestos: The action template {} is left out: {why}", path.display()),
            }
        }
    }
    templates
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

fn read(path: &Path, id: &str) -> Result<ActionTemplate, String> {
    trusted(path)?;
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_SIZE {
        return Err(format!("it's larger than {} KB", MAX_SIZE / 1024));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(&text, id)
}

fn parse(text: &str, id: &str) -> Result<ActionTemplate, String> {
    let version: Version = serde_yaml::from_str(text)
        .map_err(|_| "it doesn't start with gestos-action-template: 1".to_string())?;
    if version.gestos_action_template != FORMAT {
        return Err(format!(
            "it's version {} of the format, and this Gestos reads version {FORMAT}",
            version.gestos_action_template
        ));
    }
    let file: TemplateFile = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    if file.name.trim().is_empty() {
        return Err("it has no name".to_string());
    }
    if file.steps.is_empty() {
        return Err("it has no steps".to_string());
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
        id: id.to_string(),
        name: file.name.trim().to_string(),
        description: file.description.trim().to_string(),
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
    /// The action it makes, as the editor and the action search have actions: its steps, named
    /// as the template is, and described as it is, or by what to install for it.
    pub fn option(&self) -> crate::state::EditorActionOption {
        let tooltip = match self.missing_package() {
            Some(package) => format!("Install {package}"),
            None => self.description.clone(),
        };
        crate::state::EditorActionOption {
            category: 0,
            action_type: gestos::config::ActionType::Keyboard(self.steps.clone()),
            name: self.name.clone(),
            tooltip,
        }
    }

    /// The first program it needs that isn't installed, by its package.
    pub fn missing_package(&self) -> Option<&str> {
        self.needs
            .iter()
            .find(|need| gtk4::glib::find_program_in_path(&need.program).is_none())
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
        let template = parse(PDF, "convert-to-pdf").unwrap();
        assert_eq!(template.name, "Convert to PDF");
        assert_eq!(template.apps, ["FileManager"]);
        assert_eq!(template.needs[0].package, "LibreOffice");
        assert_eq!(
            template.steps,
            [
                KeyStep::Copy(CopyKind::Files),
                KeyStep::Run(ShellCommand::new(
                    r#"for f in "$@"; do libreoffice --headless --convert-to pdf "$f"; done"#,
                    true
                )),
            ]
        );
    }

    #[test]
    fn every_kind_of_step_is_read() {
        let text = "gestos-action-template: 1\nname: All\nsteps:\n  - open: org.gnome.TextEditor\n  - press: Control+N\n  - type: hi\n  - wait: 99999999\n  - copy: text\n";
        let steps = parse(text, "all").unwrap().steps;
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
        assert!(parse("name: Notes\nsteps: []\n", "notes").unwrap_err().contains("doesn't start"));
        let newer = PDF.replace("gestos-action-template: 1", "gestos-action-template: 2");
        assert!(parse(&newer, "pdf").unwrap_err().contains("version 2"));
    }

    #[test]
    fn anything_unknown_leaves_the_whole_template_out() {
        assert!(parse(&PDF.replace("  - copy: files", "  - delete: files"), "pdf").is_err());
        assert!(parse(&PDF.replace("description:", "author: me\ndescription:"), "pdf").is_err());
        assert!(parse(&PDF.replace("copy: files", "copy: images"), "pdf").is_err());
        assert!(parse("gestos-action-template: 1\nname: Empty\nsteps: []\n", "empty").is_err());
        assert!(parse("gestos-action-template: 1\nname: ' '\nsteps:\n  - wait: 1\n", "blank").is_err());
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
        let template = parse(PDF, "pdf").unwrap();
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
        let user = TempDir::new("user");
        let system = TempDir::new("system");
        user.write("convert-to-pdf.yaml", &PDF.replace("Convert to PDF", "Mine"), 0o644);
        system.write("convert-to-pdf.yaml", PDF, 0o644);
        system.write("notes.txt", "not a template", 0o644);
        let templates = load_from(&[user.0.clone(), system.0.clone()]);
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].name, "Mine");
    }

    #[test]
    fn templates_others_can_change_or_too_large_are_left_out() {
        let dir = TempDir::new("unsafe");
        dir.write("shared.yaml", PDF, 0o666);
        dir.write("large.yaml", &format!("{PDF}#{}\n", "x".repeat(MAX_SIZE as usize)), 0o644);
        dir.write("fine.yaml", PDF, 0o644);
        let ids: Vec<String> = load_from(&[dir.0.clone()]).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["fine"]);
        std::fs::set_permissions(&dir.0, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(load_from(&[dir.0.clone()]).is_empty());
    }

    /// An installed template's command, by its file name.
    fn installed_command(id: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("action-templates").join(format!("{id}.yaml"));
        let template = parse(&std::fs::read_to_string(path).unwrap(), id).unwrap();
        template
            .steps
            .into_iter()
            .find_map(|step| match step {
                KeyStep::Run(command) => Some(command.text),
                _ => None,
            })
            .unwrap()
    }

    /// Runs a command as Gestos does, with the files as its arguments, and the selection.
    fn run(command: &str, files: &[&Path], selection: &str) -> std::process::Output {
        std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .arg("gestos")
            .args(files)
            .env("GESTOS_SELECTION", selection)
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

    #[test]
    fn the_templates_gestos_installs_are_all_read() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("action-templates");
        let installed = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).count();
        let mut read = 0;
        for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            let id = path.file_stem().unwrap().to_string_lossy().into_owned();
            let template = parse(&std::fs::read_to_string(&path).unwrap(), &id).unwrap();
            for step in &template.steps {
                if let KeyStep::Run(command) = step {
                    assert_eq!(unquoted_variable(command), None, "{id}");
                    let status = std::process::Command::new("sh").arg("-n").arg("-c").arg(&command.text).status().unwrap();
                    assert!(status.success(), "{id}");
                }
            }
            assert!(!template.needs.is_empty(), "{id}");
            read += 1;
        }
        assert_eq!(read, installed);
        assert!(read > 0);
    }
}
