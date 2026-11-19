//! A custom action's steps: apps to open, shortcuts to press, text to type, commands to run, and
//! pauses, in the order given. They're written on one line, like
//! `open org.mozilla.firefox; press Control+L; type "gestos"; press Return`, as gestures keep
//! their actions as text.

use crate::command_variables::ShellCommand;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum KeyStep {
    /// Keys pressed together, like `Control+L`.
    Press(String),
    /// Text typed one character at a time, with whatever keys the layout needs for each.
    Type(String),
    /// A pause, in milliseconds, for an app to catch up.
    Wait(u32),
    /// An app, by its desktop file ID, opened or brought to the front, which the steps after it
    /// then go to.
    Open(String),
    /// A command run in a shell, as Run Command runs one, finished before the steps after it.
    Run(crate::command_variables::ShellCommand),
    /// What's selected, copied with the keys that copy in the app in use, for the command after
    /// it. It's never pressed where those keys aren't known, as Control+C stops a terminal's
    /// program.
    Copy(CopyKind),
    /// What the command before it printed, shown on the screen for this many seconds. What a
    /// command prints is only shown with this step after it.
    Show(u32),
}

/// What a copy step copies for the command after it: the selected files, as a file manager copies
/// them, or the selected text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CopyKind {
    Files,
    Text,
}

/// What a command waits to find copied before it runs, after a copy made for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyWait {
    /// No copy was made for it, so it runs with whatever is copied.
    Nothing,
    /// A shortcut that copies, which may copy text or files.
    Anything,
    Kind(CopyKind),
}

/// The most a single pause can be, so that a typo can't leave a gesture running for hours.
pub const MAX_WAIT_MILLIS: u32 = 60_000;
/// The longest a result is shown, in seconds, so that a typo can't leave it over the screen.
pub const MAX_SHOW_SECONDS: u32 = 60;

/// Reads steps written by `format_steps`. Steps that can't be read are left out, so that one
/// mistake in a hand-edited file doesn't lose the rest.
pub fn parse_steps(text: &str) -> Vec<KeyStep> {
    split_steps(text).iter().filter_map(|step| parse_step(step)).collect()
}

/// Splits at semicolons outside of quotes, keeping the quotes and escapes for `parse_step`.
fn split_steps(text: &str) -> Vec<String> {
    let mut steps = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == ';' && !quoted {
            steps.push(std::mem::take(&mut current));
            continue;
        }
        current.push(c);
    }
    steps.push(current);
    steps
}

fn parse_step(step: &str) -> Option<KeyStep> {
    let step = step.trim();
    let (kind, rest) = step.split_once(char::is_whitespace).unwrap_or((step, ""));
    let rest = rest.trim();
    match kind.to_lowercase().as_str() {
        "press" if !rest.is_empty() => Some(KeyStep::Press(rest.to_string())),
        "open" if !rest.is_empty() => Some(KeyStep::Open(rest.to_string())),
        "type" => unquote(rest).map(KeyStep::Type),
        "run" => unquote(rest).map(|text| KeyStep::Run(ShellCommand::new(text, false))),
        "run-with-variables" => unquote(rest).map(|text| KeyStep::Run(ShellCommand::new(text, true))),
        // As commands that showed their output were written before the step that shows it.
        "run-showing-output" => unquote(rest).map(|text| KeyStep::Run(ShellCommand::new(text, true))),
        "copy" => match rest {
            "files" => Some(KeyStep::Copy(CopyKind::Files)),
            "text" => Some(KeyStep::Copy(CopyKind::Text)),
            _ => None,
        },
        "wait" => {
            let millis = rest.parse::<u32>().ok()?;
            Some(KeyStep::Wait(millis.min(MAX_WAIT_MILLIS)))
        }
        "show" => {
            let seconds = rest.parse::<u32>().ok()?;
            Some(KeyStep::Show(seconds.clamp(1, MAX_SHOW_SECONDS)))
        }
        _ => None,
    }
}

fn unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut unquoted = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            unquoted.push(c);
            continue;
        }
        match chars.next()? {
            'n' => unquoted.push('\n'),
            't' => unquoted.push('\t'),
            other => unquoted.push(other),
        }
    }
    Some(unquoted)
}

fn quote(text: &str) -> String {
    let mut quoted = String::from('"');
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

pub fn format_steps(steps: &[KeyStep]) -> String {
    steps
        .iter()
        .map(|step| match step {
            KeyStep::Press(keys) => format!("press {keys}"),
            KeyStep::Type(text) => format!("type {}", quote(text)),
            KeyStep::Wait(millis) => format!("wait {millis}"),
            KeyStep::Open(app) => format!("open {app}"),
            KeyStep::Run(command) if command.variables => format!("run-with-variables {}", quote(command)),
            KeyStep::Run(command) => format!("run {}", quote(command)),
            KeyStep::Copy(CopyKind::Files) => "copy files".to_string(),
            KeyStep::Copy(CopyKind::Text) => "copy text".to_string(),
            KeyStep::Show(seconds) => format!("show {seconds}"),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The steps as people read them: `open Firefox, Control+L, type “gestos”, wait 200 ms`.
pub fn describe_steps(steps: &[KeyStep]) -> String {
    steps
        .iter()
        .map(|step| match step {
            KeyStep::Press(keys) => keys.clone(),
            KeyStep::Type(text) => format!("type “{text}”"),
            KeyStep::Wait(millis) => format!("wait {millis} ms"),
            KeyStep::Open(app) => format!("open {}", app_name(app)),
            KeyStep::Run(command) => format!("run {}", crate::config::short_command(command)),
            KeyStep::Copy(CopyKind::Files) => "copy the selected files".to_string(),
            KeyStep::Copy(CopyKind::Text) => "copy the selected text".to_string(),
            KeyStep::Show(seconds) => format!("show the result for {seconds} s"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether keys copy what's selected, as apps and terminals have them.
pub fn is_copy_shortcut(keys: &str) -> bool {
    let mut parts: Vec<String> = keys
        .split('+')
        .map(|part| match part.trim().to_lowercase().as_str() {
            "ctrl" | "control_l" | "control_r" => "control".to_string(),
            "shift_l" | "shift_r" => "shift".to_string(),
            other => other.to_string(),
        })
        .collect();
    parts.sort();
    matches!(
        parts.iter().map(String::as_str).collect::<Vec<_>>().as_slice(),
        ["c", "control"] | ["c", "control", "shift"] | ["control", "x"] | ["control", "insert"]
    )
}

/// The steps that copy for a command after them that's given what's copied: the last shortcut
/// before such a command, if it copies. What was copied before is cleared first, so that the command can tell when the
/// copy is there, and never gets what was copied earlier when nothing is selected.
pub fn copies_for_commands(steps: &[KeyStep]) -> Vec<usize> {
    let mut copies = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let KeyStep::Run(command) = step else { continue };
        if !command.variables {
            continue;
        }
        let last_press = steps[..i].iter().rposition(|step| matches!(step, KeyStep::Press(_)));
        if let Some(j) = last_press {
            if matches!(&steps[j], KeyStep::Press(keys) if is_copy_shortcut(keys)) && !copies.contains(&j) {
                copies.push(j);
            }
        }
    }
    copies
}

/// The installed apps a person would look for, as their names and desktop file IDs, sorted by
/// name.
pub fn installed_apps() -> Vec<(String, String)> {
    static APPS: crate::app_cache::AppCache<Vec<(String, String)>> = crate::app_cache::AppCache::new();
    APPS.get(read_installed_apps)
}

fn read_installed_apps() -> Vec<(String, String)> {
    use gio::prelude::AppInfoExt;
    let apps = gio::AppInfo::all()
        .into_iter()
        .filter(|info| info.should_show())
        .filter_map(|info| {
            let id = info.id()?.trim_end_matches(".desktop").to_string();
            Some((info.display_name().to_string(), id))
        })
        .collect();
    distinct_names(apps)
}

/// Gives apps that share a name, as file managers often call themselves "Files", the last part of
/// their ID after it, like "Files (Nemo)", and sorts them by name.
fn distinct_names(mut apps: Vec<(String, String)>) -> Vec<(String, String)> {
    apps.sort_by(|a, b| a.1.cmp(&b.1));
    apps.dedup_by(|a, b| a.1 == b.1);
    let names: Vec<String> = apps.iter().map(|(name, _)| name.clone()).collect();
    for (name, id) in &mut apps {
        if names.iter().filter(|other| *other == name).count() > 1 {
            let program = id.rsplit('.').next().unwrap_or(id);
            let mut letters = program.chars();
            let program: String = letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect())
                .unwrap_or_default();
            *name = format!("{name} ({program})");
        }
    }
    apps.sort_by_key(|(name, _)| name.to_lowercase());
    apps
}

/// An app's name, as installed_apps has it, or its ID if it isn't installed.
pub fn app_name(id: &str) -> String {
    use gio::prelude::AppInfoExt;
    installed_apps()
        .into_iter()
        .find(|(_, app)| app == id)
        .map(|(name, _)| name)
        .or_else(|| {
            gio::DesktopAppInfo::new(&format!("{id}.desktop"))
                .map(|info| info.display_name().to_string())
        })
        .unwrap_or_else(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps_that_share_a_name_say_which_they_are() {
        let apps = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter().map(|(name, id)| (name.to_string(), id.to_string())).collect()
        };
        assert_eq!(
            distinct_names(apps(&[
                ("Files", "org.gnome.Nautilus"),
                ("Terminal", "org.gnome.Ptyxis"),
                ("Files", "nemo"),
            ])),
            apps(&[
                ("Files (Nautilus)", "org.gnome.Nautilus"),
                ("Files (Nemo)", "nemo"),
                ("Terminal", "org.gnome.Ptyxis"),
            ])
        );
    }

    fn steps() -> Vec<KeyStep> {
        vec![
            KeyStep::Press("Control+L".to_string()),
            KeyStep::Type("say \"hi\"; C:\\ ok\n".to_string()),
            KeyStep::Wait(200),
            KeyStep::Press("Return".to_string()),
        ]
    }

    #[test]
    fn steps_survive_saving_with_quotes_semicolons_and_backslashes() {
        let text = format_steps(&steps());
        assert_eq!(
            text,
            r#"press Control+L; type "say \"hi\"; C:\\ ok\n"; wait 200; press Return"#
        );
        assert_eq!(parse_steps(&text), steps());
    }

    #[test]
    fn unreadable_steps_are_left_out() {
        assert_eq!(
            parse_steps("press Alt+Tab; jump 3; type unquoted; wait soon; wait 5"),
            [KeyStep::Press("Alt+Tab".to_string()), KeyStep::Wait(5)]
        );
        assert_eq!(parse_steps(""), []);
    }

    #[test]
    fn an_app_to_open_survives_saving() {
        let steps = vec![
            KeyStep::Open("org.mozilla.firefox".to_string()),
            KeyStep::Press("Control+T".to_string()),
        ];
        let text = format_steps(&steps);
        assert_eq!(text, "open org.mozilla.firefox; press Control+T");
        assert_eq!(parse_steps(&text), steps);
        assert_eq!(parse_steps("open "), []);
    }

    #[test]
    fn copy_and_show_steps_survive_saving() {
        let steps = vec![
            KeyStep::Copy(CopyKind::Files),
            KeyStep::Run(ShellCommand::new(r#"echo "$#""#, true)),
            KeyStep::Show(5),
            KeyStep::Copy(CopyKind::Text),
        ];
        let text = format_steps(&steps);
        assert_eq!(text, r#"copy files; run-with-variables "echo \"$#\""; show 5; copy text"#);
        assert_eq!(parse_steps(&text), steps);
        assert_eq!(parse_steps("copy images; show soon"), []);
        assert_eq!(parse_steps("show 3600"), [KeyStep::Show(MAX_SHOW_SECONDS)]);
        // Written before there was a step to show it, and read as a plain command.
        assert_eq!(parse_steps(r#"run-showing-output "date""#), [KeyStep::Run(ShellCommand::new("date", true))]);
    }

    #[test]
    fn copy_shortcuts_are_known_however_written() {
        assert!(is_copy_shortcut("Control+C"));
        assert!(is_copy_shortcut("ctrl+shift+c"));
        assert!(is_copy_shortcut("Control+X"));
        assert!(is_copy_shortcut("Control_L+Insert"));
        assert!(!is_copy_shortcut("Control+V"));
        assert!(!is_copy_shortcut("C"));
    }

    #[test]
    fn a_copy_is_waited_for_only_by_a_command_given_the_variables() {
        let press = |keys: &str| KeyStep::Press(keys.to_string());
        let run = |command: &str, variables| KeyStep::Run(ShellCommand::new(command, variables));
        let steps = [
            press("Control+C"),
            KeyStep::Wait(100),
            run(r#"upload.sh "$@""#, true),
            press("Control+C"),
            run(r#"upload.sh "$@""#, false),
            press("Control+C"),
            press("Return"),
            run(r#"echo "$GESTOS_CLIPBOARD""#, true),
        ];
        assert_eq!(copies_for_commands(&steps), [0]);
    }

    #[test]
    fn a_command_to_run_survives_saving() {
        let steps = vec![
            KeyStep::Run(r#"~/bin/backup.sh "$HOME/a b"; echo done"#.into()),
            KeyStep::Press("Control+V".to_string()),
            KeyStep::Run(ShellCommand::new(r#"upload.sh "$@""#, true)),
        ];
        let text = format_steps(&steps);
        assert_eq!(
            text,
            r#"run "~/bin/backup.sh \"$HOME/a b\"; echo done"; press Control+V; run-with-variables "upload.sh \"$@\"""#
        );
        assert_eq!(parse_steps(&text), steps);
        let named = [KeyStep::Run("~/bin/backup.sh --now".into())];
        assert_eq!(describe_steps(&named), "run backup.sh --now");
    }

    #[test]
    fn an_app_that_isnt_installed_is_described_by_its_id() {
        let steps = [KeyStep::Open("no.such.App".to_string())];
        assert_eq!(describe_steps(&steps), "open no.such.App");
    }

    #[test]
    fn a_wait_is_kept_short() {
        assert_eq!(parse_steps("wait 99999999"), [KeyStep::Wait(MAX_WAIT_MILLIS)]);
    }

    #[test]
    fn steps_read_as_a_list() {
        assert_eq!(
            describe_steps(&steps()[..3]),
            "Control+L, type “say \"hi\"; C:\\ ok\n”, wait 200 ms"
        );
    }
}
