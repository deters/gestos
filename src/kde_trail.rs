//! The path of a gesture on KDE's Wayland, where no program can ask where the pointer is. While a
//! gesture is drawn, a KWin script says each place the pointer moves to, and the path is drawn
//! there, in a layer over the screen the gesture started on that clicks go through.
//!
//! The daemon reads the mouse before KWin speeds the pointer up, so its own points wouldn't
//! follow the pointer; KWin's do.

use crate::active_app::kwin;
use crate::trail::{bounds, coverage, premultiplied, Path, TrailEvent};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How often the path is shown again while a gesture is drawn.
const FRAME: Duration = Duration::from_millis(16);
/// How long the path takes to fade.
const FADE_TIME: Duration = Duration::from_millis(300);
/// How long KWin gets to run the script and say where the pointer is.
const FIRST_REPORT_TIMEOUT: Duration = Duration::from_millis(500);
const REPORT_PATH: &str = "/io/github/deters/Gestos/Pointer";
const REPORT_INTERFACE: &str = "io.github.deters.Gestos.Pointer";

type Error = Box<dyn std::error::Error>;

enum Event {
    Started,
    Ended,
    /// Where the pointer is, in KWin's coordinates for the whole desktop.
    Pointer(f64, f64),
    /// Whether the window in use covers its screen, as a game or a video does.
    Fullscreen(bool),
}

/// Where the script's reports arrive.
struct Report(Mutex<Sender<Event>>);

#[zbus::interface(name = "io.github.deters.Gestos.Pointer")]
impl Report {
    fn pointer(&self, x: i32, y: i32) {
        let _ = self.0.lock().map(|sender| sender.send(Event::Pointer(f64::from(x), f64::from(y))));
    }

    fn fullscreen(&self, fullscreen: bool) {
        let _ = self.0.lock().map(|sender| sender.send(Event::Fullscreen(fullscreen)));
    }
}

/// The script, which says whether the window in use is fullscreen, and then where the pointer is
/// each time it moves, to this connection, in whole points, as D-Bus needs one type. Plasma 6 calls windows what Plasma 5 called clients.
fn script(reply_to: &str) -> String {
    format!(
        "const say = (method, ...args) => callDBus(\"{reply_to}\", \"{REPORT_PATH}\", \"{REPORT_INTERFACE}\", method, ...args);\n\
         const window = workspace.activeWindow || workspace.activeClient;\n\
         say(\"Fullscreen\", Boolean(window && window.fullScreen));\n\
         const pointer = () => say(\"Pointer\", Math.round(workspace.cursorPos.x), Math.round(workspace.cursorPos.y));\n\
         pointer();\n\
         workspace.cursorPosChanged.connect(pointer);\n"
    )
}

/// Starts drawing paths on their own thread. `show` is asked as each gesture starts.
pub fn spawn(show: impl Fn() -> bool + Send + 'static) -> Sender<TrailEvent> {
    let (events, received) = std::sync::mpsc::channel();
    let (trail_events, trail_received) = std::sync::mpsc::channel::<TrailEvent>();
    let forwarded = events.clone();
    std::thread::spawn(move || {
        for event in trail_received {
            let event = match event {
                TrailEvent::Started => Event::Started,
                TrailEvent::Ended => Event::Ended,
            };
            if forwarded.send(event).is_err() {
                return;
            }
        }
    });
    std::thread::spawn(move || run(events, &received, show));
    trail_events
}

fn run(events: Sender<Event>, received: &Receiver<Event>, show: impl Fn() -> bool) {
    let mut bus: Option<zbus::blocking::Connection> = None;
    let mut screen: Option<wayland::Screen> = None;
    let mut warned = false;
    while let Ok(event) = received.recv() {
        if !matches!(event, Event::Started) || !show() {
            continue;
        }
        if bus.is_none() {
            bus = connect_bus(&events).map_err(|e| warn(&mut warned, e)).ok();
        }
        if screen.is_none() {
            screen = wayland::Screen::connect().map_err(|e| warn(&mut warned, e)).ok();
        }
        let (Some(conn), Some(connected)) = (&bus, &mut screen) else {
            wait_for_end(received);
            continue;
        };
        let name = format!("gestos-pointer-{}", std::process::id());
        let reply_to = conn.unique_name().map(|name| name.to_string()).unwrap_or_default();
        let drawn = kwin::load_script(conn, &name, &script(&reply_to))
            .and_then(|_| draw_gesture(connected, received));
        kwin::unload_script(conn, &name);
        // A connection that broke, as when KWin restarted, is made again for the next gesture.
        if let Err(e) = drawn {
            eprintln!("gestosd: Stopped drawing a gesture's path: {e}");
            screen = None;
            bus = None;
        }
        // Reports sent before the script stopped aren't the next gesture's.
        while let Ok(event) = received.try_recv() {
            if matches!(event, Event::Started) {
                // Started again so quickly: this one is lost, and the path not drawn.
                break;
            }
        }
    }
}

fn warn(warned: &mut bool, error: Error) {
    if !*warned {
        eprintln!("gestosd: Can't draw gesture paths: {error}");
        *warned = true;
    }
}

fn connect_bus(events: &Sender<Event>) -> Result<zbus::blocking::Connection, Error> {
    let conn = zbus::blocking::Connection::session()?;
    conn.object_server().at(REPORT_PATH, Report(Mutex::new(events.clone())))?;
    Ok(conn)
}

fn wait_for_end(received: &Receiver<Event>) {
    while let Ok(event) = received.recv() {
        if matches!(event, Event::Ended) {
            return;
        }
    }
}

/// Draws one gesture's path until it ends, and takes it away.
fn draw_gesture(screen: &mut wayland::Screen, received: &Receiver<Event>) -> Result<(), Error> {
    // The script says first whether the window is fullscreen, and then where the pointer is.
    let deadline = Instant::now() + FIRST_REPORT_TIMEOUT;
    let start = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match received.recv_timeout(left) {
            Ok(Event::Pointer(x, y)) => break (x, y),
            Ok(Event::Fullscreen(true)) => return finish(received),
            Ok(Event::Ended) => return Ok(()),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {
                eprintln!("gestosd: KWin didn't say where the pointer is");
                return finish(received);
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    };

    let mut layer = screen.open_layer(start)?;
    let mut path = Path::new(f64::from(layer.scale));
    let mut last = layer.to_pixels(start);
    path.points.push(last);
    let mut shown = Instant::now();
    loop {
        match received.recv_timeout(FRAME) {
            Ok(Event::Pointer(x, y)) => {
                let point = layer.to_pixels((x, y));
                if point != last {
                    layer.draw(&path.segment_to(point));
                    last = point;
                }
            }
            Ok(Event::Ended) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Event::Started | Event::Fullscreen(_)) | Err(RecvTimeoutError::Timeout) => {}
        }
        if shown.elapsed() >= FRAME {
            screen.present(&mut layer)?;
            shown = Instant::now();
        }
    }
    screen.present(&mut layer)?;
    screen.fade(layer, FADE_TIME)
}

/// Waits for the gesture to end, when its path isn't drawn.
fn finish(received: &Receiver<Event>) -> Result<(), Error> {
    wait_for_end(received);
    Ok(())
}

mod wayland {
    use super::{bounds, coverage, premultiplied, Error};
    use crate::trail::Segment;
    use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
    use smithay_client_toolkit::output::{OutputHandler, OutputState};
    use smithay_client_toolkit::reexports::client::globals::registry_queue_init;
    use smithay_client_toolkit::reexports::client::protocol::{wl_output, wl_shm, wl_surface};
    use smithay_client_toolkit::reexports::client::{Connection, EventQueue, QueueHandle};
    use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
    use smithay_client_toolkit::shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure,
    };
    use smithay_client_toolkit::shell::WaylandSurface;
    use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
    use smithay_client_toolkit::shm::{Shm, ShmHandler};
    use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
    use std::time::{Duration, Instant};

    /// How long the desktop gets to make room for the layer.
    const CONFIGURE_TIMEOUT: Duration = Duration::from_millis(500);

    pub struct Screen {
        queue: EventQueue<State>,
        state: State,
        compositor: CompositorState,
        layer_shell: LayerShell,
    }

    struct State {
        registry: RegistryState,
        outputs: OutputState,
        shm: Shm,
        /// The size the desktop gave the layer, in its points, once it has.
        configured: Option<(u32, u32)>,
        closed: bool,
    }

    /// A rectangle of pixels: x, y, width and height.
    type Rect = (i32, i32, i32, i32);

    /// A layer over one screen, the path drawn in it, and two buffers to show it in, so one can be
    /// drawn in while the desktop shows the other.
    pub struct PathLayer {
        layer: LayerSurface,
        pool: SlotPool,
        buffers: [Buffer; 2],
        /// What each buffer still lacks of the path.
        missing: [Vec<Rect>; 2],
        pixels: Vec<u32>,
        width: i32,
        height: i32,
        pub scale: i32,
        /// Where the screen is on the desktop, in KWin's coordinates.
        origin: (i32, i32),
        /// What the path covers, to fade only that.
        drawn: Option<Rect>,
    }

    impl Screen {
        pub fn connect() -> Result<Self, Error> {
            let conn = Connection::connect_to_env()?;
            let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
            let qh = queue.handle();
            let compositor = CompositorState::bind(&globals, &qh)?;
            let layer_shell = LayerShell::bind(&globals, &qh)
                .map_err(|_| "this desktop has no layer shell to draw in")?;
            let mut state = State {
                registry: RegistryState::new(&globals),
                outputs: OutputState::new(&globals, &qh),
                shm: Shm::bind(&globals, &qh)?,
                configured: None,
                closed: false,
            };
            // The screens say where they are and their scale once asked.
            queue.roundtrip(&mut state)?;
            queue.roundtrip(&mut state)?;
            Ok(Screen { queue, state, compositor, layer_shell })
        }

        /// A layer over the screen `point` is on, which clicks go through.
        pub fn open_layer(&mut self, point: (f64, f64)) -> Result<PathLayer, Error> {
            let outputs = &self.state.outputs;
            let screens: Vec<_> = outputs
                .outputs()
                .filter_map(|output| {
                    let info = outputs.info(&output)?;
                    let (x, y) = info.logical_position?;
                    let (w, h) = info.logical_size?;
                    Some((output, (x, y, w, h), info.scale_factor.max(1)))
                })
                .collect();
            let inside = |(x, y, w, h): (i32, i32, i32, i32)| {
                point.0 >= f64::from(x) && point.0 < f64::from(x + w) && point.1 >= f64::from(y) && point.1 < f64::from(y + h)
            };
            let (output, (x, y, _, _), scale) = screens
                .iter()
                .find(|(_, rect, _)| inside(*rect))
                .or(screens.first())
                .cloned()
                .ok_or("no screen to draw on")?;

            let qh = self.queue.handle();
            let surface = self.compositor.create_surface(&qh);
            surface.set_buffer_scale(scale);
            let region = Region::new(&self.compositor)?;
            surface.set_input_region(Some(region.wl_region()));
            let layer = self.layer_shell.create_layer_surface(&qh, surface, Layer::Overlay,
                Some("gestos-path"), Some(&output));
            layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer.commit();
            self.state.configured = None;
            self.state.closed = false;
            let start = Instant::now();
            while self.state.configured.is_none() && !self.state.closed && start.elapsed() < CONFIGURE_TIMEOUT {
                self.queue.blocking_dispatch(&mut self.state)?;
            }
            let (points_wide, points_high) = self.state.configured.ok_or("the desktop didn't show the path")?;
            let (width, height) = (points_wide as i32 * scale, points_high as i32 * scale);

            let size = (width * height * 4) as usize;
            let mut pool = SlotPool::new(size * 2, &self.state.shm)?;
            let mut buffer = || -> Result<Buffer, Error> {
                let (buffer, canvas) = pool.create_buffer(width, height, width * 4, wl_shm::Format::Argb8888)?;
                canvas.fill(0);
                Ok(buffer)
            };
            let buffers = [buffer()?, buffer()?];
            Ok(PathLayer {
                layer,
                pool,
                buffers,
                missing: [Vec::new(), Vec::new()],
                pixels: vec![0; (width * height) as usize],
                width,
                height,
                scale,
                origin: (x, y),
                drawn: None,
            })
        }

        /// Shows what's drawn so far, in a buffer the desktop isn't showing. Nothing waits for one.
        pub fn present(&mut self, layer: &mut PathLayer) -> Result<(), Error> {
            self.pump()?;
            layer.present(1.0, false);
            self.queue.flush()?;
            Ok(())
        }

        /// Fades the path, and takes its layer away.
        pub fn fade(&mut self, mut layer: PathLayer, fade: Duration) -> Result<(), Error> {
            let start = Instant::now();
            while start.elapsed() < fade && !self.state.closed {
                std::thread::sleep(super::FRAME);
                self.pump()?;
                let left = 1.0 - start.elapsed().as_secs_f64() / fade.as_secs_f64();
                layer.present(left.max(0.0), true);
                self.queue.flush()?;
            }
            drop(layer);
            self.queue.roundtrip(&mut self.state)?;
            Ok(())
        }

        /// Handles what the desktop has said, as which buffers it's done with, without waiting.
        fn pump(&mut self) -> Result<(), Error> {
            self.queue.flush()?;
            if let Some(guard) = self.queue.prepare_read() {
                let fd = guard.connection_fd();
                let mut fds = [nix::poll::PollFd::new(&fd, nix::poll::PollFlags::POLLIN)];
                if nix::poll::poll(&mut fds, 0)? > 0 {
                    guard.read()?;
                }
            }
            self.queue.dispatch_pending(&mut self.state)?;
            Ok(())
        }
    }

    impl PathLayer {
        /// Where a point on the desktop is in the layer's pixels.
        pub fn to_pixels(&self, (x, y): (f64, f64)) -> (i32, i32) {
            let scale = f64::from(self.scale);
            (((x - f64::from(self.origin.0)) * scale).round() as i32, ((y - f64::from(self.origin.1)) * scale).round() as i32)
        }

        pub fn draw(&mut self, segment: &Segment) {
            let Some(rect) = bounds(segment, self.width, self.height) else { return };
            let (left, top, w, h) = rect;
            for y in top..top + h {
                for x in left..left + w {
                    let pixel = &mut self.pixels[(y * self.width + x) as usize];
                    let (cover, along) = coverage(segment, x, y);
                    // The most a pixel was covered, so where pieces overlap isn't darker.
                    if cover > 0.0 && (cover * 255.0).round() as u32 > *pixel >> 24 {
                        *pixel = premultiplied(segment, cover, along);
                    }
                }
            }
            for missing in &mut self.missing {
                missing.push(rect);
            }
            self.drawn = Some(match self.drawn {
                Some(drawn) => union(drawn, rect),
                None => rect,
            });
        }

        /// Puts what a free buffer lacks in it, faded to `opacity`, and shows it. When fading,
        /// all that's drawn is put in it again.
        fn present(&mut self, opacity: f64, fading: bool) {
            let Some(index) = (0..2).find(|&i| self.pool.canvas(&self.buffers[i]).is_some()) else {
                return;
            };
            if fading {
                self.missing[index] = self.drawn.into_iter().collect();
            }
            if self.missing[index].is_empty() {
                return;
            }
            let factor = (opacity.clamp(0.0, 1.0) * 256.0) as u32;
            let canvas = self.pool.canvas(&self.buffers[index]).expect("it was free");
            for &(left, top, w, h) in &self.missing[index] {
                for y in top..top + h {
                    let row = (y * self.width) as usize;
                    for x in left as usize..(left + w) as usize {
                        let pixel = fade(self.pixels[row + x], factor);
                        canvas[(row + x) * 4..(row + x) * 4 + 4].copy_from_slice(&pixel.to_le_bytes());
                    }
                }
            }
            let surface = self.layer.wl_surface();
            for &(left, top, w, h) in &self.missing[index] {
                surface.damage_buffer(left, top, w, h);
            }
            self.missing[index].clear();
            if self.buffers[index].attach_to(surface).is_ok() {
                self.layer.commit();
            }
        }
    }

    /// A premultiplied pixel at `factor` 256ths of its opacity.
    fn fade(pixel: u32, factor: u32) -> u32 {
        if factor >= 256 {
            return pixel;
        }
        let channel = |shift: u32| ((pixel >> shift & 0xff) * factor / 256) << shift;
        channel(24) | channel(16) | channel(8) | channel(0)
    }

    fn union(a: Rect, b: Rect) -> Rect {
        let (left, top) = (a.0.min(b.0), a.1.min(b.1));
        let (right, bottom) = ((a.0 + a.2).max(b.0 + b.2), (a.1 + a.3).max(b.1 + b.3));
        (left, top, right - left, bottom - top)
    }

    impl CompositorHandler for State {
        fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
        fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
        fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
        fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
        fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    }

    impl OutputHandler for State {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.outputs
        }
        fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
        fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
        fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    }

    impl LayerShellHandler for State {
        fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
            self.closed = true;
        }

        fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
            self.configured = Some(configure.new_size);
        }
    }

    impl ShmHandler for State {
        fn shm_state(&mut self) -> &mut Shm {
            &mut self.shm
        }
    }

    impl ProvidesRegistryState for State {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry
        }
        registry_handlers![OutputState];
    }

    delegate_registry!(State);
    delegate_dispatch2!(State);

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn fading_lowers_every_channel_alike() {
            assert_eq!(fade(0xff80_4020, 256), 0xff80_4020);
            assert_eq!(fade(0xff80_4020, 128), 0x7f40_2010);
            assert_eq!(fade(0xff80_4020, 0), 0);
        }

        #[test]
        fn the_drawn_part_grows_to_hold_each_piece() {
            assert_eq!(union((10, 10, 5, 5), (0, 12, 4, 10)), (0, 10, 15, 12));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run in a KWin in a window of its own, as `dbus-run-session -- sh -c 'kwin_wayland --socket
    /// gestos-test & sleep 3; WAYLAND_DISPLAY=gestos-test HOLD_MS=8000 cargo test -- --ignored
    /// kde_trail'`. For `HOLD_MS`, the path follows the pointer in KWin's window, and then fades.
    #[test]
    #[ignore = "needs KWin"]
    fn a_path_follows_the_pointer_in_a_real_kwin() {
        let (events, received) = std::sync::mpsc::channel();
        let conn = connect_bus(&events).unwrap();
        let name = "gestos-pointer-test";
        let reply_to = conn.unique_name().unwrap().to_string();
        let hold = std::env::var("HOLD_MS").ok().and_then(|ms| ms.parse().ok()).unwrap_or(1000);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(hold));
            let _ = events.send(Event::Ended);
        });
        let mut screen = wayland::Screen::connect().unwrap();
        kwin::load_script(&conn, name, &script(&reply_to)).unwrap();
        let drawn = draw_gesture(&mut screen, &received);
        kwin::unload_script(&conn, name);
        drawn.unwrap();
    }

    #[test]
    fn the_script_reports_to_this_connection() {
        let script = script(":1.42");
        assert!(script.contains("callDBus(\":1.42\", \"/io/github/deters/Gestos/Pointer\""));
        assert!(script.contains("workspace.cursorPosChanged.connect(pointer)"));
        assert!(script.contains("workspace.activeWindow || workspace.activeClient"));
    }
}
