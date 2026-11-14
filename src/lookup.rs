//! What to do with text selected anywhere: open a web address, an email address or a file, work out
//! a sum, or search the web for anything else.

use std::path::{Path, PathBuf};

/// Where text that's none of the others is looked up.
pub const SEARCH_URL: &str = "https://duckduckgo.com/?q=";

/// Longer selections are cut to this many characters to search for, as a whole page selected by
/// mistake would make an address too long.
const LONGEST_SEARCH: usize = 300;

/// The endings of web addresses written without `https://` or `www.`, which few file names have.
const WEB_ENDINGS: &[&str] = &[
    "com", "org", "net", "io", "dev", "app", "edu", "gov", "info", "br", "pt", "de", "uk", "fr",
    "es", "it", "nl", "eu", "us", "ca", "au", "jp", "co",
];

#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// A web address to open, with its scheme.
    Web(String),
    Email(String),
    File(PathBuf),
    /// A sum as it was written, and what it comes to.
    Sum(String, String),
    /// A web address that searches for the text.
    Search(String),
}

/// What to do with the text selected, or None if nothing is. A path starting with `~` is in
/// `home`.
pub fn lookup_for(text: &str, home: Option<&Path>) -> Option<Lookup> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(path) = existing_file(text, home) {
        return Some(Lookup::File(path));
    }
    if let Some(url) = web_address(text) {
        return Some(Lookup::Web(url));
    }
    if is_email(text) {
        return Some(Lookup::Email(text.to_string()));
    }
    if let Some(result) = sum(text) {
        return Some(Lookup::Sum(text.to_string(), result));
    }
    let query: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let query: String = query.chars().take(LONGEST_SEARCH).collect();
    Some(Lookup::Search(format!("{SEARCH_URL}{}", percent_encode(&query))))
}

fn existing_file(text: &str, home: Option<&Path>) -> Option<PathBuf> {
    let path = match text.strip_prefix("~/") {
        Some(rest) => home?.join(rest),
        None if text.starts_with('/') => PathBuf::from(text),
        None => return None,
    };
    path.exists().then_some(path)
}

fn web_address(text: &str) -> Option<String> {
    if text.contains(char::is_whitespace) {
        return None;
    }
    let lower = text.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(text.to_string());
    }
    if lower.starts_with("www.") && text.len() > 4 {
        return Some(format!("https://{text}"));
    }
    // A bare address, like example.com or example.com/page.
    let host = lower.split(['/', '?', '#']).next()?;
    let ending = host.rsplit_once('.')?.1;
    let labels_ok = host.split('.').all(|label| {
        !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    (labels_ok && WEB_ENDINGS.contains(&ending)).then(|| format!("https://{text}"))
}

fn is_email(text: &str) -> bool {
    let Some((name, domain)) = text.split_once('@') else { return false };
    !name.is_empty()
        && !text.contains(char::is_whitespace)
        && !domain.contains('@')
        && domain.split('.').count() >= 2
        && domain.split('.').all(|label| !label.is_empty())
}

/// What a sum comes to, written as briefly as it can be, or None if the text isn't one. A number
/// alone isn't a sum.
pub fn sum(text: &str) -> Option<String> {
    // Dates and phone numbers, like 2026-11-14 or 555-0123, are numbers joined by dashes.
    if text.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '/') {
        return None;
    }
    let tokens = tokenize(text)?;
    if !tokens.iter().any(|token| matches!(token, Token::Op(_))) {
        return None;
    }
    let mut parser = Parser { tokens: &tokens, at: 0 };
    let value = parser.expression()?;
    if parser.at != tokens.len() || !value.is_finite() {
        return None;
    }
    Some(format_number(value))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Number(f64),
    Op(char),
    Open,
    Close,
}

fn tokenize(text: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => {
                chars.next();
            }
            '0'..='9' | '.' | ',' => {
                let mut number = String::new();
                while let Some(&d) = chars.peek() {
                    match d {
                        '0'..='9' | '.' => number.push(d),
                        // A decimal comma, as in 2,5.
                        ',' => number.push('.'),
                        _ => break,
                    }
                    chars.next();
                }
                tokens.push(Token::Number(number.parse().ok()?));
            }
            '+' | '-' | '*' | '/' | '^' | '%' => {
                tokens.push(Token::Op(c));
                chars.next();
            }
            'x' | 'X' | '×' | '·' => {
                tokens.push(Token::Op('*'));
                chars.next();
            }
            '÷' | ':' => {
                tokens.push(Token::Op('/'));
                chars.next();
            }
            '(' => {
                tokens.push(Token::Open);
                chars.next();
            }
            ')' => {
                tokens.push(Token::Close);
                chars.next();
            }
            // Ends a sum written with its result, like "2 + 2 =".
            '=' if chars.clone().skip(1).all(char::is_whitespace) => break,
            _ => return None,
        }
    }
    (!tokens.is_empty()).then_some(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
}

impl Parser<'_> {
    fn next_is(&self, op: char) -> bool {
        self.tokens.get(self.at) == Some(&Token::Op(op))
    }

    fn expression(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        loop {
            if self.next_is('+') {
                self.at += 1;
                value += self.term()?;
            } else if self.next_is('-') {
                self.at += 1;
                value -= self.term()?;
            } else {
                return Some(value);
            }
        }
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.power()?;
        loop {
            if self.next_is('*') {
                self.at += 1;
                value *= self.power()?;
            } else if self.next_is('/') {
                self.at += 1;
                value /= self.power()?;
            } else if self.next_is('%') {
                self.at += 1;
                value %= self.power()?;
            } else {
                return Some(value);
            }
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.next_is('^') {
            self.at += 1;
            // Right to left, as 2^3^2 is 2^9.
            return Some(base.powf(self.power()?));
        }
        Some(base)
    }

    fn unary(&mut self) -> Option<f64> {
        if self.next_is('-') {
            self.at += 1;
            return Some(-self.unary()?);
        }
        if self.next_is('+') {
            self.at += 1;
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Option<f64> {
        match *self.tokens.get(self.at)? {
            Token::Number(n) => {
                self.at += 1;
                Some(n)
            }
            Token::Open => {
                self.at += 1;
                let value = self.expression()?;
                (self.tokens.get(self.at) == Some(&Token::Close)).then(|| self.at += 1)?;
                Some(value)
            }
            _ => None,
        }
    }
}

/// A number as briefly as it can be written: whole numbers without decimals, others with as many
/// as they need, up to ten.
fn format_number(value: f64) -> String {
    if value == value.trunc() && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.10}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Text made safe to put in a web address, as a search for it.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look_up(text: &str) -> Option<Lookup> {
        lookup_for(text, None)
    }

    #[test]
    fn web_addresses_open_with_or_without_their_scheme() {
        assert_eq!(look_up("https://gnome.org/a?b=1"), Some(Lookup::Web("https://gnome.org/a?b=1".into())));
        assert_eq!(look_up(" www.gnome.org "), Some(Lookup::Web("https://www.gnome.org".into())));
        assert_eq!(look_up("gnome.org/apps"), Some(Lookup::Web("https://gnome.org/apps".into())));
        // A file name looks like an address, but doesn't end like one.
        assert!(matches!(look_up("notes.txt"), Some(Lookup::Search(_))));
    }

    #[test]
    fn an_email_address_is_written_to() {
        assert_eq!(look_up("me@example.com"), Some(Lookup::Email("me@example.com".into())));
        assert!(matches!(look_up("me@localhost"), Some(Lookup::Search(_))));
    }

    #[test]
    fn a_file_that_exists_opens() {
        assert_eq!(look_up("/"), Some(Lookup::File(PathBuf::from("/"))));
        assert!(matches!(look_up("/no/such/file/here"), Some(Lookup::Search(_))));
        let home = Path::new("/");
        assert_eq!(lookup_for("~/tmp", Some(home)), Some(Lookup::File(PathBuf::from("/tmp"))));
    }

    #[test]
    fn sums_are_worked_out() {
        assert_eq!(sum("2 + 2"), Some("4".into()));
        assert_eq!(sum("(1 + 2) * 3 - 4 / 2"), Some("7".into()));
        assert_eq!(sum("2^3^2"), Some("512".into()));
        assert_eq!(sum("-3 x 4"), Some("-12".into()));
        assert_eq!(sum("2,5 × 2"), Some("5".into()));
        assert_eq!(sum("10 / 4 ="), Some("2.5".into()));
        assert_eq!(sum("1 / 3"), Some("0.3333333333".into()));
        assert_eq!(sum("7 % 3"), Some("1".into()));
        assert_eq!(sum("42"), None);
        assert_eq!(sum("2026-11-14"), None);
        assert_eq!(sum("14/11/2026"), None);
        assert_eq!(sum("1 / 0"), None);
        assert_eq!(sum("2 +"), None);
        assert_eq!(sum("(2 + 3"), None);
        assert_eq!(look_up("6 * 7"), Some(Lookup::Sum("6 * 7".into(), "42".into())));
    }

    #[test]
    fn anything_else_is_searched_for() {
        assert_eq!(
            look_up("  mouse   gestures & linux "),
            Some(Lookup::Search(format!("{SEARCH_URL}mouse+gestures+%26+linux")))
        );
        assert_eq!(look_up("   "), None);
        let long = "a ".repeat(400);
        let Some(Lookup::Search(url)) = look_up(&long) else { panic!() };
        assert!(url.len() < SEARCH_URL.len() + LONGEST_SEARCH + 10);
    }
}
