//! A custom action's steps: apps to open, shortcuts to press, text to type, and pauses, in the
//! order given. They're written on one line, like
//! `open org.mozilla.firefox; press Control+L; type "gestos"; press Return`, as gestures keep
//! their actions as text.

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
}

/// The most a single pause can be, so that a typo can't leave a gesture running for hours.
pub const MAX_WAIT_MILLIS: u32 = 60_000;

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
        "wait" => {
            let millis = rest.parse::<u32>().ok()?;
            Some(KeyStep::Wait(millis.min(MAX_WAIT_MILLIS)))
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
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The installed apps a person would look for, as their names and desktop file IDs, sorted by
/// name.
pub fn installed_apps() -> Vec<(String, String)> {
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
