//! A keyboard action's steps: shortcuts to press, text to type, and pauses, in the order given.
//! They're written on one line, like `press Control+L; type "gestos"; wait 200; press Return`,
//! as gestures keep their actions as text.

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum KeyStep {
    /// Keys pressed together, like `Control+L`.
    Press(String),
    /// Text typed one character at a time, with whatever keys the layout needs for each.
    Type(String),
    /// A pause, in milliseconds, for an app to catch up.
    Wait(u32),
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
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The steps as people read them: `Control+L, type “gestos”, wait 200 ms, Return`.
pub fn describe_steps(steps: &[KeyStep]) -> String {
    steps
        .iter()
        .map(|step| match step {
            KeyStep::Press(keys) => keys.clone(),
            KeyStep::Type(text) => format!("type “{text}”"),
            KeyStep::Wait(millis) => format!("wait {millis} ms"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

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
