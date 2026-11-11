//! The path of a gesture, drawn over everything on screen while it's drawn, on X11. GNOME's
//! extension draws it there; elsewhere on X11, Gestos draws it itself, in a window of its own that
//! clicks go through. The daemon says when a gesture starts and ends; in between, the path follows
//! the pointer, read once a frame, so its tip stays right under it.
//!
//! With a compositor, the window is see-through, and the line is smooth and fades once the gesture
//! is done. Without one, nothing can be seen through a window, so the window takes the shape of the
//! line instead, and it goes away at once.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// The line's width at 96 DPI, as on GNOME.
const LINE_WIDTH: f64 = 5.0;
/// How often the pointer is read while a gesture is drawn.
const FRAME: Duration = Duration::from_millis(16);
/// How long the path takes to fade, with a compositor.
const FADE_TIME: Duration = Duration::from_millis(300);
/// How far along the path its color goes from violet to pink, at 96 DPI.
const GRADIENT_LENGTH: f64 = 800.0;
/// The same violet to pink as in Gestos's own drawings.
const START_COLOR: [f64; 3] = [0.49, 0.27, 0.90];
const END_COLOR: [f64; 3] = [0.98, 0.72, 0.80];

pub enum TrailEvent {
    Started,
    Ended,
}

/// Starts drawing paths on their own thread, on X11 desktops other than GNOME, which draws them.
/// `show` is asked as each gesture starts, so the setting can change while this runs.
pub fn spawn(show: impl Fn() -> bool + Send + 'static) -> Option<Sender<TrailEvent>> {
    let gnome = crate::config::get_environment_suffix() == Some("gnome");
    if !crate::active_app::is_x11_session() || gnome {
        return None;
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || run(&receiver, show));
    Some(sender)
}

fn run(events: &Receiver<TrailEvent>, show: impl Fn() -> bool) {
    let mut screen: Option<x11::Screen> = None;
    let mut warned = false;
    while let Ok(event) = events.recv() {
        if !matches!(event, TrailEvent::Started) || !show() {
            continue;
        }
        if screen.is_none() {
            match x11::Screen::connect() {
                Ok(connected) => screen = Some(connected),
                Err(e) => {
                    if !warned {
                        eprintln!("gestosd: Can't draw gesture paths: {e}");
                        warned = true;
                    }
                    continue;
                }
            }
        }
        // A connection that broke, as when X restarted, is made again for the next gesture.
        if let Err(e) = draw_gesture(screen.as_ref().unwrap(), events) {
            eprintln!("gestosd: Stopped drawing a gesture's path: {e}");
            screen = None;
        }
    }
}

/// Draws one gesture's path until it ends, and takes it away.
fn draw_gesture(screen: &x11::Screen, events: &Receiver<TrailEvent>) -> Result<(), x11::Error> {
    // Not over a game or a video, which a window on top would make stutter.
    if screen.fullscreen_window_active()? {
        wait_for_end(events);
        return Ok(());
    }
    let mut trail = screen.open_trail()?;
    let mut path = Path::new(screen.scale());
    let mut last = screen.pointer()?;
    path.points.push(last);
    loop {
        match events.recv_timeout(FRAME) {
            Ok(TrailEvent::Ended) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(TrailEvent::Started) | Err(RecvTimeoutError::Timeout) => {}
        }
        let point = screen.pointer()?;
        if point != last {
            trail.draw(&path.segment_to(point))?;
            last = point;
        }
    }
    // Where the pointer was let go, which may be after the last frame.
    let point = screen.pointer()?;
    if point != last {
        trail.draw(&path.segment_to(point))?;
    }
    trail.close(FADE_TIME)
}

fn wait_for_end(events: &Receiver<TrailEvent>) {
    while let Ok(event) = events.recv() {
        if matches!(event, TrailEvent::Ended) {
            return;
        }
    }
}

/// The points so far, and how far along the path the last one is.
struct Path {
    points: Vec<(i32, i32)>,
    length: f64,
    radius: f64,
    gradient_length: f64,
}

/// A piece of the path to draw: from one point to the next, with the color at each end.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub radius: f64,
    pub from_color: [f64; 3],
    pub to_color: [f64; 3],
}

impl Path {
    fn new(scale: f64) -> Self {
        Path {
            points: Vec::new(),
            length: 0.0,
            radius: LINE_WIDTH * scale / 2.0,
            gradient_length: GRADIENT_LENGTH * scale,
        }
    }

    fn segment_to(&mut self, point: (i32, i32)) -> Segment {
        let from = *self.points.last().unwrap_or(&point);
        let start = self.length;
        self.length += f64::from(point.0 - from.0).hypot(f64::from(point.1 - from.1));
        self.points.push(point);
        Segment {
            from: (f64::from(from.0), f64::from(from.1)),
            to: (f64::from(point.0), f64::from(point.1)),
            radius: self.radius,
            from_color: color_at(start / self.gradient_length),
            to_color: color_at(self.length / self.gradient_length),
        }
    }
}

/// The path's color a part of the way along its gradient, staying pink past its end.
pub fn color_at(part: f64) -> [f64; 3] {
    let part = part.clamp(0.0, 1.0);
    std::array::from_fn(|i| START_COLOR[i] + (END_COLOR[i] - START_COLOR[i]) * part)
}

/// How much of a pixel, centered at `x + 0.5, y + 0.5`, the segment covers, from 0 to 1, with
/// its edges smoothed over about a pixel. Its ends are round, so pieces join smoothly.
/// Also returns how far along the segment the pixel is, from 0 to 1, for its color.
pub fn coverage(segment: &Segment, x: i32, y: i32) -> (f64, f64) {
    let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
    let (ax, ay) = segment.from;
    let (dx, dy) = (segment.to.0 - ax, segment.to.1 - ay);
    let squared = dx * dx + dy * dy;
    let along = if squared == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / squared).clamp(0.0, 1.0)
    };
    let distance = (px - (ax + along * dx)).hypot(py - (ay + along * dy));
    ((segment.radius + 0.5 - distance).clamp(0.0, 1.0), along)
}

/// The pixels a segment touches, as a rectangle: x, y, width and height, inside the screen.
pub fn bounds(segment: &Segment, width: i32, height: i32) -> Option<(i32, i32, i32, i32)> {
    let reach = segment.radius + 1.0;
    let left = ((segment.from.0.min(segment.to.0) - reach).floor() as i32).max(0);
    let top = ((segment.from.1.min(segment.to.1) - reach).floor() as i32).max(0);
    let right = ((segment.from.0.max(segment.to.0) + reach).ceil() as i32).min(width);
    let bottom = ((segment.from.1.max(segment.to.1) + reach).ceil() as i32).min(height);
    (right > left && bottom > top).then_some((left, top, right - left, bottom - top))
}

/// Each row of pixels the segment covers at least half of, as where it starts and how long it is.
/// The segment with round ends is convex, so each row is a single run.
pub fn rows(segment: &Segment, width: i32, height: i32) -> Vec<(i32, i32, i32)> {
    let Some((left, top, w, h)) = bounds(segment, width, height) else { return Vec::new() };
    let mut rows = Vec::new();
    for y in top..top + h {
        let covered = |x: i32| coverage(segment, x, y).0 >= 0.5;
        let Some(start) = (left..left + w).find(|&x| covered(x)) else { continue };
        let end = (start..left + w).rev().find(|&x| covered(x)).unwrap_or(start);
        rows.push((y, start, end - start + 1));
    }
    rows
}

/// The scale of the screen against 96 DPI, from Xft.dpi in the X resources, as desktops set it.
pub fn scale_from_resources(resources: &str) -> f64 {
    resources
        .lines()
        .filter_map(|line| line.strip_prefix("Xft.dpi:"))
        .find_map(|dpi| dpi.trim().parse::<f64>().ok())
        .filter(|dpi| *dpi > 0.0)
        .map_or(1.0, |dpi| dpi / 96.0)
}

mod x11 {
    use super::{bounds, coverage, rows, scale_from_resources, Segment};
    use std::time::{Duration, Instant};
    use x11rb::connection::Connection;
    use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
    use x11rb::protocol::xproto::{
        AtomEnum, ClipOrdering, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _,
        CreateWindowAux, ImageFormat, ImageOrder, PropMode, Rectangle, StackMode, VisualClass,
        Visualid, Window, WindowClass,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    pub type Error = Box<dyn std::error::Error>;

    pub struct Screen {
        conn: RustConnection,
        number: usize,
        root: Window,
    }

    impl Screen {
        pub fn connect() -> Result<Self, Error> {
            let (conn, number) = x11rb::connect(None)?;
            conn.shape_query_version()?.reply()?;
            let root = conn.setup().roots[number].root;
            Ok(Screen { conn, number, root })
        }

        fn atom(&self, name: &str) -> Result<u32, Error> {
            Ok(self.conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
        }

        pub fn pointer(&self) -> Result<(i32, i32), Error> {
            let reply = self.conn.query_pointer(self.root)?.reply()?;
            Ok((i32::from(reply.root_x), i32::from(reply.root_y)))
        }

        pub fn scale(&self) -> f64 {
            let resources = self
                .conn
                .get_property(false, self.root, AtomEnum::RESOURCE_MANAGER, AtomEnum::STRING, 0, 65536)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| String::from_utf8_lossy(&reply.value).into_owned())
                .unwrap_or_default();
            scale_from_resources(&resources)
        }

        /// Whether the window in use covers the whole screen, as a game or a video does.
        pub fn fullscreen_window_active(&self) -> Result<bool, Error> {
            let active = self
                .conn
                .get_property(false, self.root, self.atom("_NET_ACTIVE_WINDOW")?, AtomEnum::WINDOW, 0, 1)?
                .reply()?
                .value32()
                .and_then(|mut values| values.next())
                .filter(|window| *window != 0);
            let Some(window) = active else { return Ok(false) };
            let fullscreen = self.atom("_NET_WM_STATE_FULLSCREEN")?;
            let state = self
                .conn
                .get_property(false, window, self.atom("_NET_WM_STATE")?, AtomEnum::ATOM, 0, 64)?
                .reply()?;
            Ok(state.value32().is_some_and(|mut atoms| atoms.any(|atom| atom == fullscreen)))
        }

        fn compositing(&self) -> Result<bool, Error> {
            let selection = self.atom(&format!("_NET_WM_CM_S{}", self.number))?;
            Ok(self.conn.get_selection_owner(selection)?.reply()?.owner != 0)
        }

        /// A visual that can be seen through, with 32 bits for each pixel.
        fn see_through_visual(&self) -> Option<Visualid> {
            self.conn.setup().roots[self.number]
                .allowed_depths
                .iter()
                .filter(|depth| depth.depth == 32)
                .flat_map(|depth| depth.visuals.iter())
                .find(|visual| visual.class == VisualClass::TRUE_COLOR)
                .map(|visual| visual.visual_id)
        }

        /// A window over the whole screen, over everything else, that clicks go through. The path
        /// is drawn in it, see-through with a compositor, or shaped as the path without one.
        pub fn open_trail(&self) -> Result<Trail<'_>, Error> {
            let conn = &self.conn;
            let screen = &conn.setup().roots[self.number];
            let (width, height) = (screen.width_in_pixels, screen.height_in_pixels);
            let window = conn.generate_id()?;
            let visual = if self.compositing()? { self.see_through_visual() } else { None };
            let aux = CreateWindowAux::new().override_redirect(1).border_pixel(0);
            let mut colormap = None;
            let mut gc = None;
            if let Some(visual) = visual {
                let id = conn.generate_id()?;
                conn.create_colormap(ColormapAlloc::NONE, id, self.root, visual)?;
                colormap = Some(id);
                let aux = aux.background_pixel(0).colormap(id);
                conn.create_window(32, window, self.root, 0, 0, width, height, 0,
                    WindowClass::INPUT_OUTPUT, visual, &aux)?;
                let id = conn.generate_id()?;
                conn.create_gc(id, window, &Default::default())?;
                gc = Some(id);
            } else {
                let aux = aux.background_pixel(pixel_of(screen, super::START_COLOR));
                conn.create_window(screen.root_depth, window, self.root, 0, 0, width, height, 0,
                    WindowClass::INPUT_OUTPUT, screen.root_visual, &aux)?;
                // Nothing of it shows until the path is drawn.
                conn.shape_rectangles(SO::SET, SK::BOUNDING, ClipOrdering::UNSORTED, window, 0, 0, &[])?;
            }
            conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, window, 0, 0, &[])?;
            // As compositors take it: a drag's icon, which gets no shadow, and isn't faded in.
            let kind = self.atom("_NET_WM_WINDOW_TYPE")?;
            let dnd = self.atom("_NET_WM_WINDOW_TYPE_DND")?;
            conn.change_property32(PropMode::REPLACE, window, kind, AtomEnum::ATOM, &[dnd])?;
            conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_NAME, AtomEnum::STRING,
                b"Gestos gesture path")?;
            conn.map_window(window)?;
            conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
            conn.flush()?;
            let pixels = visual.map(|_| vec![0u32; usize::from(width) * usize::from(height)]);
            Ok(Trail {
                screen: self,
                window,
                colormap,
                gc,
                width: i32::from(width),
                height: i32::from(height),
                pixels,
                lsb_first: conn.setup().image_byte_order == ImageOrder::LSB_FIRST,
            })
        }
    }

    /// The pixel for a color on a screen without a compositor, from how its visual packs colors.
    fn pixel_of(screen: &x11rb::protocol::xproto::Screen, color: [f64; 3]) -> u32 {
        let visual = screen
            .allowed_depths
            .iter()
            .flat_map(|depth| depth.visuals.iter())
            .find(|visual| visual.visual_id == screen.root_visual);
        let Some(visual) = visual else { return screen.white_pixel };
        let channel = |mask: u32, value: f64| {
            if mask == 0 {
                return 0;
            }
            let shift = mask.trailing_zeros();
            let max = mask >> shift;
            (((value * f64::from(max)).round() as u32) << shift) & mask
        };
        channel(visual.red_mask, color[0])
            | channel(visual.green_mask, color[1])
            | channel(visual.blue_mask, color[2])
    }

    pub struct Trail<'a> {
        screen: &'a Screen,
        window: Window,
        colormap: Option<u32>,
        /// What the see-through window's picture is put in it with.
        gc: Option<u32>,
        width: i32,
        height: i32,
        /// The see-through window's picture, as premultiplied ARGB, with a compositor.
        pixels: Option<Vec<u32>>,
        lsb_first: bool,
    }

    impl Trail<'_> {
        pub fn draw(&mut self, segment: &Segment) -> Result<(), Error> {
            let conn = &self.screen.conn;
            match (&mut self.pixels, self.gc) {
                (Some(pixels), Some(gc)) => {
                    let Some((left, top, w, h)) = bounds(segment, self.width, self.height) else {
                        return Ok(());
                    };
                    let mut data = Vec::with_capacity((w * h * 4) as usize);
                    for y in top..top + h {
                        for x in left..left + w {
                            let pixel = &mut pixels[(y * self.width + x) as usize];
                            let (cover, along) = coverage(segment, x, y);
                            // The most a pixel was covered, so where pieces overlap isn't darker.
                            if cover > 0.0 && (cover * 255.0).round() as u32 > *pixel >> 24 {
                                *pixel = premultiplied(segment, cover, along);
                            }
                            data.extend_from_slice(&if self.lsb_first {
                                pixel.to_le_bytes()
                            } else {
                                pixel.to_be_bytes()
                            });
                        }
                    }
                    conn.put_image(ImageFormat::Z_PIXMAP, self.window, gc, w as u16, h as u16,
                        left as i16, top as i16, 0, 32, &data)?;
                }
                _ => {
                    let rectangles: Vec<Rectangle> = rows(segment, self.width, self.height)
                        .into_iter()
                        .map(|(y, x, w)| Rectangle { x: x as i16, y: y as i16, width: w as u16, height: 1 })
                        .collect();
                    conn.shape_rectangles(SO::UNION, SK::BOUNDING, ClipOrdering::UNSORTED, self.window,
                        0, 0, &rectangles)?;
                }
            }
            conn.flush()?;
            Ok(())
        }

        /// Fades the path, with a compositor that fades windows, and takes it away.
        pub fn close(self, fade: Duration) -> Result<(), Error> {
            let conn = &self.screen.conn;
            if self.pixels.is_some() {
                let opacity = self.screen.atom("_NET_WM_WINDOW_OPACITY")?;
                let start = Instant::now();
                while start.elapsed() < fade {
                    let left = 1.0 - start.elapsed().as_secs_f64() / fade.as_secs_f64();
                    let value = (left.clamp(0.0, 1.0) * f64::from(u32::MAX)) as u32;
                    conn.change_property32(PropMode::REPLACE, self.window, opacity, AtomEnum::CARDINAL,
                        &[value])?;
                    conn.flush()?;
                    std::thread::sleep(super::FRAME);
                }
            }
            Ok(())
        }
    }

    #[cfg(test)]
    impl Trail<'_> {
        /// The X errors so far, once the server has handled everything sent.
        pub fn errors(&self) -> Vec<String> {
            let conn = &self.screen.conn;
            conn.get_input_focus().unwrap().reply().unwrap();
            let mut errors = Vec::new();
            while let Some(event) = conn.poll_for_event().unwrap() {
                if let x11rb::protocol::Event::Error(error) = event {
                    errors.push(format!("{error:?}"));
                }
            }
            errors
        }

        pub fn sees_through(&self) -> bool {
            self.pixels.is_some()
        }

        /// A pixel of the trail's window, as the server has it.
        pub fn pixel(&self, x: i16, y: i16) -> u32 {
            let reply = self.screen.conn
                .get_image(ImageFormat::Z_PIXMAP, self.window, x, y, 1, 1, !0)
                .unwrap()
                .reply()
                .unwrap();
            let bytes: [u8; 4] = reply.data[..4].try_into().unwrap();
            if self.lsb_first { u32::from_le_bytes(bytes) } else { u32::from_be_bytes(bytes) }
        }

        /// How many rectangles make up the window's shape, without a compositor.
        pub fn shape_rectangles(&self) -> usize {
            self.screen.conn
                .shape_get_rectangles(self.window, SK::BOUNDING)
                .unwrap()
                .reply()
                .unwrap()
                .rectangles
                .len()
        }
    }

    impl Drop for Trail<'_> {
        fn drop(&mut self) {
            let conn = &self.screen.conn;
            if let Some(gc) = self.gc {
                let _ = conn.free_gc(gc);
            }
            let _ = conn.destroy_window(self.window);
            if let Some(colormap) = self.colormap {
                let _ = conn.free_colormap(colormap);
            }
            let _ = conn.flush();
        }
    }

    fn premultiplied(segment: &Segment, cover: f64, along: f64) -> u32 {
        let alpha = cover;
        let channel = |i: usize| {
            let value = segment.from_color[i] + (segment.to_color[i] - segment.from_color[i]) * along;
            (value * alpha * 255.0).round() as u32
        };
        ((alpha * 255.0).round() as u32) << 24 | channel(0) << 16 | channel(1) << 8 | channel(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(from: (f64, f64), to: (f64, f64), radius: f64) -> Segment {
        Segment { from, to, radius, from_color: START_COLOR, to_color: END_COLOR }
    }

    #[test]
    fn a_pixel_on_the_line_is_covered_and_one_away_from_it_is_not() {
        let line = segment((10.0, 10.0), (50.0, 10.0), 2.5);
        assert_eq!(coverage(&line, 30, 9).0, 1.0);
        assert_eq!(coverage(&line, 30, 20).0, 0.0);
        // Its ends are round, so a pixel just past an end is covered too.
        assert_eq!(coverage(&line, 51, 9).0, 1.0);
        assert_eq!(coverage(&line, 54, 9).0, 0.0);
    }

    #[test]
    fn a_pixel_is_as_far_along_as_it_is_on_the_line() {
        let line = segment((0.0, 0.0), (100.0, 0.0), 2.5);
        assert!((coverage(&line, 49, 0).1 - 0.5).abs() < 0.01);
        assert_eq!(coverage(&line, 150, 0).1, 1.0);
    }

    #[test]
    fn a_segment_touches_only_pixels_near_it_and_on_screen() {
        let line = segment((10.0, 10.0), (20.0, 15.0), 2.5);
        assert_eq!(bounds(&line, 1920, 1080), Some((6, 6, 18, 13)));
        let off = segment((-50.0, -50.0), (-40.0, -40.0), 2.5);
        assert_eq!(bounds(&off, 1920, 1080), None);
        let edge = segment((0.0, 0.0), (5.0, 0.0), 2.5);
        assert_eq!(bounds(&edge, 1920, 1080).map(|b| (b.0, b.1)), Some((0, 0)));
    }

    #[test]
    fn rows_cover_a_level_line_as_wide_as_it_is() {
        // Through the middle of a row of pixels, five pixels wide.
        let line = segment((10.0, 10.5), (30.0, 10.5), 2.5);
        let rows = rows(&line, 1920, 1080);
        assert_eq!(rows.len(), 5);
        let middle = rows.iter().find(|(y, _, _)| *y == 10).unwrap();
        assert_eq!((middle.1, middle.2), (7, 26));
    }

    #[test]
    fn a_point_alone_is_drawn_as_a_dot() {
        let dot = segment((10.0, 10.0), (10.0, 10.0), 2.5);
        assert!(!rows(&dot, 100, 100).is_empty());
        assert_eq!(coverage(&dot, 9, 9).0, 1.0);
    }

    #[test]
    fn the_color_goes_from_violet_to_pink_along_the_path() {
        assert_eq!(color_at(0.0), START_COLOR);
        assert_eq!(color_at(1.0), END_COLOR);
        assert_eq!(color_at(5.0), END_COLOR);
        let path = &mut Path::new(1.0);
        path.points.push((0, 0));
        let first = path.segment_to((400, 0));
        let second = path.segment_to((800, 0));
        assert_eq!(first.from_color, START_COLOR);
        assert_eq!(first.to_color, second.from_color);
        assert_eq!(second.to_color, END_COLOR);
    }

    /// Run with an X server, as `DISPLAY=:1 cargo test -- --ignored trail`, with and without a
    /// compositor.
    #[test]
    #[ignore = "needs an X server"]
    fn a_path_is_drawn_on_a_real_x_server() {
        let screen = x11::Screen::connect().unwrap();
        let mut trail = screen.open_trail().unwrap();
        let mut path = Path::new(1.0);
        path.points.push((100, 100));
        for point in [(200, 100), (300, 200), (300, 300)] {
            trail.draw(&path.segment_to(point)).unwrap();
        }
        assert_eq!(trail.errors(), Vec::<String>::new());
        if trail.sees_through() {
            assert_eq!(trail.pixel(150, 100) >> 24, 255, "on the line");
            assert_eq!(trail.pixel(150, 150), 0, "away from it");
        } else {
            assert!(trail.shape_rectangles() > 0);
        }
        let hold = std::env::var("TRAIL_HOLD_MS").ok().and_then(|ms| ms.parse().ok()).unwrap_or(0);
        std::thread::sleep(Duration::from_millis(hold));
        trail.close(FADE_TIME).unwrap();
    }

    #[test]
    fn the_scale_comes_from_xft_dpi() {
        assert_eq!(scale_from_resources("Xft.antialias:\t1\nXft.dpi:\t192\n"), 2.0);
        assert_eq!(scale_from_resources("Xft.dpi: 144"), 1.5);
        assert_eq!(scale_from_resources(""), 1.0);
        assert_eq!(scale_from_resources("Xft.dpi: none"), 1.0);
    }
}
