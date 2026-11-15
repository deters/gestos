//! A small dark panel at the bottom of the screen the pointer is on, saying what a gesture did,
//! that it couldn't and why, or that it wasn't one of the user's. It goes away by itself, and takes
//! no clicks. GNOME's extension and KDE's on-screen display show it there; on other X11 desktops,
//! Gestos draws it itself, as the extension does.

use pangocairo::pango;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// How long it shows: longer for a failure, which there's more to read in.
const SHOWN_TIME: Duration = Duration::from_millis(1200);
const FAILED_SHOWN_TIME: Duration = Duration::from_millis(3500);
/// How long it takes to fade, with a compositor.
const FADE_TIME: Duration = Duration::from_millis(200);
/// How far above the bottom of the screen it is, at 96 DPI.
const BOTTOM_MARGIN: f64 = 72.0;
/// The panel's look at 96 DPI, as the extension's.
const PADDING_X: f64 = 18.0;
const PADDING_Y: f64 = 10.0;
const RADIUS: f64 = 14.0;
/// Wider text wraps, so a long reason stays readable.
const MAX_TEXT_WIDTH: f64 = 560.0;
const FONT: &str = "Sans Bold 11";
const BACKGROUND: [f64; 4] = [0.0, 0.0, 0.0, 0.72];
const FAILED_BACKGROUND: [f64; 4] = [0.47, 0.08, 0.08, 0.85];

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub text: String,
    pub failed: bool,
}

type Error = Box<dyn std::error::Error>;

/// Where the panel is shown.
trait Screen {
    /// The scale of the screen against 96 DPI, as whole or fractional.
    fn scale(&mut self) -> f64;
    /// Shows the panel, in place of the one shown, if one is.
    fn show(&mut self, image: cairo::ImageSurface) -> Result<(), Error>;
    /// Fades the panel shown, where it can, and takes it away.
    fn hide(&mut self, fade: Duration) -> Result<(), Error>;
}

/// Starts showing messages on their own thread, on X11 desktops whose own panel doesn't: all but
/// GNOME and KDE.
pub fn spawn() -> Option<Sender<Message>> {
    let desktop = crate::config::get_environment_suffix();
    if !crate::active_app::is_x11_session() || matches!(desktop, Some("gnome" | "kde")) {
        return None;
    }
    let connect: fn() -> Result<Box<dyn Screen>, Error> = || Ok(Box::new(x11::Screen::connect()?));
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || run(&receiver, connect));
    Some(sender)
}

fn run(messages: &Receiver<Message>, connect: fn() -> Result<Box<dyn Screen>, Error>) {
    let mut screen: Option<Box<dyn Screen>> = None;
    let mut warned = false;
    while let Ok(message) = messages.recv() {
        if screen.is_none() {
            match connect() {
                Ok(connected) => screen = Some(connected),
                Err(e) => {
                    if !warned {
                        eprintln!("gestosd: Can't show what gestures did: {e}");
                        warned = true;
                    }
                    continue;
                }
            }
        }
        // A connection that broke, as when the display restarted, is made again for the next one.
        if let Err(e) = show(screen.as_deref_mut().unwrap(), message, messages) {
            eprintln!("gestosd: Stopped showing what a gesture did: {e}");
            screen = None;
        }
    }
}

/// Shows a message, and each that comes while it shows in its place, until none has for a while.
fn show(screen: &mut dyn Screen, mut message: Message, messages: &Receiver<Message>) -> Result<(), Error> {
    let scale = screen.scale();
    screen.show(render(&message, scale)?)?;
    loop {
        let time = if message.failed { FAILED_SHOWN_TIME } else { SHOWN_TIME };
        match messages.recv_timeout(time) {
            Ok(next) => {
                message = next;
                screen.show(render(&message, scale)?)?;
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
    screen.hide(FADE_TIME)
}

/// The panel as an image, at the screen's scale.
pub fn render(message: &Message, scale: f64) -> Result<cairo::ImageSurface, cairo::Error> {
    let measure = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)?;
    let layout = text_layout(&cairo::Context::new(&measure)?, &message.text, scale);
    let (_, text) = layout.pixel_extents();
    let width = (f64::from(text.width()) + 2.0 * PADDING_X * scale).ceil() as i32;
    let height = (f64::from(text.height()) + 2.0 * PADDING_Y * scale).ceil() as i32;

    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)?;
    let cr = cairo::Context::new(&surface)?;
    rounded_rectangle(&cr, f64::from(width), f64::from(height), RADIUS * scale);
    let [r, g, b, a] = if message.failed { FAILED_BACKGROUND } else { BACKGROUND };
    cr.set_source_rgba(r, g, b, a);
    cr.fill()?;
    let layout = text_layout(&cr, &message.text, scale);
    cr.move_to(PADDING_X * scale - f64::from(text.x()), PADDING_Y * scale - f64::from(text.y()));
    cr.set_source_rgb(1.0, 1.0, 1.0);
    pangocairo::functions::show_layout(&cr, &layout);
    drop(cr);
    surface.flush();
    Ok(surface)
}

fn text_layout(cr: &cairo::Context, text: &str, scale: f64) -> pango::Layout {
    let layout = pangocairo::functions::create_layout(cr);
    pangocairo::functions::context_set_resolution(&layout.context(), 96.0 * scale);
    layout.context_changed();
    layout.set_font_description(Some(&pango::FontDescription::from_string(FONT)));
    layout.set_text(text);
    layout.set_width((MAX_TEXT_WIDTH * scale * f64::from(pango::SCALE)) as i32);
    layout.set_wrap(pango::WrapMode::WordChar);
    layout.set_alignment(pango::Alignment::Center);
    layout
}

fn rounded_rectangle(cr: &cairo::Context, width: f64, height: f64, radius: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    let radius = radius.min(width / 2.0).min(height / 2.0);
    cr.new_path();
    cr.arc(width - radius, radius, radius, -FRAC_PI_2, 0.0);
    cr.arc(width - radius, height - radius, radius, 0.0, FRAC_PI_2);
    cr.arc(radius, height - radius, radius, FRAC_PI_2, PI);
    cr.arc(radius, radius, radius, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}

/// The image's pixels as premultiplied ARGB, a row after another.
fn pixels(surface: &mut cairo::ImageSurface) -> Vec<u32> {
    let (width, stride) = (surface.width() as usize, surface.stride() as usize);
    let data = surface.data().expect("the panel's image is only used here");
    data.chunks(stride)
        .flat_map(|row| row[..width * 4].chunks(4).map(|px| u32::from_ne_bytes(px.try_into().unwrap())))
        .collect()
}

/// Each row's runs of pixels at least half covered, as where they start and how long they are:
/// the panel's shape, on a screen without a compositor.
fn opaque_runs(pixels: &[u32], width: usize) -> Vec<(i32, i32, i32)> {
    let mut runs = Vec::new();
    for (y, row) in pixels.chunks(width).enumerate() {
        let mut x = 0;
        while x < width {
            if row[x] >> 24 < 0x80 {
                x += 1;
                continue;
            }
            let start = x;
            while x < width && row[x] >> 24 >= 0x80 {
                x += 1;
            }
            runs.push((y as i32, start as i32, (x - start) as i32));
        }
    }
    runs
}

/// Where the panel goes: centered at the bottom of the monitor, kept inside it.
fn position(monitor: (i32, i32, i32, i32), size: (i32, i32), margin: f64) -> (i32, i32) {
    let (mx, my, mw, mh) = monitor;
    let x = mx + (mw - size.0) / 2;
    let y = my + mh - size.1 - margin.round() as i32;
    (x.max(mx), y.max(my))
}

mod x11 {
    use super::{opaque_runs, pixels, position, Error, BOTTOM_MARGIN};
    use std::time::{Duration, Instant};
    use x11rb::connection::Connection;
    use x11rb::protocol::randr::ConnectionExt as _;
    use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
    use x11rb::protocol::xproto::{
        AtomEnum, ClipOrdering, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _,
        CreateWindowAux, ImageFormat, ImageOrder, PropMode, Rectangle, StackMode, VisualClass,
        Visualid, Window, WindowClass,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    pub struct Screen {
        conn: RustConnection,
        number: usize,
        root: Window,
        shown: Option<Panel>,
    }

    impl super::Screen for Screen {
        fn scale(&mut self) -> f64 {
            Screen::scale(self)
        }

        fn show(&mut self, image: cairo::ImageSurface) -> Result<(), Error> {
            if let Some(shown) = self.shown.take() {
                self.destroy(shown);
            }
            let margin = BOTTOM_MARGIN * self.scale();
            self.shown = Some(self.open_panel(image, margin)?);
            Ok(())
        }

        fn hide(&mut self, fade: std::time::Duration) -> Result<(), Error> {
            let Some(shown) = self.shown.take() else { return Ok(()) };
            let faded = self.fade(&shown, fade);
            self.destroy(shown);
            faded
        }
    }

    impl Screen {
        pub fn connect() -> Result<Self, Error> {
            let (conn, number) = x11rb::connect(None)?;
            conn.shape_query_version()?.reply()?;
            let root = conn.setup().roots[number].root;
            Ok(Screen { conn, number, root, shown: None })
        }

        fn atom(&self, name: &str) -> Result<u32, Error> {
            Ok(self.conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
        }

        pub fn scale(&self) -> f64 {
            let resources = self
                .conn
                .get_property(false, self.root, AtomEnum::RESOURCE_MANAGER, AtomEnum::STRING, 0, 65536)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| String::from_utf8_lossy(&reply.value).into_owned())
                .unwrap_or_default();
            crate::trail::scale_from_resources(&resources)
        }

        /// The monitor the pointer is on, or the whole screen when RandR can't say.
        fn pointer_monitor(&self) -> Result<(i32, i32, i32, i32), Error> {
            let pointer = self.conn.query_pointer(self.root)?.reply()?;
            let (px, py) = (i32::from(pointer.root_x), i32::from(pointer.root_y));
            let monitors = self
                .conn
                .randr_get_monitors(self.root, true)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map(|reply| reply.monitors)
                .unwrap_or_default();
            let rect = |m: &x11rb::protocol::randr::MonitorInfo| {
                (i32::from(m.x), i32::from(m.y), i32::from(m.width), i32::from(m.height))
            };
            let inside = |(x, y, w, h): (i32, i32, i32, i32)| px >= x && px < x + w && py >= y && py < y + h;
            if let Some(monitor) = monitors.iter().map(rect).find(|m| inside(*m)) {
                return Ok(monitor);
            }
            let screen = &self.conn.setup().roots[self.number];
            Ok((0, 0, i32::from(screen.width_in_pixels), i32::from(screen.height_in_pixels)))
        }

        fn compositing(&self) -> Result<bool, Error> {
            let selection = self.atom(&format!("_NET_WM_CM_S{}", self.number))?;
            Ok(self.conn.get_selection_owner(selection)?.reply()?.owner != 0)
        }

        fn see_through_visual(&self) -> Option<Visualid> {
            self.conn.setup().roots[self.number]
                .allowed_depths
                .iter()
                .filter(|depth| depth.depth == 32)
                .flat_map(|depth| depth.visuals.iter())
                .find(|visual| visual.class == VisualClass::TRUE_COLOR)
                .map(|visual| visual.visual_id)
        }

        /// The panel, over everything else, that clicks go through: see-through with a
        /// compositor, or shaped as the panel without one.
        pub fn open_panel(&self, mut image: cairo::ImageSurface, margin: f64) -> Result<Panel, Error> {
            let conn = &self.conn;
            let screen = &conn.setup().roots[self.number];
            let (width, height) = (image.width(), image.height());
            let mut pixels = pixels(&mut image);
            let (x, y) = position(self.pointer_monitor()?, (width, height), margin);
            let window = conn.generate_id()?;
            let gc = conn.generate_id()?;
            let visual = if self.compositing()? { self.see_through_visual() } else { None };
            let aux = CreateWindowAux::new().override_redirect(1).border_pixel(0).background_pixel(0);
            let mut colormap = None;
            let depth = if let Some(visual) = visual {
                let id = conn.generate_id()?;
                conn.create_colormap(ColormapAlloc::NONE, id, self.root, visual)?;
                colormap = Some(id);
                conn.create_window(32, window, self.root, x as i16, y as i16, width as u16,
                    height as u16, 0, WindowClass::INPUT_OUTPUT, visual, &aux.colormap(id))?;
                32
            } else {
                conn.create_window(screen.root_depth, window, self.root, x as i16, y as i16,
                    width as u16, height as u16, 0, WindowClass::INPUT_OUTPUT, screen.root_visual, &aux)?;
                let rectangles: Vec<Rectangle> = opaque_runs(&pixels, width as usize)
                    .into_iter()
                    .map(|(y, x, w)| Rectangle { x: x as i16, y: y as i16, width: w as u16, height: 1 })
                    .collect();
                conn.shape_rectangles(SO::SET, SK::BOUNDING, ClipOrdering::UNSORTED, window, 0, 0,
                    &rectangles)?;
                // Nothing shows through, so it's the panel's color with no see-through left.
                for pixel in &mut pixels {
                    *pixel |= 0xff00_0000;
                }
                screen.root_depth
            };
            conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, window, 0, 0, &[])?;
            let kind = self.atom("_NET_WM_WINDOW_TYPE")?;
            let notification = self.atom("_NET_WM_WINDOW_TYPE_NOTIFICATION")?;
            conn.change_property32(PropMode::REPLACE, window, kind, AtomEnum::ATOM, &[notification])?;
            conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_NAME, AtomEnum::STRING,
                b"Gestos")?;
            conn.map_window(window)?;
            conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
            conn.create_gc(gc, window, &Default::default())?;

            let lsb_first = conn.setup().image_byte_order == ImageOrder::LSB_FIRST;
            let bytes: Vec<u8> = pixels
                .iter()
                .flat_map(|pixel| if lsb_first { pixel.to_le_bytes() } else { pixel.to_be_bytes() })
                .collect();
            // In pieces, as a request can only be so big.
            let row_bytes = width as usize * 4;
            let rows_per_piece = (60_000 / row_bytes).max(1);
            for (piece, data) in bytes.chunks(row_bytes * rows_per_piece).enumerate() {
                let rows = data.len() / row_bytes;
                conn.put_image(ImageFormat::Z_PIXMAP, window, gc, width as u16, rows as u16, 0,
                    (piece * rows_per_piece) as i16, 0, depth, data)?;
            }
            conn.flush()?;
            Ok(Panel { window, gc, colormap, sees_through: visual.is_some() })
        }

        /// Fades the panel, with a compositor.
        fn fade(&self, panel: &Panel, fade: Duration) -> Result<(), Error> {
            if !panel.sees_through {
                return Ok(());
            }
            let opacity = self.atom("_NET_WM_WINDOW_OPACITY")?;
            let start = Instant::now();
            while start.elapsed() < fade {
                let left = 1.0 - start.elapsed().as_secs_f64() / fade.as_secs_f64();
                let value = (left.clamp(0.0, 1.0) * f64::from(u32::MAX)) as u32;
                self.conn.change_property32(PropMode::REPLACE, panel.window, opacity, AtomEnum::CARDINAL,
                    &[value])?;
                self.conn.flush()?;
                std::thread::sleep(Duration::from_millis(16));
            }
            Ok(())
        }

        fn destroy(&self, panel: Panel) {
            let conn = &self.conn;
            let _ = conn.free_gc(panel.gc);
            let _ = conn.destroy_window(panel.window);
            if let Some(colormap) = panel.colormap {
                let _ = conn.free_colormap(colormap);
            }
            let _ = conn.flush();
        }
    }

    pub struct Panel {
        window: Window,
        gc: u32,
        colormap: Option<u32>,
        sees_through: bool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(text: &str, failed: bool) -> Message {
        Message { text: text.to_string(), failed }
    }

    #[test]
    fn the_panel_fits_its_text_and_grows_with_the_scale() {
        let short = render(&message("Back", false), 1.0).unwrap();
        let long = render(&message("Gesture not recognized", false), 1.0).unwrap();
        assert!(long.width() > short.width());
        assert!(short.height() > 2 * PADDING_Y as i32);
        let double = render(&message("Back", false), 2.0).unwrap();
        assert!(double.width() > short.width() * 3 / 2);
    }

    #[test]
    fn a_long_reason_wraps() {
        let reason = "The \u{201c}Gestos\u{201d} GNOME extension didn't answer, so this couldn't run, \
                      and nothing was done about it at all this time";
        let panel = render(&message(reason, true), 1.0).unwrap();
        assert!(f64::from(panel.width()) <= MAX_TEXT_WIDTH + 2.0 * PADDING_X + 1.0);
        let one_line = render(&message("Back", true), 1.0).unwrap();
        assert!(panel.height() > one_line.height());
    }

    #[test]
    fn a_failure_is_red_and_its_corners_are_round() {
        let mut panel = render(&message("xclip isn't installed", true), 1.0).unwrap();
        let width = panel.width() as usize;
        let pixels = pixels(&mut panel);
        assert_eq!(pixels[0] >> 24, 0, "a corner is cut off");
        let middle_left = pixels[(pixels.len() / width / 2) * width + 2];
        let (red, blue) = ((middle_left >> 16) & 0xff, middle_left & 0xff);
        assert!(red > blue);
        let runs = opaque_runs(&pixels, width);
        let first = runs.first().unwrap();
        assert_eq!(first.0, 0);
        assert!(first.1 > 0, "the top row starts after the round corner");
    }

    #[test]
    fn the_panel_is_centered_at_the_bottom_of_its_monitor() {
        assert_eq!(position((1920, 0, 1920, 1080), (200, 40), 72.0), (1920 + 860, 1080 - 40 - 72));
        // Wider than a small monitor, it starts at its edge.
        assert_eq!(position((0, 0, 100, 100), (200, 40), 72.0), (0, 0));
    }

    fn shows_a_panel(screen: &mut dyn Screen) {
        let image = render(&message("xclip isn't installed", true), screen.scale()).unwrap();
        screen.show(image).unwrap();
        let hold = std::env::var("PANEL_HOLD_MS").ok().and_then(|ms| ms.parse().ok()).unwrap_or(0);
        std::thread::sleep(Duration::from_millis(hold));
        screen.hide(FADE_TIME).unwrap();
    }

    /// Run with an X server, as `DISPLAY=:1 cargo test -- --ignored panel`.
    #[test]
    #[ignore = "needs an X server"]
    fn a_panel_is_shown_on_a_real_x_server() {
        let mut screen = x11::Screen::connect().unwrap();
        shows_a_panel(&mut screen);
    }
}
