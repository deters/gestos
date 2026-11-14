use std::ffi::CString;
use std::fs::File;
use std::io::{Error, ErrorKind};
use std::os::fd::{AsRawFd, FromRawFd};
use std::time::{Duration, Instant};

const REPLACE_TIMEOUT: Duration = Duration::from_secs(2);
const REPLACE_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Per-device single-instance lock, held until the process exits.
///
/// This is a POSIX record lock on a shared memory object. The kernel releases it when the
/// holder dies and reports the holder's pid on request, so no pid is stored that could go
/// stale and end up signalling an unrelated process.
///
/// The lock object is never unlinked: an exiting instance could otherwise remove the name
/// that its replacement has already locked.
pub struct DaemonIpc {
    _lock_file: File,
}

fn shm_name(device_name: &str) -> String {
    // Different paths to one device (such as a by-path symlink) must share a lock.
    let canonical = std::fs::canonicalize(device_name)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| device_name.to_string());
    format!(
        "/gestos_uid_{}_dev_{}",
        unsafe { libc::getuid() },
        canonical.replace('/', "%")
    )
}

fn write_lock_request() -> libc::flock {
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as libc::c_short;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    lock
}

fn try_lock(file: &File) -> Result<bool, Error> {
    let lock = write_lock_request();
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &lock) } == 0 {
        return Ok(true);
    }
    let err = Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EACCES) | Some(libc::EAGAIN) => Ok(false),
        _ => Err(err),
    }
}

fn lock_holder(file: &File) -> Result<Option<libc::pid_t>, Error> {
    let mut lock = write_lock_request();
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLK, &mut lock) } != 0 {
        return Err(Error::last_os_error());
    }
    if lock.l_type == libc::F_UNLCK as libc::c_short {
        Ok(None)
    } else {
        Ok(Some(lock.l_pid))
    }
}

impl DaemonIpc {
    /// Takes the lock for `device_name`, asking any running instance to exit first.
    pub fn acquire(device_name: &str) -> Result<Self, Error> {
        let c_name = CString::new(shm_name(device_name)).map_err(|_| {
            Error::new(ErrorKind::InvalidInput, "device name contains a NUL byte")
        })?;

        let fd = unsafe { libc::shm_open(c_name.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600) };
        if fd < 0 {
            return Err(Error::last_os_error());
        }
        let file = unsafe { File::from_raw_fd(fd) };

        if !try_lock(&file)? {
            if let Some(pid) = lock_holder(&file)? {
                println!("Asking gestosd running on pid {} to exit..", pid);
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }

            let deadline = Instant::now() + REPLACE_TIMEOUT;
            while !try_lock(&file)? {
                if Instant::now() >= deadline {
                    return Err(Error::new(
                        ErrorKind::TimedOut,
                        "the running gestosd instance did not exit",
                    ));
                }
                std::thread::sleep(REPLACE_POLL_INTERVAL);
            }
        }

        Ok(DaemonIpc { _lock_file: file })
    }
}

/// The lock Gestos's gesture editor holds while it's open, for this user.
fn editor_lock_name() -> String {
    format!("/gestos_uid_{}_editor", unsafe { libc::getuid() })
}

/// Held while the gesture editor is open. A gesture drawn then is the one being recorded, so it
/// isn't run. The kernel lets go of it when the app ends, however it ends.
pub struct EditorLock {
    _lock_file: File,
}

impl EditorLock {
    pub fn acquire() -> Result<Self, Error> {
        let c_name = CString::new(editor_lock_name()).expect("no NUL byte in the lock name");
        let fd = unsafe { libc::shm_open(c_name.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600) };
        if fd < 0 {
            return Err(Error::last_os_error());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        if !try_lock(&file)? {
            return Err(Error::new(ErrorKind::WouldBlock, "another editor is open"));
        }
        Ok(EditorLock { _lock_file: file })
    }
}

/// Whether the gesture editor is open in another process, to record the gesture drawn next.
pub fn editor_is_open() -> bool {
    let Ok(c_name) = CString::new(editor_lock_name()) else {
        return false;
    };
    let fd = unsafe { libc::shm_open(c_name.as_ptr(), libc::O_RDWR, 0o600) };
    if fd < 0 {
        return false;
    }
    let file = unsafe { File::from_raw_fd(fd) };
    matches!(lock_holder(&file), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};

    const HOLDER_DEVICE_ENV: &str = "GESTOS_TEST_LOCK_DEVICE";

    // Runs as the competing instance when re-executed by the test below.
    #[test]
    fn lock_holder_process() {
        let Ok(device) = std::env::var(HOLDER_DEVICE_ENV) else {
            return;
        };
        let _ipc = DaemonIpc::acquire(&device).unwrap();
        let mut stdout = std::io::stdout();
        writeln!(stdout, "locked").unwrap();
        stdout.flush().unwrap();
        std::thread::sleep(Duration::from_secs(30));
    }

    const EDITOR_HOLDER_ENV: &str = "GESTOS_TEST_EDITOR_LOCK";

    // Runs as an open editor when re-executed by the test below.
    #[test]
    fn editor_lock_holder_process() {
        if std::env::var(EDITOR_HOLDER_ENV).is_err() {
            return;
        }
        let _lock = EditorLock::acquire().unwrap();
        println!("locked");
        std::io::stdout().flush().unwrap();
        std::thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn an_open_editor_is_seen_until_its_process_ends() {
        let mut editor = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "ipc::tests::editor_lock_holder_process", "--nocapture"])
            .env(EDITOR_HOLDER_ENV, "1")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = editor.stdout.take().unwrap();
        assert!(BufReader::new(stdout).lines().map_while(Result::ok).any(|l| l.contains("locked")));
        assert!(editor_is_open());
        // Only one editor records at a time.
        assert!(EditorLock::acquire().is_err());
        editor.kill().unwrap();
        editor.wait().unwrap();
        assert!(!editor_is_open());
    }

    fn spawn_holder(device: &str) -> std::process::Child {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "ipc::tests::lock_holder_process", "--nocapture"])
            .env(HOLDER_DEVICE_ENV, device)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let locked = BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .any(|line| line.contains("locked"));
        assert!(locked, "holder process never took the lock");
        child
    }

    #[test]
    fn acquire_replaces_a_live_holder_and_reclaims_a_dead_ones_lock() {
        let device = format!("/nonexistent/gestosd-test-{}", std::process::id());

        let mut live = spawn_holder(&device);
        let ipc = DaemonIpc::acquire(&device).unwrap();
        assert_eq!(live.wait().unwrap().signal(), Some(libc::SIGTERM));
        drop(ipc);

        // The kernel frees a dead holder's lock, so there is no holder left to signal.
        let mut dead = spawn_holder(&device);
        dead.kill().unwrap();
        dead.wait().unwrap();
        let ipc = DaemonIpc::acquire(&device).unwrap();
        drop(ipc);

        let c_name = CString::new(shm_name(&device)).unwrap();
        unsafe {
            libc::shm_unlink(c_name.as_ptr());
        }
    }
}
