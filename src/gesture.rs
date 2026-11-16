use crate::protractor::{path_length, Point2D};

const MIN_GESTURE_POINTS: usize = 5;

/// Turns pointer input into gesture paths. It only tracks state, so the daemon stays
/// responsible for reading devices and for acting on the results.
pub struct GestureRecognizer {
    sample_distance_squared: f64,
    min_path_length: f64,
    drawing: bool,
    moved: bool,
    x: f64,
    y: f64,
    points: Vec<Point2D>,
}

impl GestureRecognizer {
    pub fn new(sample_distance: f64, min_path_length: f64) -> Self {
        GestureRecognizer {
            sample_distance_squared: sample_distance * sample_distance,
            min_path_length,
            drawing: false,
            moved: false,
            x: 0.0,
            y: 0.0,
            points: Vec::new(),
        }
    }

    pub fn is_drawing(&self) -> bool {
        self.drawing
    }

    pub fn move_by(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        self.moved = true;
    }

    pub fn set_x(&mut self, x: f64) {
        self.x = x;
        self.moved = true;
    }

    pub fn set_y(&mut self, y: f64) {
        self.y = y;
        self.moved = true;
    }

    /// Starts a path at the current position, which is returned.
    pub fn press(&mut self) -> Point2D {
        let start = self.position();
        self.drawing = true;
        self.points.clear();
        self.points.push(start);
        start
    }

    /// Ends the current frame, returning a new path point if the pointer moved far enough.
    pub fn sync(&mut self) -> Option<Point2D> {
        let moved = std::mem::take(&mut self.moved);
        if !moved || !self.drawing {
            return None;
        }
        let position = self.position();
        let far_enough = self.points.last().is_none_or(|last| {
            let dx = position.x - last.x;
            let dy = position.y - last.y;
            dx * dx + dy * dy >= self.sample_distance_squared
        });
        if far_enough {
            self.points.push(position);
            Some(position)
        } else {
            None
        }
    }

    /// Ends the path. `None` means it was too short to be a gesture, so it counts as a click.
    pub fn release(&mut self) -> Option<Vec<Point2D>> {
        self.drawing = false;
        if std::mem::take(&mut self.moved) {
            self.points.push(self.position());
        }
        let points = std::mem::take(&mut self.points);
        let long_enough = points.len() >= MIN_GESTURE_POINTS
            && path_length(&points) >= self.min_path_length;
        long_enough.then_some(points)
    }

    fn position(&self) -> Point2D {
        Point2D {
            x: self.x,
            y: self.y,
        }
    }
}

/// What a held middle button is doing: nothing yet, or moving a window once it has moved far
/// enough.
/// Ordered so the most that happened in one input frame is the greatest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WheelDragStep {
    Nothing,
    Moved,
    Started,
}

/// Tells a middle button that drags or scrolls from one that clicks. It's only a click once it's
/// released without having done either, so the press is held back until then.
pub struct WheelDrag {
    min_distance_squared: f64,
    held: bool,
    dragging: bool,
    scrolled: bool,
    dx: f64,
    dy: f64,
}

impl WheelDrag {
    pub fn new(min_distance: f64) -> Self {
        WheelDrag {
            min_distance_squared: min_distance * min_distance,
            held: false,
            dragging: false,
            scrolled: false,
            dx: 0.0,
            dy: 0.0,
        }
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    pub fn press(&mut self) {
        self.held = true;
        self.dragging = false;
        self.scrolled = false;
        self.dx = 0.0;
        self.dy = 0.0;
    }

    pub fn move_by(&mut self, dx: f64, dy: f64) -> WheelDragStep {
        if !self.held {
            return WheelDragStep::Nothing;
        }
        if self.dragging {
            return WheelDragStep::Moved;
        }
        self.dx += dx;
        self.dy += dy;
        if self.dx * self.dx + self.dy * self.dy < self.min_distance_squared {
            return WheelDragStep::Nothing;
        }
        self.dragging = true;
        WheelDragStep::Started
    }

    /// Whether a scroll is while it's held, which makes it no click.
    pub fn scroll(&mut self) -> bool {
        self.scrolled |= self.held;
        self.held
    }

    /// Whether it dragged or scrolled. One that did neither was a click.
    pub fn release(&mut self) -> bool {
        self.held = false;
        std::mem::take(&mut self.dragging) | std::mem::take(&mut self.scrolled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_press_without_movement_is_a_click() {
        let mut drag = WheelDrag::new(15.0);
        drag.press();
        assert_eq!(drag.move_by(3.0, 4.0), WheelDragStep::Nothing);
        assert!(!drag.release());
    }

    #[test]
    fn a_wheel_drag_starts_once_far_enough() {
        let mut drag = WheelDrag::new(15.0);
        drag.press();
        assert_eq!(drag.move_by(0.0, -10.0), WheelDragStep::Nothing);
        assert_eq!(drag.move_by(0.0, -10.0), WheelDragStep::Started);
        assert_eq!(drag.move_by(0.0, 1.0), WheelDragStep::Moved);
        assert!(drag.release());
        assert_eq!(drag.move_by(0.0, 50.0), WheelDragStep::Nothing);
    }

    #[test]
    fn a_wheel_press_that_scrolled_is_no_click() {
        let mut drag = WheelDrag::new(15.0);
        assert!(!drag.scroll());
        drag.press();
        assert!(drag.scroll());
        assert!(drag.release());
        drag.press();
        assert!(!drag.release());
    }

    #[test]
    fn each_wheel_press_starts_from_where_it_is() {
        let mut drag = WheelDrag::new(15.0);
        drag.press();
        drag.move_by(0.0, 10.0);
        drag.release();
        drag.press();
        assert_eq!(drag.move_by(0.0, 10.0), WheelDragStep::Nothing);
    }

    fn recognizer() -> GestureRecognizer {
        GestureRecognizer::new(3.0, 15.0)
    }

    fn step(r: &mut GestureRecognizer, dx: f64, dy: f64) -> Option<Point2D> {
        r.move_by(dx, dy);
        r.sync()
    }

    #[test]
    fn a_press_without_movement_is_a_click() {
        let mut r = recognizer();
        r.press();
        assert_eq!(r.release(), None);
    }

    #[test]
    fn too_few_points_are_a_click() {
        let mut r = recognizer();
        r.press();
        step(&mut r, 30.0, 0.0);
        step(&mut r, 30.0, 0.0);
        assert_eq!(r.release(), None);
    }

    #[test]
    fn many_points_over_a_short_distance_are_a_click() {
        let mut r = GestureRecognizer::new(1.0, 15.0);
        r.press();
        for _ in 0..6 {
            step(&mut r, 1.0, 0.0);
        }
        assert_eq!(r.release(), None);
    }

    #[test]
    fn a_long_drag_returns_its_path() {
        let mut r = recognizer();
        r.press();
        for _ in 0..5 {
            assert!(step(&mut r, 10.0, 0.0).is_some());
        }
        let points = r.release().unwrap();
        assert_eq!(points.len(), 6);
        assert_eq!(points[5], Point2D { x: 50.0, y: 0.0 });
    }

    #[test]
    fn movement_below_the_sample_distance_is_not_recorded_until_it_adds_up() {
        let mut r = recognizer();
        r.press();
        assert_eq!(step(&mut r, 1.0, 0.0), None);
        assert_eq!(step(&mut r, 1.0, 0.0), None);
        assert_eq!(step(&mut r, 1.0, 0.0), Some(Point2D { x: 3.0, y: 0.0 }));
    }

    #[test]
    fn movement_outside_a_press_is_tracked_but_not_recorded() {
        let mut r = recognizer();
        assert_eq!(step(&mut r, 50.0, 7.0), None);
        assert_eq!(r.press(), Point2D { x: 50.0, y: 7.0 });
    }

    #[test]
    fn release_includes_movement_from_an_unfinished_frame() {
        let mut r = recognizer();
        r.press();
        for _ in 0..4 {
            step(&mut r, 10.0, 0.0);
        }
        r.move_by(1.0, 0.0);
        let points = r.release().unwrap();
        assert_eq!(points.last(), Some(&Point2D { x: 41.0, y: 0.0 }));
    }

    #[test]
    fn absolute_positions_replace_the_current_position() {
        let mut r = recognizer();
        r.set_x(100.0);
        r.set_y(200.0);
        r.sync();
        assert_eq!(r.press(), Point2D { x: 100.0, y: 200.0 });
    }
}
