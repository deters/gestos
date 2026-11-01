use gestos::chooser::{encode_entries, parse_choice, ChooserEntry};
use gestos::config::{ActionType, Configuration, Gesture};
use gestos::modifiers::Modifiers;
use gestos::ipc::DaemonIpc;
use gestos::wayland::WaylandContext;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use zbus::interface;

static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);
static IS_MANAGER: AtomicBool = AtomicBool::new(false);
static RELOAD_MANAGER: AtomicBool = AtomicBool::new(false);
static CHILD_PIDS: once_cell::sync::Lazy<std::sync::Mutex<Vec<u32>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(Vec::new()));

static CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();
// Written to by the D-Bus thread to wake the event loop, which may be waiting for input.
static RELOAD_WAKE: OnceLock<UnixStream> = OnceLock::new();

// Exit status for errors that restarting can't fix, such as a device without the trigger button.
const EXIT_PERMANENT_ERROR: i32 = 2;

const DBUS_PATH: &str = "/io/github/deters/Gestos/Daemon";
const LEGACY_DBUS_NAME: &str = "org.mygestures.Daemon";
const LEGACY_DBUS_PATH: &str = "/org/mygestures/Daemon";
// Keeps GNOME from treating a released Super as a tap, which opens the Overview. It's F24, which
// nothing normally uses.
#[cfg(target_os = "linux")]
const UNUSED_KEY: u16 = 194;
#[cfg(target_os = "linux")]
const MODIFIER_RELEASE_TIMEOUT: Duration = Duration::from_millis(1500);
#[cfg(target_os = "linux")]
const MODIFIER_RELEASE_POLL: Duration = Duration::from_millis(20);
// Gives udev time to set a new device's permissions.
#[cfg(target_os = "linux")]
const KEYBOARD_SETTLE_DELAY: Duration = Duration::from_millis(500);

const CHOOSER_EXIT_CANCELLED: i32 = 1;
const CHOOSER_FOCUS_RETURN_DELAY_MILLIS: u64 = 400;
const LEGACY_STOP_TIMEOUT: Duration = Duration::from_secs(2);
const GRAB_ATTEMPTS: u32 = 5;
const GRAB_RETRY_INTERVAL: Duration = Duration::from_millis(100);

const DBUS_NAME_ATTEMPTS: u32 = 15;
const DBUS_NAME_RETRY_INTERVAL: Duration = Duration::from_millis(200);

const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(200);
const CHILD_RESTART_MIN_DELAY: Duration = Duration::from_secs(1);
const CHILD_RESTART_MAX_DELAY: Duration = Duration::from_secs(30);
const CHILD_STABLE_RUNTIME: Duration = Duration::from_secs(30);

fn config_is_valid() -> bool {
    CONFIG_PATH
        .get()
        .is_some_and(|path| Configuration::load_from_file(path).is_some())
}

fn kill_children() {
    let pids = CHILD_PIDS.lock().unwrap();
    for &pid in pids.iter() {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGTERM,
        );
    }
}

struct DaemonDbus;

#[interface(name = "io.github.deters.Gestos.Daemon")]
impl DaemonDbus {
    fn reload(&self) -> Result<(), zbus::fdo::Error> {
        if !config_is_valid() {
            return Err(zbus::fdo::Error::Failed(
                "Invalid configuration file format".to_string(),
            ));
        }
        if IS_MANAGER.load(Ordering::Relaxed) {
            RELOAD_MANAGER.store(true, Ordering::Relaxed);
            kill_children();
        } else {
            RELOAD_REQUESTED.store(true, Ordering::Relaxed);
            if let Some(mut wake) = RELOAD_WAKE.get() {
                let _ = wake.write(&[1]);
            }
        }
        Ok(())
    }

    #[zbus(property)]
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn stop(&self) {
        if IS_MANAGER.load(Ordering::Relaxed) {
            kill_children();
        }
        std::process::exit(0);
    }

    #[zbus(signal)]
    async fn gesture_started(&self, ctxt: &zbus::object_server::SignalContext<'_>, x: f64, y: f64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn gesture_updated(&self, ctxt: &zbus::object_server::SignalContext<'_>, x: f64, y: f64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn gesture_ended(&self, ctxt: &zbus::object_server::SignalContext<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_executed(&self, ctxt: &zbus::object_server::SignalContext<'_>, gesture_name: String, action_desc: String, icon_name: String) -> zbus::Result<()>;
}

fn find_mouse_device() -> Option<PathBuf> {
    let input_dir = Path::new("/dev/input/by-path");
    if !input_dir.exists() {
        return None;
    }
    let mut fallback = None;
    if let Ok(entries) = fs::read_dir(input_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains("event-mouse") {
                return Some(input_dir.join(name));
            }
            if name.contains("-mouse") && fallback.is_none() {
                fallback = Some(input_dir.join(name));
            }
        }
    }
    fallback
}

fn check_permissions(device: &str) -> bool {
    let mut uinput_ok = false;
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open("/dev/uinput") {
        uinput_ok = true;
        drop(file);
    } else if let Ok(file) = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/misc/uinput")
    {
        uinput_ok = true;
        drop(file);
    }

    let uinput_exists = std::path::Path::new("/dev/uinput").exists()
        || std::path::Path::new("/dev/misc/uinput").exists();
    let dev_ok = std::fs::OpenOptions::new().read(true).open(device).is_ok();

    if !uinput_ok || !dev_ok {
        eprintln!("\n=========================================================================");
        eprintln!("ERROR: Missing permissions or kernel module to run gestosd.");
        if !dev_ok {
            eprintln!(" - Cannot read mouse input device '{}'.", device);
        }
        if !uinput_ok {
            if !uinput_exists {
                eprintln!(" - /dev/uinput virtual device creator does not exist. The uinput kernel module is likely not loaded.");
            } else {
                eprintln!(" - Cannot write to /dev/uinput virtual device creator.");
            }
        }

        if !uinput_exists {
            eprintln!("\nTo resolve the missing kernel module:\n");
            eprintln!("1. Load the uinput kernel module:");
            eprintln!("   sudo modprobe uinput");
            eprintln!("   (To load it automatically on boot, add 'uinput' to /etc/modules)\n");
            eprintln!(
                "=========================================================================\n"
            );
            return false;
        }

        eprintln!("\nTo resolve permissions, choose one of the following setups:\n");
        eprintln!("OPTION A: SECURE SETUP (Recommended - Only gestosd has access)");
        eprintln!(
            "  1. Install the gestosd udev rules (restricts access to the 'input' group):"
        );
        eprintln!("     sudo cp 99-gestos.rules /etc/udev/rules.d/");
        eprintln!("     sudo udevadm control --reload-rules && sudo udevadm trigger");
        eprintln!("  2. Configure the gestosd binary to run as Set-Group-ID (SGID) to 'input':");
        eprintln!("     sudo chown root:input /usr/local/bin/gestosd");
        eprintln!("     sudo chmod 2755 /usr/local/bin/gestosd");
        eprintln!("     (Note: This allows only this binary to capture inputs/inject actions)");

        eprintln!("\nOPTION B: STANDARD SETUP (Allows all user applications to capture inputs)");
        eprintln!("  1. Add your user to the 'input' group:");
        eprintln!("     sudo usermod -aG input $USER");
        eprintln!("  2. Install the udev rules:");
        eprintln!("     sudo cp 99-gestos.rules /etc/udev/rules.d/");
        eprintln!("     sudo udevadm control --reload-rules && sudo udevadm trigger");
        eprintln!("=========================================================================\n");
        return false;
    }
    true
}

#[cfg(target_os = "linux")]
enum DaemonSignal {
    GestureStarted(f64, f64),
    GestureUpdated(f64, f64),
    GestureEnded,
    ActionExecuted(String, String, String),
}

#[cfg(target_os = "linux")]
const NOTIFICATIONS_NAME: &str = "org.freedesktop.Notifications";
#[cfg(target_os = "linux")]
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
#[cfg(target_os = "linux")]
const GESTURE_NOTIFICATION_DURATION: Duration = Duration::from_secs(1);

/// Shows which gesture ran, replacing the notification `replaces` if it's still up, and returns
/// its id. Transient, so that it stays out of the desktop's list of notifications.
#[cfg(target_os = "linux")]
fn show_gesture_notification(
    conn: &zbus::blocking::Connection,
    service: &str,
    replaces: u32,
    gesture_name: &str,
    action_desc: &str,
    icon_name: &str,
) -> zbus::Result<u32> {
    let mut hints = std::collections::HashMap::new();
    hints.insert("transient", zbus::zvariant::Value::from(true));
    let reply = conn.call_method(
        Some(service),
        NOTIFICATIONS_PATH,
        Some(NOTIFICATIONS_NAME),
        "Notify",
        &(
            "Gestos",
            replaces,
            icon_name,
            gesture_name,
            action_desc,
            Vec::<&str>::new(),
            hints,
            GESTURE_NOTIFICATION_DURATION.as_millis() as i32,
        ),
    )?;
    reply.body().deserialize()
}

/// Closes the notification after its time is up, unless a newer one has replaced it. Desktops
/// may ignore the time asked for, as GNOME can.
#[cfg(target_os = "linux")]
fn close_gesture_notification_later(
    conn: zbus::blocking::Connection,
    id: u32,
    latest: std::sync::Arc<std::sync::atomic::AtomicU64>,
    shown: u64,
) {
    std::thread::spawn(move || {
        std::thread::sleep(GESTURE_NOTIFICATION_DURATION);
        if latest.load(Ordering::SeqCst) == shown {
            let _ = conn.call_method(
                Some(NOTIFICATIONS_NAME),
                NOTIFICATIONS_PATH,
                Some(NOTIFICATIONS_NAME),
                "CloseNotification",
                &(id,),
            );
        }
    });
}

/// Emits D-Bus signals on their own thread, because a stalled bus must never hold up pointer
/// forwarding. If the queue fills up, the event loop drops signals instead of waiting.
#[cfg(target_os = "linux")]
fn spawn_signal_thread() -> std::sync::mpsc::SyncSender<DaemonSignal> {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<DaemonSignal>(1024);
    std::thread::spawn(move || {
        let Ok(conn) = zbus::blocking::Connection::session() else {
            return;
        };
        let path = "/io/github/deters/Gestos/Daemon";
        let iface = "io.github.deters.Gestos.Daemon";
        let dest = None::<zbus::names::BusName>;
        let latest_notification = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut notification_id = 0;
        for signal in receiver {
            if let DaemonSignal::ActionExecuted(name, desc, icon) = &signal {
                let shown = latest_notification.fetch_add(1, Ordering::SeqCst) + 1;
                match show_gesture_notification(
                    &conn,
                    NOTIFICATIONS_NAME,
                    notification_id,
                    name,
                    desc,
                    icon,
                ) {
                    Ok(id) => {
                        notification_id = id;
                        close_gesture_notification_later(
                            conn.clone(),
                            id,
                            std::sync::Arc::clone(&latest_notification),
                            shown,
                        );
                    }
                    Err(e) => eprintln!("gestosd: Could not show a notification: {}", e),
                }
            }
            let _ = match signal {
                DaemonSignal::GestureStarted(x, y) => {
                    conn.emit_signal(dest.clone(), path, iface, "GestureStarted", &(x, y))
                }
                DaemonSignal::GestureUpdated(x, y) => {
                    conn.emit_signal(dest.clone(), path, iface, "GestureUpdated", &(x, y))
                }
                DaemonSignal::GestureEnded => {
                    conn.emit_signal(dest.clone(), path, iface, "GestureEnded", &())
                }
                DaemonSignal::ActionExecuted(name, desc, icon) => {
                    conn.emit_signal(dest.clone(), path, iface, "ActionExecuted", &(name, desc, icon))
                }
            };
        }
    });
    sender
}

/// Runs a gesture's actions in order on their own thread, because they can sleep or wait on a
/// child process (`swaylock` runs until the screen is unlocked).
#[cfg(target_os = "linux")]
type SharedKeyboards = std::sync::Arc<std::sync::Mutex<gestos::keyboards::Keyboards>>;

#[cfg(target_os = "linux")]
fn held_modifiers(keyboards: &Option<SharedKeyboards>) -> Modifiers {
    keyboards
        .as_ref()
        .and_then(|keyboards| keyboards.lock().ok().map(|keyboards| keyboards.held()))
        .unwrap_or_default()
}

/// A shortcut sent while a modifier is still held would combine with it and mean something else.
#[cfg(target_os = "linux")]
fn wait_for_modifier_release(keyboards: &Option<SharedKeyboards>) {
    let deadline = Instant::now() + MODIFIER_RELEASE_TIMEOUT;
    while !held_modifiers(keyboards).is_empty() && Instant::now() < deadline {
        std::thread::sleep(MODIFIER_RELEASE_POLL);
    }
}

/// Runs this program again, which regains its rights just long enough to open keyboards. Rights
/// are dropped for good after startup, so this is the only way to open one that appears later.
#[cfg(target_os = "linux")]
fn restart_daemon() -> ! {
    std::thread::sleep(KEYBOARD_SETTLE_DELAY);
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let error = std::env::current_exe()
        .map(|exe| Command::new(exe).args(&args).exec())
        .unwrap_or_else(|e| e);
    eprintln!("gestosd: Could not restart: {}", error);
    std::process::exit(1);
}

/// Watches for new input devices, without needing any rights.
#[cfg(target_os = "linux")]
fn watch_input_devices() -> Option<fs::File> {
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let file = unsafe { <fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) };
    let watched = unsafe {
        libc::inotify_add_watch(fd, c"/dev/input".as_ptr(), libc::IN_CREATE)
    };
    (watched >= 0).then_some(file)
}

#[cfg(target_os = "linux")]
fn spawn_gesture_actions(
    actions: Vec<ActionType>,
    wayland_ctx: WaylandContext,
    uinput: Option<gestos::uinput::UinputDevice>,
    wait_for_release: Option<SharedKeyboards>,
) {
    std::thread::spawn(move || {
        wait_for_modifier_release(&wait_for_release);
        for action in &actions {
            if let ActionType::Click(btn) = action {
                if let Some(ref u) = uinput {
                    u.click(btn.unwrap_or(1));
                }
            } else {
                wayland_ctx.execute_action(action, &|keys_str| {
                    if let Some(ref u) = uinput {
                        u.keypress_string(keys_str);
                    }
                });
            }
        }
    });
}

/// Grabs the device, waiting briefly for an instance that is exiting to release it.
#[cfg(target_os = "linux")]
fn grab_device(dev: &mut evdev::Device) -> std::io::Result<()> {
    let mut attempt = 1;
    loop {
        match dev.grab() {
            Err(e) if e.raw_os_error() == Some(libc::EBUSY) && attempt < GRAB_ATTEMPTS => {
                attempt += 1;
                std::thread::sleep(GRAB_RETRY_INTERVAL);
            }
            result => return result,
        }
    }
}

#[cfg(target_os = "linux")]
fn gesture_templates(
    config: &Configuration,
    modifiers: Modifiers,
) -> Vec<(String, Vec<gestos::protractor::Point2D>)> {
    config
        .gestures
        .iter()
        .filter(|g| !g.is_deleted && g.modifiers == modifiers)
        .map(|g| (g.name.clone(), g.points.clone()))
        .collect()
}

#[cfg(target_os = "linux")]
fn dispatch_gesture(
    points: &[gestos::protractor::Point2D],
    config: &Configuration,
    modifiers: Modifiers,
    keyboards: &Option<SharedKeyboards>,
    signals: &std::sync::mpsc::SyncSender<DaemonSignal>,
    wayland_ctx: &WaylandContext,
    uinput: &Option<gestos::uinput::UinputDevice>,
) {
    let templates = gesture_templates(config, modifiers);
    let candidates: Vec<Gesture> = gestos::protractor::match_gesture_candidates(points, &templates)
        .iter()
        .filter_map(|name| config.gestures.iter().find(|g| &g.name == name).cloned())
        .collect();

    match candidates.len() {
        0 => println!("Gesture failed: did not match any known template."),
        1 => run_gesture(&candidates[0], keyboards, signals, wayland_ctx, uinput),
        _ => {
            let signals = signals.clone();
            let wayland_ctx = wayland_ctx.clone();
            let uinput = uinput.clone();
            let keyboards = keyboards.clone();
            std::thread::spawn(move || {
                if let Some(gesture) = choose_gesture(candidates) {
                    // Actions like closing a window act on the focused one, which only gets
                    // focus back once the chooser has gone.
                    let delay = chooser_focus_return_delay();
                    println!("Waiting {} ms for focus to return.", delay.as_millis());
                    std::thread::sleep(delay);
                    run_gesture(&gesture, &keyboards, &signals, &wayland_ctx, &uinput);
                }
            });
        }
    }
}

/// Experimental: GESTOS_CHOOSER_DELAY_MS overrides the wait, to find what a desktop needs.
fn chooser_focus_return_delay() -> Duration {
    let millis = std::env::var("GESTOS_CHOOSER_DELAY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(CHOOSER_FOCUS_RETURN_DELAY_MILLIS);
    Duration::from_millis(millis)
}

/// Asks the user which of several similar gestures they meant. Nothing means they cancelled. If
/// the chooser can't run, gestures keep working by taking the closest match.
#[cfg(target_os = "linux")]
fn choose_gesture(mut candidates: Vec<Gesture>) -> Option<Gesture> {
    let entries: Vec<ChooserEntry> = candidates
        .iter()
        .map(|gesture| ChooserEntry {
            name: gesture.name.clone(),
            description: gesture.action_description(),
            points: gesture.points.clone(),
        })
        .collect();

    let app = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("gestos")))
        .filter(|path| path.exists())
        .unwrap_or_else(|| "gestos".into());
    let chooser = Command::new(app)
        .arg("--choose-gesture")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let output = chooser.and_then(|mut child| {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(encode_entries(&entries).as_bytes())?;
        }
        child.wait_with_output()
    });

    match output {
        Ok(output) if output.status.code() == Some(CHOOSER_EXIT_CANCELLED) => None,
        Ok(output) if output.status.success() => {
            let choice = parse_choice(&String::from_utf8_lossy(&output.stdout), entries.len())?;
            Some(candidates.swap_remove(choice))
        }
        _ => {
            eprintln!("gestosd: The gesture chooser failed. Using the closest match.");
            Some(candidates.swap_remove(0))
        }
    }
}

#[cfg(target_os = "linux")]
fn run_gesture(
    gesture: &Gesture,
    keyboards: &Option<SharedKeyboards>,
    signals: &std::sync::mpsc::SyncSender<DaemonSignal>,
    wayland_ctx: &WaylandContext,
    uinput: &Option<gestos::uinput::UinputDevice>,
) {
    println!("Matched gesture: {}", gesture.name);

    let action_desc = gesture.action_description();
    let icon_name = gesture
        .actions
        .first()
        .map(|a| a.get_icon_name().to_string())
        .unwrap_or_else(|| "system-run-symbolic".to_string());
    let _ = signals.try_send(DaemonSignal::ActionExecuted(
        gesture.name.clone(),
        action_desc,
        icon_name,
    ));

    let wait_for_release = if gesture.modifiers.is_empty() {
        None
    } else {
        keyboards.clone()
    };
    spawn_gesture_actions(
        gesture.actions.clone(),
        wayland_ctx.clone(),
        uinput.clone(),
        wait_for_release,
    );
}

#[cfg(target_os = "linux")]
fn run_grabber(
    device_path: &str,
    trigger_button: i32,
    sensitivity: i32,
    mut config: gestos::config::Configuration,
    wayland_ctx: gestos::wayland::WaylandContext,
) -> Result<(), std::io::Error> {
    use evdev::{Device, EventType, KeyCode};
    use gestos::gesture::GestureRecognizer;
    use gestos::uinput::UinputDevice;

    let signals = spawn_signal_thread();

    println!("Attempting to open device: {}", device_path);
    let mut dev = Device::open(device_path)?;

    let target_button = gestos::uinput::trigger_button_code(trigger_button)
        .map(KeyCode)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("trigger {} is not a mouse button", trigger_button),
            )
        })?;
    if !dev
        .supported_keys()
        .is_some_and(|keys| keys.contains(target_button))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "device {} does not have trigger button {}",
                device_path, trigger_button
            ),
        ));
    }

    // Adjust sensitivity threshold if relative device vs absolute device
    let mut threshold = sensitivity as f64;
    let mut min_path_length = 15.0;
    let abs_x = dev
        .get_absinfo()
        .ok()
        .and_then(|mut iter| iter.find(|(code, _)| *code == evdev::AbsoluteAxisCode::ABS_X))
        .map(|(_, info)| info);
    if let Some(abs_info) = abs_x {
        // Absolute units vary by device, so scale from its resolution (units per mm) or range.
        let range = abs_info.maximum() - abs_info.minimum();
        let resolution = abs_info.resolution();
        if resolution > 0 {
            min_path_length = (resolution * 2) as f64;
        } else if range > 0 {
            min_path_length = (range / 50) as f64;
        }
        if sensitivity <= 30 {
            if resolution > 0 {
                threshold = (resolution * 4) as f64;
            } else if range > 0 {
                threshold = (range / 25) as f64;
            }
        }
    }

    // Create the virtual device first: a grabbed mouse with nowhere to forward its events
    // would leave the user without a pointer.
    let uinput = match UinputDevice::init_from_device(&dev) {
        Ok(u) => Some(u),
        Err(e) => {
            eprintln!("gestosd: Failed to initialize uinput: {}", e);
            None
        }
    };

    // Exclusive grab (only for button 3 by default)
    let mut grabbed = false;
    if trigger_button == 3 {
        if uinput.is_none() {
            eprintln!("gestosd: Not grabbing the device because uinput is unavailable.");
        } else if let Err(e) = grab_device(&mut dev) {
            eprintln!("gestosd: Failed to grab device exclusively: {}", e);
        } else {
            println!("gestosd: Grabbed device exclusively.");
            grabbed = true;
        }
    }

    // Opened now, while there are rights to, and only if a gesture needs them.
    let keyboards: Option<SharedKeyboards> = config.uses_modifiers().then(|| {
        let keyboards = gestos::keyboards::Keyboards::open();
        println!("Reading modifier state from {} keyboards.", keyboards.count());
        std::sync::Arc::new(std::sync::Mutex::new(keyboards))
    });

    // Drop the elevated GID for good, including the saved GID that setegid would leave behind
    let rgid = nix::unistd::getgid();
    let egid = nix::unistd::getegid();
    if egid != rgid {
        nix::unistd::setresgid(rgid, rgid, rgid)
            .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
        println!(
            "gestosd: Successfully dropped elevated GID from {} to {}",
            egid, rgid
        );
    }

    println!(
        "Listening for events from device using libevdev (button {})",
        trigger_button
    );

    let (mut wake_reader, wake_writer) = UnixStream::pair()?;
    wake_reader.set_nonblocking(true)?;
    wake_writer.set_nonblocking(true)?;
    let _ = RELOAD_WAKE.set(wake_writer);

    let mut recognizer = GestureRecognizer::new(threshold / 10.0, min_path_length);
    let mut pending_frame: Vec<evdev::InputEvent> = Vec::new();
    let mut held_at_press = Modifiers::default();
    let mut restart_requested = false;
    let mut input_watch = keyboards.as_ref().and_then(|_| watch_input_devices());

    // Event loop
    loop {
        if RELOAD_REQUESTED.swap(false, Ordering::Relaxed) {
            println!("gestosd: Reloading configuration dynamically...");
            if let Some(new_config) = Configuration::load_from_file(&config.user_config_path) {
                config = new_config;
                restart_requested |= keyboards.is_none() && config.uses_modifiers();
                println!("gestosd: Configuration reloaded successfully.");
            } else {
                eprintln!(
                    "gestosd: Failed to reload configuration from {}",
                    config.user_config_path.display()
                );
            }
        }

        if restart_requested && !recognizer.is_drawing() {
            println!("gestosd: Restarting to open keyboards.");
            restart_daemon();
        }

        let mut poll_fds = [
            libc::pollfd {
                fd: dev.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wake_reader.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            // A negative descriptor is ignored.
            libc::pollfd {
                fd: input_watch.as_ref().map_or(-1, |watch| watch.as_raw_fd()),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        if unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as libc::nfds_t, -1) } < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        if poll_fds[1].revents != 0 {
            let mut drained = [0u8; 64];
            let _ = wake_reader.read(&mut drained);
        }
        if poll_fds[2].revents != 0 {
            if let Some(watch) = input_watch.as_mut() {
                let mut buffer = [0u8; 4096];
                if let Ok(length) = watch.read(&mut buffer) {
                    restart_requested |= gestos::keyboards::created_event_nodes(&buffer[..length])
                        .iter()
                        .any(|node| gestos::keyboards::is_physical_keyboard(node));
                }
            }
        }
        if poll_fds[0].revents == 0 {
            continue;
        }

        let events = match dev.fetch_events() {
            Ok(events) => events,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        for ev in events {
            let ev_type = ev.event_type();
            let ev_code = ev.code();
            let ev_value = ev.value();

            if ev_type == EventType::KEY && ev_code == target_button.0 {
                if ev_value == 1 {
                    held_at_press = held_modifiers(&keyboards);
                    let start = recognizer.press();
                    println!("Gesture drawing started at ({:.1}, {:.1})", start.x, start.y);
                    let _ = signals.try_send(DaemonSignal::GestureStarted(start.x, start.y));
                } else if ev_value == 0 {
                    let path = recognizer.release();
                    let _ = signals.try_send(DaemonSignal::GestureEnded);

                    if let Some(points) = path {
                        println!("Gesture drawing finished. Path points: {}", points.len());
                        if held_at_press.super_key {
                            if let Some(ref u) = uinput {
                                u.tap_key(UNUSED_KEY);
                            }
                        }
                        dispatch_gesture(
                            &points,
                            &config,
                            held_at_press,
                            &keyboards,
                            &signals,
                            &wayland_ctx,
                            &uinput,
                        );
                    } else {
                        println!("Gesture failed: path too short or too few points.");
                        // Click emulation
                        if grabbed {
                            if let Some(ref u) = uinput {
                                let u = u.clone();
                                std::thread::spawn(move || u.click(trigger_button));
                            }
                        }
                    }
                }
                continue;
            }

            // Forward whole frames, so one movement isn't split across several reports
            if grabbed {
                if ev_type != EventType::SYNCHRONIZATION {
                    pending_frame.push(ev);
                } else if ev_code == 0 && !pending_frame.is_empty() {
                    if let Some(ref u) = uinput {
                        u.forward_frame(&pending_frame);
                    }
                    pending_frame.clear();
                }
            }

            if ev_type == EventType::RELATIVE {
                match ev_code {
                    0 => recognizer.move_by(ev_value as f64, 0.0),
                    1 => recognizer.move_by(0.0, ev_value as f64),
                    _ => {}
                }
            } else if ev_type == EventType::ABSOLUTE {
                match ev_code {
                    0 => recognizer.set_x(ev_value as f64),
                    1 => recognizer.set_y(ev_value as f64),
                    _ => {}
                }
            } else if ev_type == EventType::SYNCHRONIZATION && ev_code == 0 {
                if let Some(point) = recognizer.sync() {
                    let _ = signals.try_send(DaemonSignal::GestureUpdated(point.x, point.y));
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn run_grabber(
    _device_path: &str,
    _trigger_button: i32,
    _sensitivity: i32,
    _config: gestos::config::Configuration,
    _wayland_ctx: gestos::wayland::WaylandContext,
) -> Result<(), std::io::Error> {
    println!("evdev grabbing is only supported on Linux. Mock daemon sleeping...");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// A child stopped with SIGTERM was stopped on purpose, either by this manager or by a new
/// instance taking over its device, so restarting it would fight that decision.
fn should_restart(status: ExitStatus) -> bool {
    !status.success()
        && status.signal() != Some(libc::SIGTERM)
        && status.code() != Some(EXIT_PERMANENT_ERROR)
}

/// Returns how long to wait before a restart, and the delay to use after that one.
fn restart_backoff(delay: Duration, ran_for: Duration) -> (Duration, Duration) {
    let wait = if ran_for >= CHILD_STABLE_RUNTIME {
        CHILD_RESTART_MIN_DELAY
    } else {
        delay
    };
    (wait, (wait * 2).min(CHILD_RESTART_MAX_DELAY))
}

struct ManagedChild {
    device: String,
    process: Option<Child>,
    started_at: Instant,
    restart_at: Option<Instant>,
    restart_delay: Duration,
}

fn name_has_owner(conn: &zbus::blocking::Connection, name: &str) -> zbus::Result<bool> {
    let bus = zbus::blocking::fdo::DBusProxy::new(conn)?;
    Ok(bus.name_has_owner(zbus::names::BusName::try_from(name.to_string())?)?)
}

/// Asks a MyGestures daemon owning `name` to stop, returning whether there was one. It knows
/// nothing of this release's lock and would keep its device grabbed.
fn request_legacy_stop(conn: &zbus::blocking::Connection, name: &str) -> zbus::Result<bool> {
    // The old name may still be D-Bus activatable, so calling it without an owner could start
    // the old daemon.
    if !name_has_owner(conn, name)? {
        return Ok(false);
    }
    // The owner exits without replying, so an error here is the expected result.
    let _ = conn.call_method(
        Some(name),
        LEGACY_DBUS_PATH,
        Some(LEGACY_DBUS_NAME),
        "Stop",
        &(),
    );
    Ok(true)
}

/// Stops a MyGestures daemon left running in this session, typically across a package upgrade.
/// Bounded by a timeout, so a hung bus or daemon can't prevent startup.
fn stop_legacy_instance() {
    if std::env::var("GESTOS_CHILD").is_ok() {
        return;
    }
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let stopped = (|| -> zbus::Result<bool> {
            let conn = zbus::blocking::Connection::session()?;
            if !request_legacy_stop(&conn, LEGACY_DBUS_NAME)? {
                return Ok(false);
            }
            while name_has_owner(&conn, LEGACY_DBUS_NAME)? {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(true)
        })();
        let _ = done.send(stopped);
    });
    match finished.recv_timeout(LEGACY_STOP_TIMEOUT) {
        Ok(Ok(true)) => println!("gestosd: Stopped a running MyGestures daemon."),
        Ok(Ok(false)) => {}
        Ok(Err(e)) => eprintln!("gestosd: Could not check for a MyGestures daemon: {}", e),
        Err(_) => eprintln!("gestosd: Timed out stopping a running MyGestures daemon."),
    }
}

fn start_dbus_service() {
    if std::env::var("GESTOS_CHILD").is_ok() {
        return;
    }
    std::thread::spawn(|| {
        // The bus doesn't queue this request, and an instance being replaced may still own
        // the name for a moment.
        let mut result = None;
        for attempt in 0..DBUS_NAME_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(DBUS_NAME_RETRY_INTERVAL);
            }
            let built = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name(gestos::config::get_dbus_name()))
                .and_then(|b| b.serve_at(DBUS_PATH, DaemonDbus))
                .and_then(|b| b.build());
            let done = built.is_ok();
            result = Some(built);
            if done {
                break;
            }
        }
        match result {
            Some(Ok(_conn)) => {
                println!("gestosd: Registered D-Bus service.");
                loop {
                    std::thread::sleep(Duration::from_secs(3600));
                }
            }
            Some(Err(e)) => eprintln!("gestosd: D-Bus initialization error: {}", e),
            None => {}
        }
    });
}

fn spawn_device_process(
    exe: &Path,
    device: &str,
    button: i32,
    sensitivity: i32,
    custom_config: Option<&str>,
) -> Option<Child> {
    let mut cmd = Command::new(exe);
    cmd.env("GESTOS_CHILD", "1");
    cmd.arg("-d").arg(device);
    cmd.arg("-b").arg(button.to_string());
    cmd.arg("-s").arg(sensitivity.to_string());
    if let Some(cfg) = custom_config {
        cmd.arg(cfg);
    }
    match cmd.spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            eprintln!("Failed to spawn listener process for {}: {}", device, e);
            None
        }
    }
}

/// Runs one child process per device, restarting any that fail with an increasing delay.
fn run_manager(devices: &[String], button: i32, sensitivity: i32, custom_config: Option<&str>) {
    IS_MANAGER.store(true, Ordering::Relaxed);
    let exe = std::env::current_exe().unwrap();
    let now = Instant::now();
    let mut children: Vec<ManagedChild> = devices
        .iter()
        .map(|device| ManagedChild {
            device: device.clone(),
            process: None,
            started_at: now,
            restart_at: Some(now),
            restart_delay: CHILD_RESTART_MIN_DELAY,
        })
        .collect();

    loop {
        let now = Instant::now();
        for child in &mut children {
            if let Some(process) = child.process.as_mut() {
                if let Ok(Some(status)) = process.try_wait() {
                    child.process = None;
                    if should_restart(status) {
                        let (wait, next_delay) =
                            restart_backoff(child.restart_delay, now - child.started_at);
                        eprintln!(
                            "gestosd: Listener for {} exited ({}). Restarting in {}s.",
                            child.device,
                            status,
                            wait.as_secs()
                        );
                        child.restart_at = Some(now + wait);
                        child.restart_delay = next_delay;
                    }
                }
            }
        }

        let any_running = children.iter().any(|c| c.process.is_some());
        if !any_running && RELOAD_MANAGER.swap(false, Ordering::Relaxed) {
            println!("gestosd parent: Config reload triggered. Restarting child processes...");
            for child in &mut children {
                child.restart_at = Some(now);
                child.restart_delay = CHILD_RESTART_MIN_DELAY;
            }
        }

        for child in &mut children {
            if child.restart_at.is_some_and(|at| at <= now) {
                child.restart_at = None;
                child.started_at = now;
                child.process =
                    spawn_device_process(&exe, &child.device, button, sensitivity, custom_config);
            }
        }

        // Exited children are removed here, so a reload never signals a reused pid.
        *CHILD_PIDS.lock().unwrap() = children
            .iter()
            .filter_map(|c| c.process.as_ref().map(Child::id))
            .collect();

        if children
            .iter()
            .all(|c| c.process.is_none() && c.restart_at.is_none())
        {
            return;
        }
        std::thread::sleep(CHILD_POLL_INTERVAL);
    }
}

/// Runs the MyGestures migration with the user's own group, so that this binary's elevated
/// `input` group doesn't end up owning files in their home.
fn migrate_legacy_files_as_user() {
    if std::env::var("GESTOS_CHILD").is_ok() {
        return;
    }
    let rgid = nix::unistd::getgid();
    let egid = nix::unistd::getegid();
    if egid != rgid && nix::unistd::setegid(rgid).is_err() {
        eprintln!("gestosd: Skipping the MyGestures migration: could not lower group rights.");
        return;
    }
    gestos::config::migrate_legacy_user_files(
        &gestos::config::config_home(),
        gestos::config::autostart_dir().as_deref(),
    );
    if egid != rgid && nix::unistd::setegid(egid).is_err() {
        eprintln!("gestosd: Could not restore group rights after the MyGestures migration.");
        std::process::exit(1);
    }
}

fn parse_number_arg(flag: &str, value: &str) -> i32 {
    value.parse().unwrap_or_else(|_| {
        eprintln!("Error: {} needs a number, got '{}'.", flag, value);
        std::process::exit(EXIT_PERMANENT_ERROR);
    })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut devices = Vec::new();
    let mut button = 3;
    let mut sensitivity = 30;
    let mut help = false;
    let mut custom_config = None;

    while let Some(arg) = args.next() {
        if arg == "-h" || arg == "--help" {
            help = true;
        } else if arg == "-b" || arg == "--button" {
            if let Some(val) = args.next() {
                button = parse_number_arg("--button", &val);
            }
        } else if arg == "-s" || arg == "--sensitivity" {
            if let Some(val) = args.next() {
                sensitivity = parse_number_arg("--sensitivity", &val);
            }
        } else if arg == "-d" || arg == "--device" {
            if let Some(val) = args.next() {
                devices.push(val);
            }
        } else if arg.starts_with("--device=") {
            devices.push(arg.trim_start_matches("--device=").to_string());
        } else if arg.starts_with("--button=") {
            button = parse_number_arg("--button", arg.trim_start_matches("--button="));
        } else if arg.starts_with("--sensitivity=") {
            sensitivity =
                parse_number_arg("--sensitivity", arg.trim_start_matches("--sensitivity="));
        } else if !arg.starts_with('-') {
            custom_config = Some(arg);
        }
    }

    if help {
        println!("Usage: gestosd [OPTIONS] [CONFIG_FILE]");
        println!();
        println!("CONFIG_FILE:");
        println!(
            "  Default: {}",
            gestos::config::get_default_config_path().display()
        );
        println!();
        println!("OPTIONS:");
        println!("  -d, --device <DEVICENAME>  : Device to grab (can specify multiple times)");
        println!("  -b, --button <BUTTON>      : Button used to draw the gesture");
        println!("                              Default: '3' (Right Click)");
        println!("  -s, --sensitivity <VAL>    : Distance between recorded points, in tenths");
        println!("                              of a pixel. Default: '30' (3 pixels)");
        println!("  -h, --help                 : Help");
        return;
    }

    if gestos::uinput::trigger_button_code(button).is_none() {
        eprintln!("Error: trigger {} is not a mouse button.", button);
        std::process::exit(EXIT_PERMANENT_ERROR);
    }

    migrate_legacy_files_as_user();

    let config_path = custom_config
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(gestos::config::get_default_config_path);
    let _ = CONFIG_PATH.set(config_path.clone());

    // Resolve target device list
    if devices.is_empty() {
        if let Some(default_mouse) = find_mouse_device() {
            devices.push(default_mouse.to_string_lossy().into_owned());
        } else {
            eprintln!("Could not find default evdev mouse device.");
            std::process::exit(1);
        }
    }

    // If multiple devices are specified, run process manager
    if devices.len() > 1 {
        if devices.iter().all(|device| check_permissions(device)) && config_is_valid() {
            stop_legacy_instance();
        }
        start_dbus_service();
        run_manager(&devices, button, sensitivity, custom_config.as_deref());
        return;
    }

    // Single device execution
    let device_path = &devices[0];
    if !check_permissions(device_path) {
        std::process::exit(1);
    }

    // Load configuration
    if !config_path.exists() {
        eprintln!(
            "Error: Configuration file not found at {}.",
            config_path.display()
        );
        if custom_config.is_none() {
            eprintln!("Please run gestos first to initialize the configuration.");
        }
        std::process::exit(1);
    }
    let Some(config) = Configuration::load_from_file(&config_path) else {
        eprintln!("Failed to load config from: {}", config_path.display());
        std::process::exit(1);
    };

    // Only now, so that a daemon that can't start doesn't take a working one down with it
    stop_legacy_instance();

    // Replace any instance already running on this device; the lock is held until exit
    let _instance_lock = match DaemonIpc::acquire(device_path) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("Failed to acquire the gestosd instance lock: {}", e);
            std::process::exit(1);
        }
    };

    // Registered only now, because the instance just replaced owned the bus name until it exited
    start_dbus_service();

    let wayland_ctx = WaylandContext::discover();

    println!("Trigger button: {}", button);
    println!("Capture method: evdev");
    println!("Starting grabber...");

    if let Err(e) = run_grabber(device_path, button, sensitivity, config, wayland_ctx) {
        eprintln!("gestosd: Daemon error: {}", e);
        let permanent = e.kind() == std::io::ErrorKind::InvalidInput;
        std::process::exit(if permanent { EXIT_PERMANENT_ERROR } else { 1 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exited(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    fn killed_by(signal: i32) -> ExitStatus {
        ExitStatus::from_raw(signal)
    }

    struct FakeLegacyDaemon {
        stop_calls: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    #[interface(name = "org.mygestures.Daemon")]
    impl FakeLegacyDaemon {
        fn stop(&self) {
            self.stop_calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct FakeNotifications {
        calls: std::sync::Arc<std::sync::Mutex<Vec<(u32, String, String, String, i32, bool)>>>,
    }

    #[interface(name = "org.freedesktop.Notifications")]
    impl FakeNotifications {
        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            _app: String,
            replaces: u32,
            icon: String,
            summary: String,
            body: String,
            _actions: Vec<String>,
            hints: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            timeout: i32,
        ) -> u32 {
            let transient = hints
                .get("transient")
                .and_then(|value| bool::try_from(value).ok())
                .unwrap_or(false);
            self.calls
                .lock()
                .unwrap()
                .push((replaces, icon, summary, body, timeout, transient));
            replaces + 7
        }
    }

    #[test]
    fn a_gesture_notification_says_what_ran_for_one_second() {
        let Some(client) = session_bus() else { return };
        let name = test_name("Notifications");
        let calls = std::sync::Arc::default();
        let service = FakeNotifications {
            calls: std::sync::Arc::clone(&calls),
        };
        let _service = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(name.clone()))
            .and_then(|b| b.serve_at(NOTIFICATIONS_PATH, service))
            .and_then(|b| b.build())
            .unwrap();

        let first =
            show_gesture_notification(&client, &name, 0, "Back", "Press keys: Alt+Left", "go-icon")
                .unwrap();
        assert_eq!(first, 7);
        show_gesture_notification(&client, &name, first, "Close", "Close Window", "x").unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(
            calls[0],
            (
                0,
                "go-icon".to_string(),
                "Back".to_string(),
                "Press keys: Alt+Left".to_string(),
                1000,
                true
            )
        );
        assert_eq!(calls[1].0, 7, "the newer notification replaces the older one");
    }

    fn session_bus() -> Option<zbus::blocking::Connection> {
        let conn = zbus::blocking::Connection::session().ok();
        if conn.is_none() {
            eprintln!("skipped: no D-Bus session bus");
        }
        conn
    }

    // Names are unique per test and process, so this can't reach a real daemon on the bus.
    fn test_name(suffix: &str) -> String {
        format!("io.github.deters.GestosTest{}.P{}", suffix, std::process::id())
    }

    #[test]
    fn a_legacy_daemon_is_stopped() {
        let Some(client) = session_bus() else { return };
        let name = test_name("Legacy");
        let stop_calls = std::sync::Arc::default();
        let daemon = FakeLegacyDaemon {
            stop_calls: std::sync::Arc::clone(&stop_calls),
        };
        let _service = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(name.clone()))
            .and_then(|b| b.serve_at(LEGACY_DBUS_PATH, daemon))
            .and_then(|b| b.build())
            .unwrap();

        assert!(request_legacy_stop(&client, &name).unwrap());
        assert_eq!(stop_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_name_without_an_owner_is_not_called() {
        let Some(client) = session_bus() else { return };
        assert!(!request_legacy_stop(&client, &test_name("Absent")).unwrap());
    }

    #[test]
    fn this_release_exposes_the_version_property() {
        let Some(client) = session_bus() else { return };
        let name = test_name("Real");
        // Only the property is read here: the real Stop method would exit the test process.
        let _service = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(name.clone()))
            .and_then(|b| b.serve_at(DBUS_PATH, DaemonDbus))
            .and_then(|b| b.build())
            .unwrap();

        let reply = client
            .call_method(
                Some(name.as_str()),
                DBUS_PATH,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("io.github.deters.Gestos.Daemon", "Version"),
            )
            .unwrap();
        let body = reply.body();
        let version: zbus::zvariant::Value = body.deserialize().unwrap();
        assert_eq!(version, zbus::zvariant::Value::from(env!("CARGO_PKG_VERSION")));
    }

    fn sample_gesture(name: &str, modifiers: &str) -> Gesture {
        Gesture {
            id: name.to_string(),
            name: name.to_string(),
            raw_movement: String::new(),
            points: Vec::new(),
            actions: Vec::new(),
            is_custom: true,
            is_modified: false,
            is_deleted: false,
            modifiers: Modifiers::parse(modifiers),
        }
    }

    #[test]
    fn only_gestures_with_exactly_the_held_modifiers_can_match() {
        let config = Configuration {
            gestures: vec![
                sample_gesture("plain", ""),
                sample_gesture("ctrl", "ctrl"),
                sample_gesture("ctrl-alt", "ctrl+alt"),
            ],
            apps: Vec::new(),
            user_config_path: PathBuf::new(),
        };
        let names = |modifiers: &str| -> Vec<String> {
            gesture_templates(&config, Modifiers::parse(modifiers))
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        };
        assert_eq!(names(""), ["plain"]);
        assert_eq!(names("ctrl"), ["ctrl"]);
        assert_eq!(names("ctrl+alt"), ["ctrl-alt"]);
        assert!(names("shift").is_empty());
    }

    #[test]
    fn restarts_only_children_that_failed_unexpectedly() {
        assert!(should_restart(exited(1)));
        assert!(should_restart(exited(101)));
        assert!(should_restart(killed_by(libc::SIGSEGV)));
        assert!(should_restart(killed_by(libc::SIGKILL)));

        assert!(!should_restart(exited(0)));
        assert!(!should_restart(exited(EXIT_PERMANENT_ERROR)));
        assert!(!should_restart(killed_by(libc::SIGTERM)));
    }

    #[test]
    fn restart_delay_doubles_up_to_the_maximum() {
        let quick = Duration::from_secs(1);
        assert_eq!(
            restart_backoff(Duration::from_secs(1), quick),
            (Duration::from_secs(1), Duration::from_secs(2))
        );
        assert_eq!(
            restart_backoff(Duration::from_secs(16), quick),
            (Duration::from_secs(16), Duration::from_secs(30))
        );
        assert_eq!(
            restart_backoff(Duration::from_secs(30), quick),
            (Duration::from_secs(30), Duration::from_secs(30))
        );
    }

    #[test]
    fn restart_delay_resets_after_a_stable_run() {
        assert_eq!(
            restart_backoff(Duration::from_secs(30), CHILD_STABLE_RUNTIME),
            (Duration::from_secs(1), Duration::from_secs(2))
        );
    }
}
