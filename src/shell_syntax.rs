//! Which parts of a shell command are what, so that the window a command is written in can color
//! them as it's typed: keywords, quoted text, variables, comments and operators. It only reads the
//! command, as a shell would split it, and never runs anything.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    Quoted,
    Variable,
    Comment,
    Operator,
}

/// A part of the command, from one character to before another, counted in characters as a
/// text buffer counts them. A variable inside double quotes is a part of its own, inside the
/// quoted one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

const KEYWORDS: [&str; 17] = [
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "in", "function", "select", "time", "!",
];

/// Keywords after which a command comes, as `if` and `do` are followed by one.
const BEFORE_COMMAND: [&str; 9] = ["if", "then", "else", "elif", "while", "until", "do", "time", "!"];

/// Keywords after which a name comes, and then `in`.
const BEFORE_NAME: [&str; 3] = ["for", "case", "select"];

fn is_operator(c: char) -> bool {
    matches!(c, '|' | '&' | ';' | '<' | '>' | '(' | ')')
}

fn ends_word(c: char) -> bool {
    c.is_whitespace() || is_operator(c) || matches!(c, '\'' | '"' | '$' | '`')
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Where the variable starting at `start`, a `$`, ends: after its name, its number, its sign, or
/// its braces. `None` for a `$` that starts no variable.
fn variable_end(chars: &[char], start: usize) -> Option<usize> {
    let next = *chars.get(start + 1)?;
    if next == '{' {
        let close = chars[start + 2..].iter().position(|&c| c == '}');
        return Some(close.map_or(chars.len(), |close| start + 2 + close + 1));
    }
    if next.is_ascii_digit() || matches!(next, '?' | '$' | '!' | '#' | '@' | '*' | '-') {
        return Some(start + 2);
    }
    if !is_name_start(next) {
        return None;
    }
    let length = chars[start + 1..].iter().take_while(|&&c| is_name(c)).count();
    Some(start + 1 + length)
}

/// The parts of a command worth coloring, in order.
pub fn spans(command: &str) -> Vec<Span> {
    let chars: Vec<char> = command.chars().collect();
    let mut spans = Vec::new();
    let mut push = |start: usize, end: usize, kind: Kind| {
        if end > start {
            spans.push(Span { start, end, kind });
        }
    };
    // Whether the next word is where a command's name goes, which is the only place a keyword is
    // one: `echo if` prints it.
    let mut at_command = true;
    let mut expects_in = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let word_start = i == 0 || chars[i - 1].is_whitespace() || is_operator(chars[i - 1]);
        match c {
            '\n' => {
                at_command = true;
                i += 1;
            }
            _ if c.is_whitespace() => i += 1,
            '#' if word_start => {
                let end = chars[i..].iter().position(|&c| c == '\n').map_or(chars.len(), |n| i + n);
                push(i, end, Kind::Comment);
                i = end;
            }
            '\\' => {
                i += 2;
                at_command = false;
            }
            '\'' => {
                let close = chars[i + 1..].iter().position(|&c| c == '\'');
                let end = close.map_or(chars.len(), |close| i + 1 + close + 1);
                push(i, end, Kind::Quoted);
                i = end;
                at_command = false;
            }
            '"' => {
                let start = i;
                let mut variables = Vec::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    match chars[i] {
                        '\\' => i += 2,
                        '$' => match variable_end(&chars, i) {
                            Some(end) => {
                                variables.push((i, end));
                                i = end;
                            }
                            None => i += 1,
                        },
                        _ => i += 1,
                    }
                }
                i = (i + 1).min(chars.len());
                push(start, i, Kind::Quoted);
                for (start, end) in variables {
                    push(start, end, Kind::Variable);
                }
                at_command = false;
            }
            '$' => {
                if chars.get(i + 1) == Some(&'(') {
                    push(i, i + 2, Kind::Operator);
                    i += 2;
                    at_command = true;
                } else if let Some(end) = variable_end(&chars, i) {
                    push(i, end, Kind::Variable);
                    i = end;
                    at_command = false;
                } else {
                    i += 1;
                }
            }
            '`' => {
                push(i, i + 1, Kind::Operator);
                i += 1;
                at_command = true;
            }
            _ if is_operator(c) => {
                let length = chars[i..].iter().take_while(|&&c| is_operator(c)).count();
                push(i, i + length, Kind::Operator);
                // After a redirection comes a file, and after the others a command.
                at_command = !matches!(c, '<' | '>');
                expects_in = false;
                i += length;
            }
            _ => {
                let length = chars[i..].iter().take_while(|&&c| !ends_word(c)).count().max(1);
                let word: String = chars[i..i + length].iter().collect();
                if at_command && KEYWORDS.contains(&word.as_str()) || expects_in && word == "in" {
                    push(i, i + length, Kind::Keyword);
                    expects_in = BEFORE_NAME.contains(&word.as_str());
                    at_command = BEFORE_COMMAND.contains(&word.as_str());
                } else if let Some(name) = word.split_once('=').map(|(name, _)| name).filter(|name| {
                    at_command && name.starts_with(is_name_start) && name.chars().all(is_name)
                }) {
                    // An assignment before a command, or on its own.
                    push(i, i + name.chars().count(), Kind::Variable);
                } else {
                    at_command = false;
                }
                i += length;
            }
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(command: &str) -> Vec<(String, Kind)> {
        let chars: Vec<char> = command.chars().collect();
        spans(command)
            .into_iter()
            .map(|span| (chars[span.start..span.end].iter().collect(), span.kind))
            .collect()
    }

    fn part(text: &str, kind: Kind) -> (String, Kind) {
        (text.to_string(), kind)
    }

    #[test]
    fn a_plain_command_has_nothing_to_color() {
        assert!(parts("firefox --new-window").is_empty());
    }

    #[test]
    fn keywords_are_only_keywords_where_a_command_goes() {
        assert_eq!(
            parts("if true; then echo if; fi"),
            [
                part("if", Kind::Keyword),
                part(";", Kind::Operator),
                part("then", Kind::Keyword),
                part(";", Kind::Operator),
                part("fi", Kind::Keyword),
            ]
        );
    }

    #[test]
    fn a_loop_has_its_in_and_its_body() {
        assert_eq!(
            parts("for f in *.png\ndo echo \"$f\"\ndone"),
            [
                part("for", Kind::Keyword),
                part("in", Kind::Keyword),
                part("do", Kind::Keyword),
                part("\"$f\"", Kind::Quoted),
                part("$f", Kind::Variable),
                part("done", Kind::Keyword),
            ]
        );
    }

    #[test]
    fn variables_inside_single_quotes_are_text() {
        assert_eq!(
            parts("echo '$HOME' \"${HOME}/x\" $1 $?"),
            [
                part("'$HOME'", Kind::Quoted),
                part("\"${HOME}/x\"", Kind::Quoted),
                part("${HOME}", Kind::Variable),
                part("$1", Kind::Variable),
                part("$?", Kind::Variable),
            ]
        );
    }

    #[test]
    fn a_comment_starts_a_word_and_runs_to_the_end_of_the_line() {
        assert_eq!(
            parts("echo a#b # note\nls"),
            [part("# note", Kind::Comment)]
        );
    }

    #[test]
    fn operators_and_substitutions_are_marked() {
        assert_eq!(
            parts("ls | grep x && echo $(date) > out"),
            [
                part("|", Kind::Operator),
                part("&&", Kind::Operator),
                part("$(", Kind::Operator),
                part(")", Kind::Operator),
                part(">", Kind::Operator),
            ]
        );
    }

    #[test]
    fn an_assignment_names_its_variable() {
        assert_eq!(
            parts("LANG=C sort file"),
            [part("LANG", Kind::Variable)]
        );
        assert!(parts("echo a=b").is_empty());
    }

    #[test]
    fn what_isnt_finished_yet_runs_to_the_end() {
        assert_eq!(parts("echo 'unclosed"), [part("'unclosed", Kind::Quoted)]);
        assert_eq!(parts("echo \"unclosed $x"), [
            part("\"unclosed $x", Kind::Quoted),
            part("$x", Kind::Variable),
        ]);
        assert_eq!(parts("echo ${open"), [part("${open", Kind::Variable)]);
        assert!(parts("echo \\").is_empty());
    }

    #[test]
    fn spans_count_characters_not_bytes() {
        let found = spans("echo \"é\" $x");
        assert_eq!(found.last(), Some(&Span { start: 9, end: 11, kind: Kind::Variable }));
    }
}
