//! Ready-made commands for "Run Command in Shell" and an advanced action's command steps, offered
//! in the window commands are written in. Each works on what the variables give it: the copied
//! files, as `"$@"`, or the selected text. None overwrites a file: one that's there already is
//! skipped, or the new one is named with the time.

/// A ready-made command, what it works on, and the program it needs with the package that has it.
pub struct LibraryCommand {
    pub group: &'static str,
    pub name: &'static str,
    pub command: &'static str,
    pub program: &'static str,
    pub package: &'static str,
}

const FILES: &str = "Copied Files";
const TEXT: &str = "Selected Text";

// Text in a web address, encoded as one.
macro_rules! url_with_selection {
    ($address:literal) => {
        concat!(
            "xdg-open \"",
            $address,
            "$(python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1]))' \"$GESTOS_SELECTION\")\""
        )
    };
}

pub const LIBRARY: &[LibraryCommand] = &[
    LibraryCommand {
        group: FILES,
        name: "Convert to PDF",
        command: r#"for f in "$@"; do libreoffice --headless --convert-to pdf --outdir "$(dirname "$f")" "$f"; done"#,
        program: "libreoffice",
        package: "LibreOffice",
    },
    LibraryCommand {
        group: FILES,
        name: "Merge PDFs",
        command: r#"pdfunite "$@" "$(dirname "$1")/Merged $(date '+%F %H-%M-%S').pdf""#,
        program: "pdfunite",
        package: "poppler-utils",
    },
    LibraryCommand {
        group: FILES,
        name: "Convert Images to PNG",
        command: r#"for f in "$@"; do [ -e "${f%.*}.png" ] || convert "$f" "${f%.*}.png"; done"#,
        program: "convert",
        package: "ImageMagick",
    },
    LibraryCommand {
        group: FILES,
        name: "Convert Images to JPG",
        command: r#"for f in "$@"; do [ -e "${f%.*}.jpg" ] || convert "$f" "${f%.*}.jpg"; done"#,
        program: "convert",
        package: "ImageMagick",
    },
    LibraryCommand {
        group: FILES,
        name: "Make Images Half the Size",
        command: r#"for f in "$@"; do [ -e "${f%.*} (small).${f##*.}" ] || convert "$f" -resize 50% "${f%.*} (small).${f##*.}"; done"#,
        program: "convert",
        package: "ImageMagick",
    },
    LibraryCommand {
        group: FILES,
        name: "Compress to ZIP",
        command: r#"cd "$(dirname "$1")" && n=$# && for f; do set -- "$@" "${f##*/}"; done && shift $n && zip -r "Archive $(date '+%F %H-%M-%S').zip" "$@""#,
        program: "zip",
        package: "zip",
    },
    LibraryCommand {
        group: FILES,
        name: "Extract ZIP Files",
        command: r#"for f in "$@"; do unzip -n "$f" -d "${f%.*}"; done"#,
        program: "unzip",
        package: "unzip",
    },
    LibraryCommand {
        group: FILES,
        name: "Show Checksums",
        command: r#"notify-send "SHA-256" "$(for f in "$@"; do printf '%s  %s\n' "$(sha256sum < "$f" | cut -d' ' -f1)" "${f##*/}"; done)""#,
        program: "notify-send",
        package: "libnotify",
    },
    LibraryCommand {
        group: TEXT,
        name: "Search the Web",
        command: url_with_selection!("https://duckduckgo.com/?q="),
        program: "python3",
        package: "Python",
    },
    LibraryCommand {
        group: TEXT,
        name: "Translate",
        command: url_with_selection!("https://translate.google.com/?op=translate&sl=auto&text="),
        program: "python3",
        package: "Python",
    },
    LibraryCommand {
        group: TEXT,
        name: "Open Link",
        command: r#"xdg-open "$GESTOS_SELECTION""#,
        program: "xdg-open",
        package: "xdg-utils",
    },
    LibraryCommand {
        group: TEXT,
        name: "Read Aloud",
        command: r#"spd-say "$GESTOS_SELECTION""#,
        program: "spd-say",
        package: "speech-dispatcher",
    },
    LibraryCommand {
        group: TEXT,
        name: "Calculate",
        command: r#"notify-send "$GESTOS_SELECTION" "= $(printf '%s\n' "$GESTOS_SELECTION" | bc -l)""#,
        program: "bc",
        package: "bc",
    },
    LibraryCommand {
        group: TEXT,
        name: "Count Words",
        command: r#"notify-send "$(printf %s "$GESTOS_SELECTION" | wc -w) words""#,
        program: "notify-send",
        package: "libnotify",
    },
];

/// The name of a ready-made command, if a command is one, so a gesture can be named after it.
pub fn name_of(command: &str) -> Option<&'static str> {
    LIBRARY.iter().find(|entry| entry.command == command.trim()).map(|entry| entry.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Runs a command as Gestos does, with the files as its arguments, and the selection.
    fn run(command: &str, files: &[&std::path::Path], selection: &str) -> std::process::Output {
        Command::new("sh")
            .arg("-c")
            .arg(command)
            .arg("gestos")
            .args(files)
            .env("GESTOS_SELECTION", selection)
            .output()
            .unwrap()
    }

    #[test]
    fn every_command_uses_its_program_and_what_it_works_on() {
        for entry in LIBRARY {
            assert!(entry.command.contains(entry.program), "{}", entry.name);
            let works_on = if entry.group == FILES { "\"$@\"" } else { "\"$GESTOS_SELECTION\"" };
            assert!(entry.command.contains(works_on), "{}", entry.name);
        }
    }

    #[test]
    fn every_command_is_valid_shell() {
        for entry in LIBRARY {
            let status = Command::new("sh").arg("-n").arg("-c").arg(entry.command).status().unwrap();
            assert!(status.success(), "{}", entry.name);
        }
    }

    #[test]
    fn a_zip_keeps_the_files_names_however_odd() {
        if Command::new("zip").arg("-v").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("gestos-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let odd = dir.join("a 'b' $c.txt");
        let plain = dir.join("d.txt");
        std::fs::write(&odd, "1").unwrap();
        std::fs::write(&plain, "2").unwrap();
        let zip = LIBRARY.iter().find(|entry| entry.name == "Compress to ZIP").unwrap();
        assert!(run(zip.command, &[&odd, &plain], "").status.success());
        let archive = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "zip"))
            .unwrap();
        let listing = Command::new("unzip").arg("-Z1").arg(&archive).output().unwrap();
        let names = String::from_utf8_lossy(&listing.stdout);
        assert_eq!(names.lines().collect::<Vec<_>>(), ["a 'b' $c.txt", "d.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_selection_is_encoded_in_the_address() {
        if Command::new("python3").arg("-V").output().is_err() {
            return;
        }
        let search = LIBRARY.iter().find(|entry| entry.name == "Search the Web").unwrap();
        // The address it would open, rather than opening it.
        let shown = search.command.replacen("xdg-open", "printf %s", 1);
        let output = run(&shown, &[], "a & b?");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "https://duckduckgo.com/?q=a%20%26%20b%3F");
    }
}
