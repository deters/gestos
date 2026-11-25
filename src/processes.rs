//! The processes running now, read from /proc, as `pgrep` and `pkill` would find them, without
//! running either.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// A process running now.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: i32,
    /// The process that started it, or 0 when it's unknown.
    pub ppid: i32,
    /// The name it runs as, cut to 15 characters, as the kernel keeps it.
    pub name: String,
    /// Its command line, its program first.
    pub args: Vec<String>,
    /// The user it runs as.
    pub uid: u32,
}

impl Process {
    /// Whether it runs as this user.
    pub fn is_own(&self) -> bool {
        self.uid == nix::unistd::Uid::effective().as_raw()
    }

    /// The program it runs, by file name.
    pub fn program(&self) -> Option<&str> {
        Path::new(self.args.first()?).file_name()?.to_str()
    }
}

/// Every process that can be read. One that ends while it's read is left out.
pub fn running() -> Vec<Process> {
    let Ok(entries) = std::fs::read_dir("/proc") else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse().ok()?;
            let dir = entry.path();
            let uid = std::fs::metadata(&dir).ok()?.uid();
            let name = std::fs::read_to_string(dir.join("comm")).ok()?.trim_end().to_string();
            let ppid = std::fs::read_to_string(dir.join("stat")).ok().as_deref().and_then(parent_in_stat).unwrap_or(0);
            let cmdline = std::fs::read(dir.join("cmdline")).unwrap_or_default();
            let args = cmdline
                .split(|&b| b == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect();
            Some(Process { pid, ppid, name, args, uid })
        })
        .collect()
}

/// The parent's ID in a process's `/proc/<pid>/stat`. Its fields follow the name, which is in
/// parentheses and may itself hold spaces and parentheses, so they're counted from the last `)`.
fn parent_in_stat(stat: &str) -> Option<i32> {
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

/// Whether a process runs as this name, for anyone.
pub fn is_running(name: &str) -> bool {
    running().iter().any(|process| process.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_found_as_this_users() {
        let me = std::process::id() as i32;
        let found = running().into_iter().find(|process| process.pid == me).unwrap();
        assert!(found.is_own());
        let exe = std::env::current_exe().unwrap();
        assert!(exe.file_name().unwrap().to_str().unwrap().starts_with(&found.name));
        assert!(is_running(&found.name));
        assert!(!is_running("gestos-no-such-program"));
        assert_eq!(found.ppid as u32, std::os::unix::process::parent_id());
    }

    #[test]
    fn the_parent_is_read_after_the_name_whatever_it_holds() {
        assert_eq!(parent_in_stat("9693 (gestosd) S 1 9693 9693 0 -1"), Some(1));
        assert_eq!(parent_in_stat("42 (a) b (c) R 7 42 42 0 -1"), Some(7));
        assert_eq!(parent_in_stat("42 (gestosd"), None);
    }

    // Checked against pgrep itself, the one place it's run.
    #[test]
    fn the_same_processes_are_found_as_pgrep_finds() {
        let Ok(output) = std::process::Command::new("pgrep").args(["-u", &nix::unistd::Uid::effective().to_string(), "."]).output() else {
            eprintln!("skipped: no pgrep here");
            return;
        };
        let theirs: std::collections::BTreeSet<i32> =
            String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| line.parse().ok()).collect();
        let ours: std::collections::BTreeSet<i32> =
            running().into_iter().filter(Process::is_own).map(|process| process.pid).collect();
        // Processes start and end between the two looks, so most must be the same.
        let shared = theirs.intersection(&ours).count();
        assert!(shared * 10 >= theirs.len() * 9, "{shared} of {} found", theirs.len());
    }
}
