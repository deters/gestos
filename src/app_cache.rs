//! Installed apps are read from hundreds of desktop files, which takes a while, and the daemon would
//! do it for every gesture. What's read is kept, and read again only once an app is installed or
//! removed, which changes the folders the desktop files are in.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// When each folder with desktop files last changed, and its folders inside, as some apps keep
/// theirs in one. A folder that isn't there has no time.
pub type Stamp = Vec<(PathBuf, Option<SystemTime>)>;

/// The folders desktop files are in, as the XDG base directories say.
pub fn application_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".local/share")));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    data_home
        .into_iter()
        .chain(data_dirs.split(':').filter(|dir| !dir.is_empty()).map(PathBuf::from))
        .map(|dir| dir.join("applications"))
        .collect()
}

pub fn stamp_of(dirs: &[PathBuf]) -> Stamp {
    let changed = |dir: &Path| std::fs::metadata(dir).and_then(|meta| meta.modified()).ok();
    let mut stamp = Vec::new();
    for dir in dirs {
        stamp.push((dir.clone(), changed(dir)));
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut inside: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect();
        inside.sort();
        stamp.extend(inside.into_iter().map(|dir| {
            let time = changed(&dir);
            (dir, time)
        }));
    }
    stamp
}

/// A list read from the installed apps, kept until they change.
pub struct AppCache<T> {
    kept: Mutex<Option<(Stamp, T)>>,
}

impl<T: Clone> AppCache<T> {
    pub const fn new() -> Self {
        AppCache { kept: Mutex::new(None) }
    }

    /// What `read` gives, read again only if the apps changed since it last was.
    pub fn get(&self, read: impl FnOnce() -> T) -> T {
        self.get_in(&application_dirs(), read)
    }

    pub fn get_in(&self, dirs: &[PathBuf], read: impl FnOnce() -> T) -> T {
        let stamp = stamp_of(dirs);
        let mut kept = self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((kept_stamp, value)) = kept.as_ref() {
            if *kept_stamp == stamp {
                return value.clone();
            }
        }
        let value = read();
        *kept = Some((stamp, value.clone()));
        value
    }
}

impl<T: Clone> Default for AppCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("gestos-app-cache-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Folder times are kept to the second on some file systems, so a change is made to show.
    fn touch_later(dir: &Path) {
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::open(dir).unwrap().set_modified(later).unwrap();
    }

    #[test]
    fn apps_are_read_once_until_one_is_installed() {
        let temp = TempDir::new("installed");
        let dirs = vec![temp.0.clone()];
        let cache = AppCache::new();
        let reads = Cell::new(0);
        let read = || {
            reads.set(reads.get() + 1);
            reads.get()
        };
        assert_eq!(cache.get_in(&dirs, read), 1);
        assert_eq!(cache.get_in(&dirs, read), 1);
        std::fs::write(temp.0.join("new.desktop"), "").unwrap();
        touch_later(&temp.0);
        assert_eq!(cache.get_in(&dirs, read), 2);
        assert_eq!(cache.get_in(&dirs, read), 2);
    }

    #[test]
    fn a_folder_of_apps_inside_counts_too() {
        let temp = TempDir::new("inside");
        let inside = temp.0.join("kde4");
        std::fs::create_dir(&inside).unwrap();
        let before = stamp_of(&[temp.0.clone()]);
        touch_later(&inside);
        assert_ne!(stamp_of(&[temp.0.clone()]), before);
    }

    #[test]
    fn a_folder_that_appears_counts_as_a_change() {
        let temp = TempDir::new("appears");
        let missing = temp.0.join("applications");
        let before = stamp_of(&[missing.clone()]);
        assert_eq!(before, vec![(missing.clone(), None)]);
        std::fs::create_dir(&missing).unwrap();
        assert_ne!(stamp_of(&[missing]), before);
    }

    #[test]
    fn the_folders_follow_the_xdg_variables() {
        // Read as the variables are now, without changing them, as tests run side by side.
        let dirs = application_dirs();
        assert!(dirs.iter().all(|dir| dir.ends_with("applications")));
        assert!(!dirs.is_empty());
    }
}
