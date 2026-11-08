//! The area a gesture is drawn in: as wide as it's given, and always as tall as a fixed share of
//! that, so that a taller window leaves its room to what's below. Its parents can measure it for
//! more width than they give it, so its height stops growing at the widest it's shown.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk4 as gtk;

// How much wider than tall it is, and the smallest and largest width its height follows.
const RATIO: f64 = 2.0;
const MIN_WIDTH: i32 = 300;
const MAX_WIDTH: i32 = 500;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct GestureCanvas;

    #[glib::object_subclass]
    impl ObjectSubclass for GestureCanvas {
        const NAME: &'static str = "GestosGestureCanvas";
        type Type = super::GestureCanvas;
        type ParentType = gtk::DrawingArea;
    }

    impl ObjectImpl for GestureCanvas {}

    impl WidgetImpl for GestureCanvas {
        // Its height depends on its width, so GTK asks for the width first.
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => (MIN_WIDTH, MIN_WIDTH, -1, -1),
                _ => {
                    let width = if for_size < 0 { MIN_WIDTH } else { for_size.clamp(MIN_WIDTH, MAX_WIDTH) };
                    let height = (f64::from(width) / RATIO).round() as i32;
                    (height, height, -1, -1)
                }
            }
        }
    }

    impl DrawingAreaImpl for GestureCanvas {}
}

glib::wrapper! {
    pub struct GestureCanvas(ObjectSubclass<imp::GestureCanvas>)
        @extends gtk::DrawingArea, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl GestureCanvas {
    pub fn new() -> Self {
        let canvas: Self = glib::Object::new();
        canvas.set_hexpand(true);
        canvas
    }
}

impl Default for GestureCanvas {
    fn default() -> Self {
        Self::new()
    }
}
