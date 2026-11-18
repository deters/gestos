//! Which parts of a shell command are what, so that the window a command is written in can color
//! them as it's typed: keywords, function names, quoted text, variables, comments and operators. It only reads the
//! command, as a shell would split it, and never runs anything.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    /// A function's name, where it's defined and where it's called after that.
    Function,
    Quoted,
    Variable,
    Comment,
    Operator,
}

/// A part of the command, from one character to before another, counted in characters as a
/// text buffer counts them.
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

/// A command inside another, as `$(…)` or `` `…` `` are, from its opening to after its close,
/// and how deep it is: 1 inside the command, 2 inside one of those, and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inner {
    pub start: usize,
    pub end: usize,
    pub depth: usize,
}

/// The parts of a command worth coloring, in order. Quoted text is cut around what's colored
/// inside it, so a variable or a command in double quotes is shown as itself.
pub fn spans(command: &str) -> Vec<Span> {
    parse(command).0
}

/// The commands inside a command, each after the ones inside it.
pub fn inner_commands(command: &str) -> Vec<Inner> {
    parse(command).1
}

fn parse(command: &str) -> (Vec<Span>, Vec<Inner>) {
    let chars: Vec<char> = command.chars().collect();
    let mut parser = Parser {
        chars: &chars,
        spans: Vec::new(),
        inner: Vec::new(),
        depth: 0,
        functions: Vec::new(),
    };
    parser.commands(0, None);
    (parser.spans, parser.inner)
}

struct Parser<'a> {
    chars: &'a [char],
    spans: Vec<Span>,
    inner: Vec<Inner>,
    depth: usize,
    /// The functions defined so far, whose calls are colored too.
    functions: Vec<String>,
}

impl Parser<'_> {
    /// Reads a command inside another, from `start`, its opening `$(` or `` ` `` that's `opening`
    /// long, to the `close` that ends it. Where it ended.
    fn inner_command(&mut self, start: usize, opening: usize, close: char) -> usize {
        self.push(start, start + opening, Kind::Operator);
        self.depth += 1;
        let end = self.commands(start + opening, Some(close));
        self.inner.push(Inner { start, end: end.min(self.chars.len()), depth: self.depth });
        self.depth -= 1;
        end
    }

    fn push(&mut self, start: usize, end: usize, kind: Kind) {
        let end = end.min(self.chars.len());
        if end > start {
            self.spans.push(Span { start, end, kind });
        }
    }

    /// Reads commands from `i` to the end, or, inside `$(…)` or `` `…` ``, to the `close` that
    /// ends it. Where it stopped, after that close.
    fn commands(&mut self, mut i: usize, close: Option<char>) -> usize {
        let chars = self.chars;
        // Whether the next word is where a command's name goes, which is the only place a keyword
        // is one: `echo if` prints it.
        let mut at_command = true;
        let mut expects_in = false;
        let mut expects_function = false;
        // Parentheses opened inside `$(…)`, which its `)` comes after.
        let mut open = 0u32;
        while i < chars.len() {
            let c = chars[i];
            let word_start = i == 0 || chars[i - 1].is_whitespace() || is_operator(chars[i - 1]);
            if Some(c) == close && (c == '`' || open == 0) {
                self.push(i, i + 1, Kind::Operator);
                return i + 1;
            }
            match c {
                '\n' => {
                    at_command = true;
                    i += 1;
                }
                _ if c.is_whitespace() => i += 1,
                '#' if word_start => {
                    let end = chars[i..].iter().position(|&c| c == '\n').map_or(chars.len(), |n| i + n);
                    self.push(i, end, Kind::Comment);
                    i = end;
                }
                '\\' => {
                    i += 2;
                    at_command = false;
                }
                '\'' => {
                    let close = chars[i + 1..].iter().position(|&c| c == '\'');
                    let end = close.map_or(chars.len(), |close| i + 1 + close + 1);
                    self.push(i, end, Kind::Quoted);
                    i = end;
                    at_command = false;
                }
                '"' => {
                    i = self.double_quoted(i);
                    at_command = false;
                }
                '$' if chars.get(i + 1) == Some(&'(') => {
                    i = self.inner_command(i, 2, ')');
                    at_command = false;
                }
                '$' => match variable_end(chars, i) {
                    Some(end) => {
                        self.push(i, end, Kind::Variable);
                        i = end;
                        at_command = false;
                    }
                    None => i += 1,
                },
                '`' => {
                    i = self.inner_command(i, 1, '`');
                    at_command = false;
                }
                '(' | ')' => {
                    if c == '(' {
                        open += 1;
                    } else {
                        open = open.saturating_sub(1);
                    }
                    self.push(i, i + 1, Kind::Operator);
                    at_command = c == '(';
                    i += 1;
                }
                _ if is_operator(c) => {
                    let length = chars[i..]
                        .iter()
                        .take_while(|&&c| is_operator(c) && !matches!(c, '(' | ')'))
                        .count();
                    self.push(i, i + length, Kind::Operator);
                    // After a redirection comes a file, and after the others a command.
                    at_command = !matches!(c, '<' | '>');
                    expects_in = false;
                    i += length;
                }
                _ => {
                    let length = chars[i..].iter().take_while(|&&c| !ends_word(c)).count().max(1);
                    let word: String = chars[i..i + length].iter().collect();
                    // `name() {`, with spaces or not before and inside the parentheses.
                    let after: String = chars[i + length..].iter().filter(|c| **c != ' ').take(2).collect();
                    let defined = expects_function || at_command && after == "()" && word.chars().all(|c| is_name(c) || c == '-');
                    if defined {
                        self.push(i, i + length, Kind::Function);
                        if !self.functions.contains(&word) {
                            self.functions.push(word);
                        }
                        expects_function = false;
                        at_command = false;
                    } else if at_command && KEYWORDS.contains(&word.as_str()) || expects_in && word == "in" {
                        self.push(i, i + length, Kind::Keyword);
                        expects_in = BEFORE_NAME.contains(&word.as_str());
                        expects_function = word == "function";
                        at_command = BEFORE_COMMAND.contains(&word.as_str());
                    } else if at_command && self.functions.contains(&word) {
                        self.push(i, i + length, Kind::Function);
                        at_command = false;
                    } else if word == "{" || word == "}" {
                        // A function's body, or a group of commands, starts with a command.
                        at_command = true;
                    } else if let Some(name) = word.split_once('=').map(|(name, _)| name).filter(|name| {
                        at_command && name.starts_with(is_name_start) && name.chars().all(is_name)
                    }) {
                        // An assignment before a command, or on its own.
                        self.push(i, i + name.chars().count(), Kind::Variable);
                    } else {
                        at_command = false;
                    }
                    i += length;
                }
            }
        }
        i
    }

    /// Reads double-quoted text starting at `start`, its opening quote: the text itself, and the
    /// variables and commands in it, which a shell fills in. Where it ended, after its closing
    /// quote.
    fn double_quoted(&mut self, start: usize) -> usize {
        let chars = self.chars;
        let mut text = start;
        let mut i = start + 1;
        while i < chars.len() {
            match chars[i] {
                '"' => {
                    i += 1;
                    break;
                }
                '\\' => i += 2,
                '$' if chars.get(i + 1) == Some(&'(') => {
                    self.push(text, i, Kind::Quoted);
                    i = self.inner_command(i, 2, ')');
                    text = i;
                }
                '`' => {
                    self.push(text, i, Kind::Quoted);
                    i = self.inner_command(i, 1, '`');
                    text = i;
                }
                '$' => match variable_end(chars, i) {
                    Some(end) => {
                        self.push(text, i, Kind::Quoted);
                        self.push(i, end, Kind::Variable);
                        i = end;
                        text = i;
                    }
                    None => i += 1,
                },
                _ => i += 1,
            }
        }
        let end = i.min(chars.len());
        self.push(text, end, Kind::Quoted);
        end
    }
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
                part("\"", Kind::Quoted),
                part("$f", Kind::Variable),
                part("\"", Kind::Quoted),
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
                part("\"", Kind::Quoted),
                part("${HOME}", Kind::Variable),
                part("/x\"", Kind::Quoted),
                part("$1", Kind::Variable),
                part("$?", Kind::Variable),
            ]
        );
    }

    #[test]
    fn a_command_inside_quotes_is_shell_with_quotes_of_its_own() {
        assert_eq!(
            parts("fail \"$(basename \"$f\") isn't a PDF\""),
            [
                part("\"", Kind::Quoted),
                part("$(", Kind::Operator),
                part("\"", Kind::Quoted),
                part("$f", Kind::Variable),
                part("\"", Kind::Quoted),
                part(")", Kind::Operator),
                part(" isn't a PDF\"", Kind::Quoted),
            ]
        );
    }

    #[test]
    fn commands_inside_commands_end_where_theirs_do() {
        assert_eq!(
            parts("echo \"`date` $(ls | wc -l) (all)\"; if"),
            [
                part("\"", Kind::Quoted),
                part("`", Kind::Operator),
                part("`", Kind::Operator),
                part(" ", Kind::Quoted),
                part("$(", Kind::Operator),
                part("|", Kind::Operator),
                part(")", Kind::Operator),
                part(" (all)\"", Kind::Quoted),
                part(";", Kind::Operator),
                part("if", Kind::Keyword),
            ]
        );
        assert_eq!(
            parts("x=$( (cd /; ls) )"),
            [
                part("x", Kind::Variable),
                part("$(", Kind::Operator),
                part("(", Kind::Operator),
                part(";", Kind::Operator),
                part(")", Kind::Operator),
                part(")", Kind::Operator),
            ]
        );
    }

    #[test]
    fn a_keyword_inside_a_command_in_quotes_is_one() {
        assert_eq!(
            parts("echo \"$(if true; then echo y; fi)\""),
            [
                part("\"", Kind::Quoted),
                part("$(", Kind::Operator),
                part("if", Kind::Keyword),
                part(";", Kind::Operator),
                part("then", Kind::Keyword),
                part(";", Kind::Operator),
                part("fi", Kind::Keyword),
                part(")", Kind::Operator),
                part("\"", Kind::Quoted),
            ]
        );
    }

    #[test]
    fn functions_are_named_where_defined_and_where_called() {
        assert_eq!(
            parts("fail() { echo \"$*\" >&2; exit 1; }\nfunction show { echo; }\nfail x; echo fail; show"),
            [
                part("fail", Kind::Function),
                part("(", Kind::Operator),
                part(")", Kind::Operator),
                part("\"", Kind::Quoted),
                part("$*", Kind::Variable),
                part("\"", Kind::Quoted),
                part(">&", Kind::Operator),
                part(";", Kind::Operator),
                part(";", Kind::Operator),
                part("function", Kind::Keyword),
                part("show", Kind::Function),
                part(";", Kind::Operator),
                part("fail", Kind::Function),
                part(";", Kind::Operator),
                part(";", Kind::Operator),
                part("show", Kind::Function),
            ]
        );
        assert_eq!(
            parts("size() { du -h \"$1\"; }\nshow \"$(size x)\""),
            [
                part("size", Kind::Function),
                part("(", Kind::Operator),
                part(")", Kind::Operator),
                part("\"", Kind::Quoted),
                part("$1", Kind::Variable),
                part("\"", Kind::Quoted),
                part(";", Kind::Operator),
                part("\"", Kind::Quoted),
                part("$(", Kind::Operator),
                part("size", Kind::Function),
                part(")", Kind::Operator),
                part("\"", Kind::Quoted),
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
            part("\"unclosed ", Kind::Quoted),
            part("$x", Kind::Variable),
        ]);
        assert_eq!(parts("echo ${open"), [part("${open", Kind::Variable)]);
        assert!(parts("echo \\").is_empty());
    }

    #[test]
    fn inner_commands_say_how_deep_they_are() {
        let command = "echo \"$(ls $(pwd))\" `date`";
        let chars: Vec<char> = command.chars().collect();
        let found: Vec<(String, usize)> = inner_commands(command)
            .into_iter()
            .map(|inner| (chars[inner.start..inner.end].iter().collect(), inner.depth))
            .collect();
        assert_eq!(
            found,
            [("$(pwd)".to_string(), 2), ("$(ls $(pwd))".to_string(), 1), ("`date`".to_string(), 1)]
        );
        assert_eq!(inner_commands("echo $(unclosed")[0], Inner { start: 5, end: 15, depth: 1 });
    }

    #[test]
    fn every_template_is_colored_in_order_and_within_its_text() {
        let templates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("action-templates");
        for entry in std::fs::read_dir(templates).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            let length = text.chars().count();
            let found = spans(&text);
            assert!(found.windows(2).all(|pair| pair[0].end <= pair[1].start));
            assert!(found.iter().all(|span| span.start < span.end && span.end <= length));
        }
    }

    #[test]
    fn spans_count_characters_not_bytes() {
        let found = spans("echo \"é\" $x");
        assert_eq!(found.last(), Some(&Span { start: 9, end: 11, kind: Kind::Variable }));
    }
}
