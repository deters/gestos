//! Programs: apps Gestos tells by their windows, for those no installed app has, such as a game
//! started from Steam, or a Windows program run with Wine. Users see them as apps like any other:
//! one shows with the apps once it's turned on, and gestures can be made only for it, named
//! `program:<id>` as an app is by its desktop file ID. A window is the program's if one of its classes matches, or if its process has
//! one of the environment variables, set.
//!
//! Gestos's own are in this file, and turned off until the user turns them on. The user's own,
//! imported, are kept in their configuration, and one with the ID of one of Gestos's takes its
//! place. Each is a short YAML text, which is what exporting one saves.
//!
//! ```yaml
//! gestos-app: 1
//! id: steam-games
//! name: Steam Games
//! description: Games started from Steam, Windows ones too
//! icon: input-gaming-symbolic
//! windows:
//!   classes: ["steam_app_*"]
//!   environment: [SteamAppId]
//! ```

use std::path::Path;

use crate::active_app::WindowIds;

/// The version of the format this Gestos reads.
const FORMAT: u32 = 1;
/// The largest file imported, which is far more than a program needs.
const MAX_SIZE: u64 = 16 * 1024;
/// How gestures name a program among their apps. Desktop file IDs never have a colon.
pub const APP_PREFIX: &str = "program:";

/// Steam runs Windows games with Proton in windows of the class `steam_app_<id>`, and starts every
/// game, its own Linux ones too, with `SteamAppId` set to the game's.
const STEAM_GAMES: &str = "gestos-app: 1
id: steam-games
name: Steam Games
description: Games started from Steam, Windows ones too
icon: input-gaming-symbolic
windows:
  classes: [\"steam_app_*\"]
  environment: [SteamAppId]
";

/// Wine names a program's windows after its file, like `setup.exe`, however it was started:
/// from Lutris, Heroic, Bottles or by hand.
const WINDOWS_PROGRAMS: &str = "gestos-app: 1
id: windows-programs
name: Windows Programs
description: Programs for Windows run with Wine, from Lutris, Heroic, Bottles or on their own
icon: application-x-executable-symbolic
windows:
  classes: [\"*.exe\"]
";

/// The programs that come with Gestos, in the order they're matched in.
const BUILT_IN: &[&str] = &[STEAM_GAMES, WINDOWS_PROGRAMS];

/// A program as Gestos offers it.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Its own short ID.
    pub id: String,
    pub name: String,
    pub description: String,
    /// The name of the icon it's shown with, from the icon theme, if it has one.
    pub icon: Option<String>,
    /// The window classes that are the program's, where `*` stands for any text, as in
    /// `steam_app_*`. Compared without telling capitals apart.
    pub classes: Vec<String>,
    /// Variables a window's process has set, to anything but `0`, when it's the program's.
    pub environment: Vec<String>,
}

impl Program {
    /// How gestures name it among their apps.
    pub fn app_id(&self) -> String {
        format!("{APP_PREFIX}{}", self.id)
    }

    /// Whether a window is the program's, by its classes, then by what its process has set,
    /// which `has_variable` says.
    pub fn matches(&self, window: &WindowIds, has_variable: &dyn Fn(&str) -> bool) -> bool {
        let mut names = window.gtk_app_id.iter().chain(&window.classes);
        let by_class = names.any(|name| self.classes.iter().any(|pattern| matches_pattern(pattern, name)));
        by_class || self.environment.iter().any(|variable| has_variable(variable))
    }
}

/// Whether a name matches a pattern, where `*` stands for any text, even none. Capitals are
/// alike.
fn matches_pattern(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let name = name.to_lowercase();
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else { return false };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else { return rest.is_empty() };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ProgramFile {
    // Read already, and only here so that it isn't an unknown field.
    #[serde(rename = "gestos-app")]
    _version: u32,
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon: Option<String>,
    windows: WindowsFile,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowsFile {
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    environment: Vec<String>,
}

/// Only the version, read first, so that a newer format is told apart from a broken file.
#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Version {
    gestos_app: u32,
}

/// A program from its text, as the user's configuration keeps it.
pub fn from_text(text: &str) -> Result<Program, String> {
    let version: Version =
        serde_yaml::from_str(text).map_err(|_| "it doesn't start with gestos-app: 1".to_string())?;
    if version.gestos_app != FORMAT {
        return Err(format!(
            "it's version {} of the format, and this Gestos reads version {FORMAT}",
            version.gestos_app
        ));
    }
    let file: ProgramFile = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    let id = file.id.trim().to_string();
    if !crate::action_templates::is_id(&id) {
        return Err(format!("its ID, \"{id}\", has to be letters, digits, -, _ and dots, like steam-games"));
    }
    if file.name.trim().is_empty() {
        return Err("it has no name".to_string());
    }
    let icon = file.icon.map(|icon| icon.trim().to_string()).filter(|icon| !icon.is_empty());
    if let Some(icon) = &icon {
        // A name in the icon theme, never a path to a file.
        if !icon.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) || icon.starts_with('.') {
            return Err(format!("{icon} isn't the name of an icon"));
        }
    }
    let classes: Vec<String> = file.windows.classes.iter().map(|class| class.trim().to_string()).collect();
    if let Some(class) = classes.iter().find(|class| class.chars().all(|c| c == '*')) {
        return Err(if class.is_empty() {
            "one of its classes is empty".to_string()
        } else {
            format!("its class {class} would be every window")
        });
    }
    let environment: Vec<String> = file.windows.environment.iter().map(|name| name.trim().to_string()).collect();
    let is_variable = |name: &str| {
        name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if let Some(name) = environment.iter().find(|name| !is_variable(name)) {
        return Err(format!("\"{name}\" isn't the name of an environment variable"));
    }
    if classes.is_empty() && environment.is_empty() {
        return Err("it says no classes or environment variables to tell its windows by".to_string());
    }
    Ok(Program {
        id,
        name: file.name.trim().to_string(),
        description: file.description.trim().to_string(),
        icon,
        classes,
        environment,
    })
}

/// Reads a program from a file, to import it into the user's configuration: the program, and its
/// text, as the configuration keeps it.
pub fn read_to_import(from: &Path) -> Result<(Program, String), String> {
    let size = std::fs::metadata(from).map_err(|e| e.to_string())?.len();
    if size > MAX_SIZE {
        return Err(format!("It's larger than {} KB", MAX_SIZE / 1024));
    }
    let text = std::fs::read_to_string(from).map_err(|e| e.to_string())?;
    let program = from_text(&text).map_err(|why| format!("It isn't an app Gestos can read: {why}"))?;
    Ok((program, text))
}

/// A program Gestos offers, with where it comes from and whether it's on.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub program: Program,
    /// Its text, as it was imported, or as Gestos has it.
    pub text: String,
    /// Whether it's the user's own, imported, rather than one that comes with Gestos.
    pub own: bool,
    /// Whether it's the user's own in place of one that comes with Gestos, with the same ID.
    pub replaces: bool,
    pub on: bool,
}

/// The user's own programs, each as its text, and the IDs of those turned on, from their
/// configuration. Set as it's loaded or saved, as windows are matched where it isn't at hand.
static FROM_CONFIG: std::sync::RwLock<(Vec<String>, Vec<String>)> = std::sync::RwLock::new((Vec::new(), Vec::new()));

/// Makes these the user's own programs, and these IDs the ones turned on, from their
/// configuration.
pub fn set_from_config(own: &std::collections::BTreeMap<String, String>, on: &[String]) {
    if let Ok(mut from_config) = FROM_CONFIG.write() {
        *from_config = (own.values().cloned().collect(), on.to_vec());
    }
}

/// The programs offered, by name.
pub fn list() -> Vec<Listed> {
    let (own, on) = FROM_CONFIG.read().map(|from| from.clone()).unwrap_or_default();
    let mut listed = list_from(&own, &on);
    listed.sort_by(|a, b| a.program.name.to_lowercase().cmp(&b.program.name.to_lowercase()));
    listed
}

/// The user's own first, in place of those that come with Gestos with the same ID, then Gestos's,
/// each in the order they're matched in.
fn list_from(own: &[String], on: &[String]) -> Vec<Listed> {
    let built_in: Vec<Program> = BUILT_IN.iter().filter_map(|text| from_text(text).ok()).collect();
    let own = own.iter().filter_map(|text| match from_text(text) {
        Ok(program) => Some((program, text.clone(), true)),
        Err(why) => {
            eprintln!("gestos: One of your apps told by their windows is left out: {why}");
            None
        }
    });
    let all = own.chain(BUILT_IN.iter().filter_map(|text| Some((from_text(text).ok()?, text.to_string(), false))));
    let mut listed: Vec<Listed> = Vec::new();
    for (program, text, is_own) in all {
        if listed.iter().any(|listed| listed.program.id == program.id) {
            continue;
        }
        listed.push(Listed {
            replaces: is_own && built_in.iter().any(|other| other.id == program.id),
            on: on.contains(&program.id),
            program,
            text,
            own: is_own,
        });
    }
    listed
}

/// The programs turned on, in the order they're matched in.
pub fn turned_on() -> Vec<Program> {
    let (own, on) = FROM_CONFIG.read().map(|from| from.clone()).unwrap_or_default();
    list_from(&own, &on).into_iter().filter(|listed| listed.on).map(|listed| listed.program).collect()
}

/// The program a gesture names among its apps, as `program:<id>`, whether it's on or not.
pub fn find(app: &str) -> Option<Program> {
    let id = app.strip_prefix(APP_PREFIX)?;
    let (own, on) = FROM_CONFIG.read().map(|from| from.clone()).unwrap_or_default();
    list_from(&own, &on).into_iter().map(|listed| listed.program).find(|program| program.id == id)
}

/// The program turned on that a window is, as gestures name it, if any. Its process's
/// environment is only read if a program asks for a variable, and the window says its process.
pub fn matching(window: &WindowIds) -> Option<String> {
    matching_in(&turned_on(), window, &|variable| {
        window.pid.is_some_and(|pid| crate::processes::has_variable(pid, variable))
    })
}

fn matching_in(programs: &[Program], window: &WindowIds, has_variable: &dyn Fn(&str) -> bool) -> Option<String> {
    programs.iter().find(|program| program.matches(window, has_variable)).map(Program::app_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(classes: &[&str]) -> WindowIds {
        WindowIds { gtk_app_id: None, classes: classes.iter().map(|c| c.to_string()).collect(), pid: None }
    }

    const MINE: &str = "gestos-app: 1
id: my-game
name: My Game
windows:
  classes: [MyGame-*]
";

    #[test]
    fn patterns_match_any_text_where_the_star_is() {
        assert!(matches_pattern("steam_app_*", "steam_app_1091500"));
        assert!(matches_pattern("steam_app_*", "STEAM_APP_1"));
        assert!(matches_pattern("steam_app_*", "steam_app_"));
        assert!(!matches_pattern("steam_app_*", "steam"));
        assert!(matches_pattern("*.exe", "setup.exe"));
        assert!(!matches_pattern("*.exe", "setup.exe.bak"));
        assert!(matches_pattern("Mine*craft*", "Minecraft 1.20.1"));
        assert!(matches_pattern("a*b*a", "aba"));
        assert!(!matches_pattern("a*a", "a"));
        assert!(matches_pattern("Game", "game"));
        assert!(!matches_pattern("Game", "Games"));
    }

    #[test]
    fn gestos_own_programs_can_be_read() {
        for text in BUILT_IN {
            from_text(text).unwrap();
        }
    }

    #[test]
    fn steam_games_are_told_by_their_class_or_the_variable_steam_sets() {
        let steam = from_text(STEAM_GAMES).unwrap();
        let none = |_: &str| false;
        assert!(steam.matches(&window(&["steam_app_1091500", "steam_app_1091500"]), &none));
        assert!(!steam.matches(&window(&["Hades", "Hades"]), &none));
        assert!(steam.matches(&window(&["Hades", "Hades"]), &|variable| variable == "SteamAppId"));
        assert!(!steam.matches(&window(&["firefox"]), &|variable| variable == "WINEPREFIX"));
    }

    #[test]
    fn the_first_program_turned_on_that_a_window_is_is_the_one() {
        let programs: Vec<Program> = BUILT_IN.iter().map(|text| from_text(text).unwrap()).collect();
        let none = |_: &str| false;
        assert_eq!(matching_in(&programs, &window(&["Setup.EXE"]), &none).as_deref(), Some("program:windows-programs"));
        assert_eq!(matching_in(&programs, &window(&["steam_app_5"]), &none).as_deref(), Some("program:steam-games"));
        assert_eq!(matching_in(&programs, &window(&["org.gnome.Nautilus"]), &none), None);
        assert_eq!(matching_in(&[], &window(&["steam_app_5"]), &none), None);
    }

    #[test]
    fn programs_are_off_until_turned_on_and_the_users_take_the_place_of_gestos_own() {
        let listed = list_from(&[], &[]);
        assert!(listed.iter().all(|listed| !listed.on && !listed.own));
        assert!(listed.iter().any(|listed| listed.program.id == "steam-games"));

        let replacing = STEAM_GAMES.replace("name: Steam Games", "name: My Steam");
        let listed = list_from(&[replacing, MINE.to_string()], &["steam-games".to_string()]);
        let steam = listed.iter().find(|listed| listed.program.id == "steam-games").unwrap();
        assert!(steam.own && steam.replaces && steam.on);
        assert_eq!(steam.program.name, "My Steam");
        let mine = listed.iter().find(|listed| listed.program.id == "my-game").unwrap();
        assert!(mine.own && !mine.replaces && !mine.on);
        assert_eq!(listed.iter().filter(|listed| listed.program.id == "steam-games").count(), 1);
    }

    #[test]
    fn a_program_has_to_say_how_its_windows_are_told() {
        let with = |windows: &str| format!("gestos-app: 1\nid: x\nname: X\nwindows:\n{windows}");
        assert!(from_text(&with("  classes: [a]\n")).is_ok());
        assert!(from_text(&with("  environment: [GAME_ID]\n")).is_ok());
        assert!(from_text(&with("  classes: []\n")).unwrap_err().contains("no classes"));
        assert!(from_text(&with("  classes: [\"*\"]\n")).unwrap_err().contains("every window"));
        assert!(from_text(&with("  classes: [\" \"]\n")).unwrap_err().contains("empty"));
        assert!(from_text(&with("  environment: [\"A=1\"]\n")).unwrap_err().contains("environment variable"));
        assert!(from_text(&with("  classes: [a]\n  pid: 4\n")).is_err());
        assert!(from_text("gestos-app: 2\nid: x\nname: X\nwindows: {}\n").unwrap_err().contains("version 2"));
        assert!(from_text("id: x\n").unwrap_err().contains("gestos-app: 1"));
        assert!(from_text(&with("  classes: [a]\n").replace("id: x", "id: ../x")).unwrap_err().contains("ID"));
        assert!(from_text(&format!("{}icon: /tmp/a.png\n", with("  classes: [a]\n"))).unwrap_err().contains("icon"));
    }

    #[test]
    fn a_program_is_found_by_how_gestures_name_it() {
        set_from_config(&[("my-game".to_string(), MINE.to_string())].into_iter().collect(), &[]);
        assert_eq!(find("program:my-game").map(|program| program.name).as_deref(), Some("My Game"));
        assert_eq!(find("program:steam-games").map(|program| program.app_id()).as_deref(), Some("program:steam-games"));
        assert_eq!(find("my-game"), None);
        assert!(turned_on().is_empty());
    }
}
