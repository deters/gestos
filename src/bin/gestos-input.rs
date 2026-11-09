//! The only part of Gestos with access to input devices. It forwards the pointer, records gesture
//! paths and sends keys for the daemon, and never reads key events: all it asks of a keyboard is
//! which modifiers are down. It only talks to the user active on the seat, and takes no
//! configuration, so that it stays small enough to check.

// Built from these files directly, not through the gestos library, which uses GIO: a library this
// binary links is code running with the rights to read every keyboard. Some of what they contain
// is for the daemon, and only what's called here ends up in the binary.
#[allow(dead_code)]
#[path = "../gesture.rs"]
mod gesture;
#[allow(dead_code)]
#[path = "../input_protocol.rs"]
mod input_protocol;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../keyboards.rs"]
mod keyboards;
#[allow(dead_code)]
#[path = "../modifiers.rs"]
mod modifiers;
#[allow(dead_code)]
#[path = "../protractor.rs"]
mod protractor;
#[allow(dead_code)]
#[path = "../uinput.rs"]
mod uinput;

#[cfg(target_os = "linux")]
mod service {
    use crate::gesture::GestureRecognizer;
    use crate::input_protocol::{
        active_seat_uid, is_event_node, may_press_key, may_report_held, Event, LineBuffer, Request,
        SOCKET_PATH,
    };
    use crate::keyboards::{created_event_nodes, is_physical_keyboard, Keyboards};
    use crate::modifiers::HeldKeys;
    use crate::uinput::{trigger_button_code, UinputDevice};
    use evdev::{Device, EventType, KeyCode};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::time::{Duration, Instant};

    const SEAT_FILE: &str = "/run/systemd/seats/seat0";
    const SYSTEMD_FIRST_FD: i32 = 3;
    const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
    const SEAT_CHECK_INTERVAL: Duration = Duration::from_secs(1);
    // Gives udev time to set a new device's permissions.
    const KEYBOARD_SETTLE_DELAY: Duration = Duration::from_millis(500);
    const GRAB_ATTEMPTS: u32 = 5;
    const GRAB_RETRY_INTERVAL: Duration = Duration::from_millis(100);
    const MAX_SENSITIVITY: i32 = 100_000;
    const MAX_PENDING_BYTES: usize = 1024 * 1024;
    // Keeps GNOME from treating a released Super as a tap, which opens the Overview. It's F24,
    // which nothing normally uses. Tapped with any modifier key held, since which of them is Super
    // depends on the layout.
    const UNUSED_KEY: u16 = KeyCode::KEY_F24.0;

    struct Refusal {
        permanent: bool,
        message: String,
    }

    fn refused(permanent: bool, message: impl Into<String>) -> Refusal {
        Refusal {
            permanent,
            message: message.into(),
        }
    }

    fn peer_uid(stream: &UnixStream) -> std::io::Result<u32> {
        let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
        let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                &mut credentials as *mut _ as *mut libc::c_void,
                &mut length,
            )
        };
        if result == 0 {
            Ok(credentials.uid)
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    fn active_uid() -> Option<u32> {
        active_seat_uid(&std::fs::read_to_string(SEAT_FILE).ok()?)
    }

    /// Events to the daemon, which must never hold up the pointer. If it stops reading, what
    /// doesn't fit is dropped: only whole lines go in, so what arrives still makes sense.
    struct Outbox {
        stream: UnixStream,
        pending: Vec<u8>,
    }

    impl Outbox {
        fn send(&mut self, event: &Event) {
            let line = event.encode();
            if self.pending.len() + line.len() <= MAX_PENDING_BYTES {
                self.pending.extend_from_slice(line.as_bytes());
            }
        }

        /// Returns false once the daemon is gone.
        fn flush(&mut self) -> bool {
            while !self.pending.is_empty() {
                match self.stream.write(&self.pending) {
                    Ok(0) => return false,
                    Ok(written) => {
                        self.pending.drain(..written);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return false,
                }
            }
            true
        }
    }

    /// Grabs the device, waiting briefly for an instance that is exiting to release it.
    fn grab_device(dev: &mut Device) -> std::io::Result<()> {
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

    /// Watches for new input devices.
    fn watch_input_devices() -> Option<File> {
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let watched = unsafe {
            libc::inotify_add_watch(fd, c"/dev/input".as_ptr(), libc::IN_CREATE)
        };
        (watched >= 0).then_some(file)
    }

    /// Keyboards are only open while a gesture needs a modifier.
    #[derive(Default)]
    struct ModifierReader {
        keyboards: Option<Keyboards>,
        watch: Option<File>,
        reopen_at: Option<Instant>,
    }

    impl ModifierReader {
        fn set_enabled(&mut self, enabled: bool) {
            if enabled == self.keyboards.is_some() {
                return;
            }
            self.keyboards = enabled.then(Keyboards::open);
            self.watch = enabled.then(watch_input_devices).flatten();
            self.reopen_at = None;
        }

        fn held(&self) -> HeldKeys {
            self.keyboards.as_ref().map(Keyboards::held).unwrap_or_default()
        }

        fn read_watch(&mut self) {
            let Some(watch) = self.watch.as_mut() else {
                return;
            };
            let mut buffer = [0u8; 4096];
            if let Ok(length) = watch.read(&mut buffer) {
                if created_event_nodes(&buffer[..length])
                    .iter()
                    .any(|node| is_physical_keyboard(node))
                {
                    self.reopen_at = Some(Instant::now() + KEYBOARD_SETTLE_DELAY);
                }
            }
        }

        fn reopen_if_due(&mut self) {
            if self.reopen_at.is_some_and(|at| at <= Instant::now()) {
                self.reopen_at = None;
                self.keyboards = Some(Keyboards::open());
            }
        }
    }

    /// The device a client asked for, if it's a pointing device with that button. Asking for
    /// anything else is refused, since this service can open what the client can't.
    fn open_device(path: &str, button: u16) -> Result<Device, Refusal> {
        let resolved = std::fs::canonicalize(path)
            .map_err(|e| refused(false, format!("Cannot find device '{path}': {e}")))?;
        if !is_event_node(&resolved) {
            return Err(refused(true, format!("'{path}' is not an input device.")));
        }
        let dev = Device::open(&resolved)
            .map_err(|e| refused(false, format!("Cannot read device '{path}': {e}")))?;
        if !dev.supported_keys().is_some_and(|keys| keys.contains(KeyCode(button))) {
            return Err(refused(
                true,
                format!("Device {path} does not have trigger button {button}."),
            ));
        }
        Ok(dev)
    }

    /// How far the pointer must move for a new point, and for a path to count as a gesture.
    /// Absolute units vary by device, so they scale from its resolution (units per mm) or range.
    fn path_thresholds(dev: &Device, sensitivity: i32) -> (f64, f64) {
        let mut threshold = sensitivity as f64;
        let mut min_path_length = 15.0;
        let abs_x = dev
            .get_absinfo()
            .ok()
            .and_then(|mut iter| iter.find(|(code, _)| *code == evdev::AbsoluteAxisCode::ABS_X))
            .map(|(_, info)| info);
        if let Some(abs_info) = abs_x {
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
        (threshold, min_path_length)
    }

    fn read_open_request(stream: &mut UnixStream) -> Result<Request, Refusal> {
        let not_understood = || refused(true, "The request was not understood.");
        stream
            .set_read_timeout(Some(OPEN_TIMEOUT))
            .map_err(|e| refused(false, e.to_string()))?;
        let mut lines = LineBuffer::default();
        let mut bytes = [0u8; 512];
        loop {
            if let Some(line) = lines.next_line() {
                return Request::decode(&line).ok_or_else(not_understood);
            }
            if lines.overflowed() {
                return Err(not_understood());
            }
            let length = stream.read(&mut bytes).map_err(|e| refused(false, e.to_string()))?;
            if length == 0 {
                return Err(refused(false, "The connection closed."));
            }
            lines.push(&bytes[..length]);
        }
    }

    fn run_session(mut stream: UnixStream) -> Result<(), Refusal> {
        let peer = peer_uid(&stream).map_err(|e| refused(false, e.to_string()))?;
        if active_uid() != Some(peer) {
            return Err(refused(false, "Only the user active on this seat can use gestures."));
        }

        let Request::Open {
            button,
            sensitivity,
            keyboards,
            device,
        } = read_open_request(&mut stream)?
        else {
            return Err(refused(true, "Nothing has been opened yet."));
        };
        let target_button = trigger_button_code(button)
            .ok_or_else(|| refused(true, format!("Trigger {button} is not a mouse button.")))?;
        if !(1..=MAX_SENSITIVITY).contains(&sensitivity) {
            return Err(refused(true, format!("Sensitivity {sensitivity} is out of range.")));
        }
        let mut dev = open_device(&device, target_button)?;
        let (threshold, min_path_length) = path_thresholds(&dev, sensitivity);

        // Create the virtual device first: a grabbed mouse with nowhere to forward its events
        // would leave the user without a pointer.
        let uinput = match UinputDevice::init_from_device(&dev) {
            Ok(u) => Some(u),
            Err(e) => {
                eprintln!("gestos-input: Failed to initialize uinput: {}", e);
                None
            }
        };

        // Exclusive grab (only for button 3 by default)
        let mut grabbed = false;
        if button == 3 {
            if uinput.is_none() {
                eprintln!("gestos-input: Not grabbing the device because uinput is unavailable.");
            } else if let Err(e) = grab_device(&mut dev) {
                eprintln!("gestos-input: Failed to grab device exclusively: {}", e);
            } else {
                grabbed = true;
            }
        }

        let mut modifiers = ModifierReader::default();
        modifiers.set_enabled(keyboards);

        stream
            .set_nonblocking(true)
            .map_err(|e| refused(false, e.to_string()))?;
        let mut reader = stream.try_clone().map_err(|e| refused(false, e.to_string()))?;
        let mut outbox = Outbox {
            stream,
            pending: Vec::new(),
        };
        outbox.send(&Event::Ready);

        let mut lines = LineBuffer::default();
        let mut recognizer = GestureRecognizer::new(threshold / 10.0, min_path_length);
        let mut pending_frame: Vec<evdev::InputEvent> = Vec::new();
        let mut held_at_press = HeldKeys::default();
        let mut next_seat_check = Instant::now() + SEAT_CHECK_INTERVAL;
        let mut last_gesture = None;
        // Clicking left while right draws runs its own action instead of a gesture. Only a
        // grabbed mouse can keep that click from the app under the pointer.
        let mut chord_enabled = false;
        let mut trigger_held = false;
        let mut chorded = false;
        let mut holding_back_left = false;

        loop {
            if !outbox.flush() {
                return Ok(());
            }
            let now = Instant::now();
            if now >= next_seat_check {
                next_seat_check = now + SEAT_CHECK_INTERVAL;
                if active_uid() != Some(peer) {
                    return Err(refused(false, "Another user is now active on this seat."));
                }
            }
            modifiers.reopen_if_due();

            let wake_at = modifiers.reopen_at.map_or(next_seat_check, |at| at.min(next_seat_check));
            let timeout = wake_at.saturating_duration_since(now).as_millis() as i32 + 1;
            let mut poll_fds = [
                libc::pollfd {
                    fd: dev.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: reader.as_raw_fd(),
                    events: if outbox.pending.is_empty() {
                        libc::POLLIN
                    } else {
                        libc::POLLIN | libc::POLLOUT
                    },
                    revents: 0,
                },
                // A negative descriptor is ignored.
                libc::pollfd {
                    fd: modifiers.watch.as_ref().map_or(-1, |watch| watch.as_raw_fd()),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            if unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as libc::nfds_t, timeout) } < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(refused(false, err.to_string()));
            }

            if poll_fds[2].revents != 0 {
                modifiers.read_watch();
            }

            if poll_fds[1].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                let mut bytes = [0u8; 4096];
                match reader.read(&mut bytes) {
                    Ok(0) => return Ok(()),
                    Ok(length) => lines.push(&bytes[..length]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return Ok(()),
                }
                while let Some(line) = lines.next_line() {
                    match Request::decode(&line) {
                        Some(Request::Key { code, pressed }) if may_press_key(code) => {
                            // Keys must not reach another user's session, even for a moment.
                            if active_uid() != Some(peer) {
                                return Err(refused(false, "Another user is now active on this seat."));
                            }
                            if let Some(ref u) = uinput {
                                u.key(code, pressed);
                            }
                        }
                        Some(Request::SetKeyboards(enabled)) => modifiers.set_enabled(enabled),
                        Some(Request::SetChord(enabled)) => chord_enabled = enabled,
                        Some(Request::QueryHeld) => {
                            let held = if may_report_held(last_gesture, Instant::now()) {
                                modifiers.held()
                            } else {
                                HeldKeys::default()
                            };
                            outbox.send(&Event::Held(held));
                        }
                        _ => return Err(refused(true, "The request was not understood.")),
                    }
                }
                if lines.overflowed() {
                    return Err(refused(true, "The request was not understood."));
                }
            }

            if poll_fds[0].revents == 0 {
                continue;
            }

            let events = match dev.fetch_events() {
                Ok(events) => events,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(refused(false, format!("Lost device '{device}': {e}"))),
            };

            for ev in events {
                let ev_type = ev.event_type();
                let ev_code = ev.code();
                let ev_value = ev.value();

                // The left click of a chord, and its release, never reach the app.
                if ev_type == EventType::KEY && ev_code == KeyCode::BTN_LEFT.0 && grabbed {
                    if ev_value == 1 && chord_enabled && trigger_held {
                        holding_back_left = true;
                        if !chorded {
                            chorded = true;
                            outbox.send(&Event::Chord);
                        }
                        continue;
                    }
                    if ev_value == 0 && holding_back_left {
                        holding_back_left = false;
                        continue;
                    }
                }

                if ev_type == EventType::KEY && ev_code == target_button {
                    if ev_value == 1 {
                        trigger_held = true;
                        chorded = false;
                        held_at_press = modifiers.held();
                        let start = recognizer.press();
                        outbox.send(&Event::Started(start.x, start.y));
                    } else if ev_value == 0 && chorded {
                        // It was a chord, so the right button neither draws nor clicks.
                        trigger_held = false;
                        chorded = false;
                        recognizer.release();
                        outbox.send(&Event::Ended);
                    } else if ev_value == 0 {
                        trigger_held = false;
                        let path = recognizer.release();
                        outbox.send(&Event::Ended);

                        if let Some(points) = path {
                            if !held_at_press.is_empty() {
                                if let Some(ref u) = uinput {
                                    u.tap_key(UNUSED_KEY);
                                }
                            }
                            for point in &points {
                                outbox.send(&Event::Point(point.x, point.y));
                            }
                            outbox.send(&Event::Finished(held_at_press.clone()));
                            last_gesture = Some(Instant::now());
                        } else if grabbed {
                            // Click emulation
                            if let Some(ref u) = uinput {
                                let u = u.clone();
                                std::thread::spawn(move || u.click(button));
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
                        outbox.send(&Event::Updated(point.x, point.y));
                    }
                }
            }
        }
    }

    fn serve(stream: UnixStream) {
        let Ok(mut reply) = stream.try_clone() else {
            return;
        };
        if let Err(refusal) = run_session(stream) {
            eprintln!("gestos-input: {}", refusal.message);
            // A session that made the stream non-blocking may lose this, which only costs the
            // daemon the reason.
            let _ = reply.write_all(
                Event::Refused {
                    permanent: refusal.permanent,
                    message: refusal.message,
                }
                .encode()
                .as_bytes(),
            );
        }
    }

    /// For service managers that don't pass connections to their services.
    fn listen(path: &str) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        // Who may use the service is decided per connection.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || serve(stream));
        }
        Ok(())
    }

    fn connection_from_systemd() -> Option<UnixStream> {
        let for_this_process = std::env::var("LISTEN_PID").ok()?.parse::<u32>().ok()? == std::process::id();
        let one_socket = std::env::var("LISTEN_FDS").ok()? == "1";
        (for_this_process && one_socket)
            .then(|| unsafe { UnixStream::from_raw_fd(SYSTEMD_FIRST_FD) })
    }

    pub fn main() -> std::process::ExitCode {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.first().is_some_and(|arg| arg == "--listen") {
            let path = args.get(1).map_or(SOCKET_PATH, String::as_str);
            if let Err(e) = listen(path) {
                eprintln!("gestos-input: Cannot listen on {path}: {e}");
                return std::process::ExitCode::FAILURE;
            }
            return std::process::ExitCode::SUCCESS;
        }
        let Some(stream) = connection_from_systemd() else {
            eprintln!("gestos-input: This is a service. Start it through its socket, or with --listen.");
            return std::process::ExitCode::FAILURE;
        };
        serve(stream);
        std::process::ExitCode::SUCCESS
    }
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    service::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("gestos-input only runs on Linux.");
}
