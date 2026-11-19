//! What a command run by a gesture is given: what's copied and selected when it runs, the files
//! copied in a file manager, the app in use and the gesture's name. Each is an environment
//! variable, never put into the command's text, so that nothing copied can change what it runs.
//! Copied files are also its arguments, `"$@"`, and the variables a file manager gives its own
//! scripts, so that scripts written for one work too. A command only gets them when the user asks
//! for them, so that the apps others open don't keep a copy of a password for as long as they run.

use std::path::PathBuf;

/// A command run in a shell, and whether it's given the variables.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ShellCommand {
    pub text: String,
    pub variables: bool,
}

impl ShellCommand {
    pub fn new(text: impl Into<String>, variables: bool) -> Self {
        Self { text: text.into(), variables }
    }

    /// The program its first line names to run it, as a script's `#!` line does, if it names one.
    pub fn interpreter(&self) -> Option<Interpreter> {
        let line = self.text.trim_start().lines().next()?.strip_prefix("#!")?.trim();
        // As the kernel reads it: the program, then all the rest as one argument.
        let (program, argument) = match line.split_once(char::is_whitespace) {
            Some((program, rest)) => (program, Some(rest.trim().to_string())),
            None => (line, None),
        };
        (!program.is_empty()).then(|| Interpreter { program: program.to_string(), argument })
    }

    /// Whether a shell runs it, which reads the variables as the editor writes them: sh, unless
    /// its first line names a program that isn't one.
    pub fn is_shell(&self) -> bool {
        self.interpreter().is_none_or(|interpreter| SHELLS.contains(&interpreter.name()))
    }
}

/// The shells that read `"$@"` and `"$GESTOS_CLIPBOARD"` as sh does.
const SHELLS: [&str; 7] = ["sh", "bash", "dash", "zsh", "ksh", "mksh", "ash"];

/// The program a script's first line names to run it, and the one argument that line can give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interpreter {
    pub program: String,
    pub argument: Option<String>,
}

impl Interpreter {
    /// The program's name, as said when it's missing: the one `env` looks for, in
    /// `#!/usr/bin/env python3`.
    pub fn name(&self) -> &str {
        fn file_name(path: &str) -> &str {
            path.rsplit('/').next().unwrap_or(path)
        }
        let program = file_name(&self.program);
        let looked_for = self.argument.as_deref().and_then(|argument| {
            argument.split_whitespace().find(|word| !word.starts_with('-'))
        });
        match looked_for {
            Some(looked_for) if program == "env" => file_name(looked_for),
            _ => program,
        }
    }
}

/// A command without the variables, as commands are unless the user asks for them.
impl From<&str> for ShellCommand {
    fn from(text: &str) -> Self {
        Self::new(text, false)
    }
}

impl From<String> for ShellCommand {
    fn from(text: String) -> Self {
        Self::new(text, false)
    }
}

impl std::ops::Deref for ShellCommand {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

impl std::fmt::Display for ShellCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// Text an advanced action types, and whether the variables in it are filled in, written as
/// `{{clipboard}}`. Without them, it's typed as it's written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TypedText {
    pub text: String,
    pub variables: bool,
}

impl TypedText {
    pub fn new(text: impl Into<String>, variables: bool) -> Self {
        Self { text: text.into(), variables }
    }

    /// Whether it types what's copied or selected, which is then read before it's typed.
    pub fn reads_copied(&self) -> bool {
        self.variables && self.used().any(|name| COPIED_TYPED_VARIABLES.contains(&name))
    }

    /// The names of the variables it uses, known or not.
    fn used(&self) -> impl Iterator<Item = &str> {
        self.text.split("{{").skip(1).filter_map(|after| Some(after.split_once("}}")?.0.trim()))
    }

    /// The text with each variable it uses written as its value. Braces around anything else are
    /// typed as they are.
    pub fn filled(&self, variables: &[(&str, String)]) -> String {
        if !self.variables {
            return self.text.clone();
        }
        let mut filled = String::new();
        let mut rest = self.text.as_str();
        while let Some(start) = rest.find("{{") {
            filled.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let value = after.split_once("}}").and_then(|(name, after_name)| {
                let (_, value) = variables.iter().find(|(known, _)| *known == name.trim())?;
                Some((value, after_name))
            });
            match value {
                Some((value, after_name)) => {
                    filled.push_str(value);
                    rest = after_name;
                }
                None => {
                    filled.push_str("{{");
                    rest = after;
                }
            }
        }
        filled.push_str(rest);
        filled
    }
}

/// What text to type can have filled in, by the names it's written with: what a command is given
/// as its variables, and what the command before it printed.
pub fn typed_variables(context: &CommandContext, copied: &Copied, result: &str) -> Vec<(&'static str, String)> {
    let given = variables(context, copied);
    let value = |name: &str| given.iter().find(|(n, _)| *n == name).map(|(_, value)| value.clone()).unwrap_or_default();
    vec![
        ("clipboard", value("GESTOS_CLIPBOARD")),
        ("selection", value("GESTOS_SELECTION")),
        ("files", value("GESTOS_FILES")),
        ("result", result.to_string()),
        ("app", value("GESTOS_APP")),
        ("gesture", value("GESTOS_GESTURE")),
    ]
}

impl From<&str> for TypedText {
    fn from(text: &str) -> Self {
        Self::new(text, false)
    }
}

impl std::ops::Deref for TypedText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

/// What a command is told about its gesture, besides what's copied when it runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandContext {
    pub gesture: String,
    /// The app the gesture was drawn in, by its desktop file ID, if it's known.
    pub app: Option<String>,
}

/// What's copied and selected when a command runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Copied {
    pub clipboard: String,
    pub selection: String,
    /// The files copied, as a list of URIs, empty if none are. File managers copy them as
    /// text/uri-list, or only in GNOME's or MATE's own form, which starts with a line saying
    /// whether they're copied or cut.
    pub uri_list: String,
}

/// The variables a command can use, as offered in the editor: what each holds, and how it's
/// written in a command, quoted so that spaces don't split it.
pub const OFFERED: [(&str, &str); 6] = [
    ("Clipboard", "\"$GESTOS_CLIPBOARD\""),
    ("Selected Text", "\"$GESTOS_SELECTION\""),
    ("Copied Files", "\"$@\""),
    ("Copied Files, One per Line", "\"$GESTOS_FILES\""),
    ("App in Use", "\"$GESTOS_APP\""),
    ("Gesture Name", "\"$GESTOS_GESTURE\""),
];

/// The variables of text to type that hold what's copied or selected, which is only read for text
/// that uses them.
const COPIED_TYPED_VARIABLES: [&str; 3] = ["clipboard", "selection", "files"];

/// The variables text to type can use, as offered in the editor, as `OFFERED` is for commands.
pub const OFFERED_FOR_TYPING: [(&str, &str); 6] = [
    ("Clipboard", "{{clipboard}}"),
    ("Selected Text", "{{selection}}"),
    ("Copied Files, One per Line", "{{files}}"),
    ("Result of the Command Before", "{{result}}"),
    ("App in Use", "{{app}}"),
    ("Gesture Name", "{{gesture}}"),
];

/// What Nautilus, Nemo and Caja call the files selected, when they run a script: their paths,
/// and their URIs.
const FILE_MANAGER_VARIABLES: [(&str, &str); 3] = [
    ("NAUTILUS_SCRIPT_SELECTED_FILE_PATHS", "NAUTILUS_SCRIPT_SELECTED_URIS"),
    ("NEMO_SCRIPT_SELECTED_FILE_PATHS", "NEMO_SCRIPT_SELECTED_URIS"),
    ("CAJA_SCRIPT_SELECTED_FILE_PATHS", "CAJA_SCRIPT_SELECTED_URIS"),
];

/// The most an environment variable can hold, with its name, `=` and the end, on Linux.
const MAX_VARIABLE: usize = 128 * 1024;

/// What file managers copy files as, the first one most offer, then the ones that some offer
/// alone: Nemo and Thunar GNOME's own, and Caja MATE's.
pub const COPIED_FILE_TYPES: [&str; 3] =
    ["text/uri-list", "x-special/gnome-copied-files", "x-special/mate-copied-files"];

/// The URIs in a list of them, as text/uri-list has them: one per line, with comments. GNOME's
/// and MATE's own lists start with a line that says "copy" or "cut", which isn't one.
pub fn uris(uri_list: &str) -> Vec<String> {
    uri_list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !matches!(*line, "copy" | "cut"))
        .map(str::to_string)
        .collect()
}

/// The local paths of copied files. Those with none, like a folder on a server that isn't
/// mounted, only have their URIs.
pub fn file_paths(uris: &[String]) -> Vec<PathBuf> {
    use gio::prelude::FileExt;
    uris.iter().filter_map(|uri| gio::File::for_uri(uri).path()).collect()
}

/// The variables a command is given, by name. Those for a file manager's scripts are only there
/// when files are copied, as scripts look for them to tell whether they were run from one.
pub fn variables(context: &CommandContext, copied: &Copied) -> Vec<(&'static str, String)> {
    let mut variables = vec![
        ("GESTOS_APP", context.app.clone().unwrap_or_default()),
        ("GESTOS_GESTURE", context.gesture.clone()),
    ];
    let uris = uris(&copied.uri_list);
    let paths: Vec<String> =
        file_paths(&uris).iter().map(|path| path.to_string_lossy().into_owned()).collect();
    variables.extend([
        ("GESTOS_CLIPBOARD", copied.clipboard.clone()),
        ("GESTOS_SELECTION", copied.selection.clone()),
        ("GESTOS_FILES", paths.join("\n")),
    ]);
    if !uris.is_empty() {
        // As the file managers write them: each followed by a line break.
        let lines = |items: &[String]| items.iter().map(|item| format!("{item}\n")).collect::<String>();
        for (paths_name, uris_name) in FILE_MANAGER_VARIABLES {
            variables.push((paths_name, lines(&paths)));
            variables.push((uris_name, lines(&uris)));
        }
    }
    // A variable can't hold a null character.
    for (_, value) in &mut variables {
        value.retain(|c| c != '\0');
    }
    variables
}

/// The program that reads what's copied, outside GNOME, where the extension does: its name, and
/// the package it's installed with.
pub fn clipboard_reader(is_x11: bool) -> (&'static str, &'static str) {
    if is_x11 {
        ("xclip", "xclip")
    } else {
        ("wl-paste", "wl-clipboard")
    }
}

/// The package to install for commands to get what's copied, if it isn't installed.
pub fn missing_clipboard_reader() -> Option<&'static str> {
    if crate::config::get_environment_suffix() == Some("gnome") {
        return None;
    }
    let (program, package) = clipboard_reader(crate::active_app::is_x11_session());
    (!crate::wayland::is_installed(program)).then_some(package)
}

/// Whether a variable is too big to give a command, which then wouldn't start at all.
pub fn too_big(name: &str, value: &str) -> bool {
    name.len() + value.len() + 2 > MAX_VARIABLE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(variables: &'a [(&str, String)], name: &str) -> Option<&'a str> {
        variables.iter().find(|(n, _)| *n == name).map(|(_, value)| value.as_str())
    }

    #[test]
    fn a_script_names_the_program_that_runs_it_on_its_first_line() {
        let interpreter = |text: &str| ShellCommand::from(text).interpreter();
        assert_eq!(interpreter("echo hi"), None);
        assert_eq!(interpreter("echo hi\n#!/bin/bash"), None);
        let bash = interpreter("#!/bin/bash\necho hi").unwrap();
        assert_eq!((bash.program.as_str(), bash.argument.as_deref(), bash.name()), ("/bin/bash", None, "bash"));
        // All after the program is one argument, as the kernel gives it.
        let env = interpreter("#! /usr/bin/env -S python3 -u \nprint()").unwrap();
        assert_eq!((env.program.as_str(), env.argument.as_deref(), env.name()), ("/usr/bin/env", Some("-S python3 -u"), "python3"));
        assert_eq!(interpreter("#!\necho hi"), None);
    }

    #[test]
    fn only_a_shell_reads_the_variables_as_the_editor_writes_them() {
        assert!(ShellCommand::from("echo \"$@\"").is_shell());
        assert!(ShellCommand::from("#!/bin/sh\necho \"$@\"").is_shell());
        assert!(ShellCommand::from("#!/usr/bin/env bash\necho \"$@\"").is_shell());
        assert!(!ShellCommand::from("#!/usr/bin/python3\nprint()").is_shell());
        assert!(!ShellCommand::from("#!/usr/bin/env python3\nprint()").is_shell());
    }

    #[test]
    fn copied_files_are_read_from_a_list_of_uris() {
        let list = "# copied\r\nfile:///home/me/a%20b.txt\r\n\r\nsftp://server/x\r\n";
        let uris = uris(list);
        assert_eq!(uris, ["file:///home/me/a%20b.txt", "sftp://server/x"]);
        assert_eq!(file_paths(&uris), [PathBuf::from("/home/me/a b.txt")]);
    }

    #[test]
    fn files_copied_in_gnomes_or_mates_own_form_are_read_too() {
        assert_eq!(uris("copy\nfile:///tmp/a\nfile:///tmp/b"), ["file:///tmp/a", "file:///tmp/b"]);
        assert_eq!(uris("cut\nfile:///tmp/a\n"), ["file:///tmp/a"]);
        assert!(uris("copy\n").is_empty());
    }

    #[test]
    fn a_command_is_told_what_is_copied_and_where() {
        let context = CommandContext { gesture: "Upload".into(), app: Some("org.gnome.Nautilus".into()) };
        let copied = Copied {
            clipboard: "file:///tmp/a\nfile:///tmp/b".into(),
            selection: "picked\0text".into(),
            uri_list: "file:///tmp/a\nfile:///tmp/b\n".into(),
        };
        let variables = variables(&context, &copied);
        assert_eq!(value(&variables, "GESTOS_SELECTION"), Some("pickedtext"));
        assert_eq!(value(&variables, "GESTOS_FILES"), Some("/tmp/a\n/tmp/b"));
        assert_eq!(value(&variables, "GESTOS_APP"), Some("org.gnome.Nautilus"));
        assert_eq!(value(&variables, "GESTOS_GESTURE"), Some("Upload"));
        assert_eq!(value(&variables, "NAUTILUS_SCRIPT_SELECTED_FILE_PATHS"), Some("/tmp/a\n/tmp/b\n"));
        assert_eq!(value(&variables, "NEMO_SCRIPT_SELECTED_URIS"), Some("file:///tmp/a\nfile:///tmp/b\n"));
    }

    #[test]
    fn a_file_managers_variables_are_only_there_with_files_copied() {
        let copied = Copied { clipboard: "text".into(), ..Default::default() };
        let variables = variables(&CommandContext::default(), &copied);
        assert_eq!(value(&variables, "GESTOS_CLIPBOARD"), Some("text"));
        assert_eq!(value(&variables, "GESTOS_FILES"), Some(""));
        assert_eq!(value(&variables, "GESTOS_APP"), Some(""));
        assert_eq!(value(&variables, "NAUTILUS_SCRIPT_SELECTED_URIS"), None);
    }

    #[test]
    fn typed_text_has_its_variables_filled_in() {
        let variables = [("result", "42".to_string()), ("app", "org.gnome.Nautilus".to_string())];
        let typed = TypedText::new("Found {{result}}px in {{ app }}, not {{apps}}, {x} or {{result", true);
        assert_eq!(typed.filled(&variables), "Found 42px in org.gnome.Nautilus, not {{apps}}, {x} or {{result");
        let plain = TypedText::new("{{result}}", false);
        assert_eq!(plain.filled(&variables), "{{result}}");
        assert!(!TypedText::new("{{result}}", true).reads_copied());
        assert!(TypedText::new("{{ files }}", true).reads_copied());
        assert!(!TypedText::new("{{files}}", false).reads_copied());
    }

    #[test]
    fn typed_text_is_given_what_a_command_is_and_the_result() {
        let context = CommandContext { gesture: "Paste".into(), app: None };
        let copied = Copied { clipboard: "hi".into(), uri_list: "file:///tmp/a\nfile:///tmp/b".into(), ..Default::default() };
        let typed = TypedText::new("{{clipboard}}|{{files}}|{{result}}|{{gesture}}", true);
        assert_eq!(typed.filled(&typed_variables(&context, &copied, "done")), "hi|/tmp/a\n/tmp/b|done|Paste");
    }

    #[test]
    fn too_much_text_is_too_big_for_a_variable() {
        assert!(!too_big("GESTOS_CLIPBOARD", &"a".repeat(100_000)));
        assert!(too_big("GESTOS_CLIPBOARD", &"a".repeat(MAX_VARIABLE)));
    }
}
