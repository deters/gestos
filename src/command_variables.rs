//! What a command run by a gesture is given: what's copied and selected when it runs, the files
//! copied in a file manager, the app in use and the gesture's name. Each is an environment
//! variable, never put into the command's text, so that nothing copied can change what it runs.
//! Copied files are also its arguments, `"$@"`, and the variables a file manager gives its own
//! scripts, so that scripts written for one work too. Only a command that uses what's copied
//! gets it, so that the apps others open don't keep a copy of a password for as long as they run.

use std::path::PathBuf;

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
    /// The clipboard as a list of URIs, as file managers copy files, empty if it isn't one.
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

/// What Nautilus, Nemo and Caja call the files selected, when they run a script: their paths,
/// and their URIs.
const FILE_MANAGER_VARIABLES: [(&str, &str); 3] = [
    ("NAUTILUS_SCRIPT_SELECTED_FILE_PATHS", "NAUTILUS_SCRIPT_SELECTED_URIS"),
    ("NEMO_SCRIPT_SELECTED_FILE_PATHS", "NEMO_SCRIPT_SELECTED_URIS"),
    ("CAJA_SCRIPT_SELECTED_FILE_PATHS", "CAJA_SCRIPT_SELECTED_URIS"),
];

/// The most an environment variable can hold, with its name, `=` and the end, on Linux.
const MAX_VARIABLE: usize = 128 * 1024;

/// The URIs in a list of them, as text/uri-list has them: one per line, with comments.
pub fn uris(uri_list: &str) -> Vec<String> {
    uri_list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// The local paths of copied files. Those with none, like a folder on a server that isn't
/// mounted, only have their URIs.
pub fn file_paths(uris: &[String]) -> Vec<PathBuf> {
    use gio::prelude::FileExt;
    uris.iter().filter_map(|uri| gio::File::for_uri(uri).path()).collect()
}

/// The variables a command is given, by name, with what's copied if it uses it. Those for a file
/// manager's scripts are only there when files are copied, as scripts look for them to tell
/// whether they were run from one.
pub fn variables(context: &CommandContext, copied: Option<&Copied>) -> Vec<(&'static str, String)> {
    let mut variables = vec![
        ("GESTOS_APP", context.app.clone().unwrap_or_default()),
        ("GESTOS_GESTURE", context.gesture.clone()),
    ];
    let Some(copied) = copied else { return variables };
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

/// Whether a command uses what's copied, as far as its own text tells. A script it runs may too.
pub fn uses_copied(command: &str) -> bool {
    let named = ["GESTOS_CLIPBOARD", "GESTOS_SELECTION", "GESTOS_FILES", "SCRIPT_SELECTED_", "$@", "$*", "$#"];
    // `$1` to `$9`, and the same in braces, but not `${HOME}`.
    let arguments = |after: &[u8]| after.first().is_some_and(|c| b"123456789@*#".contains(c));
    let bytes = command.as_bytes();
    let numbered = (0..bytes.len()).any(|i| match &bytes[i..] {
        [b'$', b'{', rest @ ..] => arguments(rest),
        [b'$', rest @ ..] => rest.first().is_some_and(|c| b"123456789".contains(c)),
        _ => false,
    });
    named.iter().any(|name| command.contains(name)) || numbered
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
    fn copied_files_are_read_from_a_list_of_uris() {
        let list = "# copied\r\nfile:///home/me/a%20b.txt\r\n\r\nsftp://server/x\r\n";
        let uris = uris(list);
        assert_eq!(uris, ["file:///home/me/a%20b.txt", "sftp://server/x"]);
        assert_eq!(file_paths(&uris), [PathBuf::from("/home/me/a b.txt")]);
    }

    #[test]
    fn a_command_is_told_what_is_copied_and_where() {
        let context = CommandContext { gesture: "Upload".into(), app: Some("org.gnome.Nautilus".into()) };
        let copied = Copied {
            clipboard: "file:///tmp/a\nfile:///tmp/b".into(),
            selection: "picked\0text".into(),
            uri_list: "file:///tmp/a\nfile:///tmp/b\n".into(),
        };
        let variables = variables(&context, Some(&copied));
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
        let variables = variables(&CommandContext::default(), Some(&copied));
        assert_eq!(value(&variables, "GESTOS_CLIPBOARD"), Some("text"));
        assert_eq!(value(&variables, "GESTOS_FILES"), Some(""));
        assert_eq!(value(&variables, "GESTOS_APP"), Some(""));
        assert_eq!(value(&variables, "NAUTILUS_SCRIPT_SELECTED_URIS"), None);
    }

    #[test]
    fn a_command_that_doesnt_use_whats_copied_isnt_given_it() {
        let context = CommandContext { gesture: "Browser".into(), app: None };
        let names: Vec<&str> = variables(&context, None).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["GESTOS_APP", "GESTOS_GESTURE"]);
    }

    #[test]
    fn a_command_that_uses_whats_copied_says_so() {
        assert!(uses_copied(r#"notify-send "$GESTOS_CLIPBOARD""#));
        assert!(uses_copied(r#"~/bin/upload.sh "$@""#));
        assert!(uses_copied("convert $1 out.png"));
        assert!(uses_copied("echo ${1}"));
        assert!(uses_copied(r#"for f in "${@}"; do echo "$f"; done"#));
        assert!(!uses_copied(r#"notify-send "$GESTOS_GESTURE" "$HOME""#));
        assert!(!uses_copied("cd ${HOME} && ls ${PWD}"));
        assert!(!uses_copied("firefox"));
    }

    #[test]
    fn too_much_text_is_too_big_for_a_variable() {
        assert!(!too_big("GESTOS_CLIPBOARD", &"a".repeat(100_000)));
        assert!(too_big("GESTOS_CLIPBOARD", &"a".repeat(MAX_VARIABLE)));
    }
}
