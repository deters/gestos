//! What a command run by a gesture is given: what's copied and selected when it runs, the files
//! copied in a file manager, the app in use and the gesture's name. Each is an environment
//! variable, never put into the command's text, so that nothing copied can change what it runs.
//! Copied files are also its arguments, `"$@"`, and the variables a file manager gives its own
//! scripts, so that scripts written for one work too.

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

/// The variables a command is given, by name. Those for a file manager's scripts are only there
/// when files are copied, as scripts look for them to tell whether they were run from one.
pub fn variables(context: &CommandContext, copied: &Copied) -> Vec<(&'static str, String)> {
    let uris = uris(&copied.uri_list);
    let paths: Vec<String> =
        file_paths(&uris).iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let mut variables = vec![
        ("GESTOS_CLIPBOARD", copied.clipboard.clone()),
        ("GESTOS_SELECTION", copied.selection.clone()),
        ("GESTOS_FILES", paths.join("\n")),
        ("GESTOS_APP", context.app.clone().unwrap_or_default()),
        ("GESTOS_GESTURE", context.gesture.clone()),
    ];
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
    fn too_much_text_is_too_big_for_a_variable() {
        assert!(!too_big("GESTOS_CLIPBOARD", &"a".repeat(100_000)));
        assert!(too_big("GESTOS_CLIPBOARD", &"a".repeat(MAX_VARIABLE)));
    }
}
