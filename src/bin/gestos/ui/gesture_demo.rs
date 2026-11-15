//! What the gesture editor shows before a gesture is drawn: a mouse drawing one, with the button
//! gestures are drawn with held down, over and over.

use gtk4 as gtk;
use gtk::cairo;

// How long one gesture takes, from the button going down to its trail fading, in seconds.
const LOOP: f64 = 3.2;
const PRESS: f64 = 0.4;
const MOVE_END: f64 = 1.9;
const RELEASE: f64 = 2.2;

// The color of a drawn gesture, where it starts.
const VIOLET: (f64, f64, f64) = (0.49, 0.27, 0.90);

/// The shapes drawn, one after another, as points from -1 to 1 around the drawing's center. They
/// go right and down, so the mouse, to the lower right of where it draws, never covers its trail.
const SHAPES: [&[(f64, f64)]; 2] = [
    &[(-0.6, -0.8), (-0.6, 0.6), (0.8, 0.6)],
    &[(-0.8, -0.6), (0.6, -0.6), (0.6, 0.8)],
];

/// Draws the demonstration as it is `seconds` after it started, in the foreground color `fg`.
pub fn draw(cr: &cairo::Context, width: f64, height: f64, seconds: f64, button: i32, fg: (f64, f64, f64)) {
    let shape = SHAPES[(seconds / LOOP) as usize % SHAPES.len()];
    let t = seconds % LOOP;
    let size = height * 0.3;
    // Up and left of the center by half the mouse, so the shape and the mouse are centered.
    let to_canvas = |(x, y): (f64, f64)| (width / 2.0 - 16.0 + x * size, height / 2.0 - 24.0 + y * size);

    let progress = ((t - PRESS) / (MOVE_END - PRESS)).clamp(0.0, 1.0);
    let trail = points_along(shape, ease(progress));
    let tip = to_canvas(*trail.last().unwrap());
    // It fades in where it starts, and its trail fades out once the button is up.
    let fade_in = (t / 0.3).min(1.0);
    let fade_out = if t > RELEASE { 1.0 - (t - RELEASE) / (LOOP - RELEASE) } else { 1.0 };

    if trail.len() > 1 && fade_out > 0.0 {
        let (sx, sy) = to_canvas(shape[0]);
        let (ex, ey) = to_canvas(*shape.last().unwrap());
        let gradient = cairo::LinearGradient::new(sx, sy, ex, ey);
        gradient.add_color_stop_rgba(0.0, VIOLET.0, VIOLET.1, VIOLET.2, 0.4 * fade_out);
        gradient.add_color_stop_rgba(1.0, 0.98, 0.72, 0.80, 0.4 * fade_out);
        cr.set_source(&gradient).ok();
        cr.set_line_width(4.0);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_join(cairo::LineJoin::Round);
        let (x, y) = to_canvas(trail[0]);
        cr.move_to(x, y);
        for point in &trail[1..] {
            let (x, y) = to_canvas(*point);
            cr.line_to(x, y);
        }
        cr.stroke().ok();
    }

    let pressed = (PRESS..RELEASE).contains(&t);
    let alpha = fade_in * fade_out.max(0.0);
    draw_mouse(cr, tip.0 + 8.0, tip.1 + 8.0, button, pressed, fg, alpha);
}

/// The shape's points up to `share` of its length, the last one where it has got to.
fn points_along(shape: &[(f64, f64)], share: f64) -> Vec<(f64, f64)> {
    let lengths: Vec<f64> = shape
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1))
        .collect();
    let mut left = lengths.iter().sum::<f64>() * share;
    let mut points = vec![shape[0]];
    for (pair, length) in shape.windows(2).zip(&lengths) {
        if left >= *length {
            points.push(pair[1]);
            left -= length;
        } else {
            let f = left / length;
            points.push((pair[0].0 + (pair[1].0 - pair[0].0) * f, pair[0].1 + (pair[1].1 - pair[0].1) * f));
            break;
        }
    }
    points
}

/// Slow at the start and end, as a hand moves.
fn ease(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// A mouse seen from above, its top left corner at `x`, `y`, with `button` lit while pressed:
/// the right one, the wheel, or one of the side buttons for Back and Forward.
fn draw_mouse(cr: &cairo::Context, x: f64, y: f64, button: i32, pressed: bool, fg: (f64, f64, f64), alpha: f64) {
    const W: f64 = 30.0;
    const H: f64 = 46.0;
    const BUTTONS: f64 = 18.0;
    let body = |cr: &cairo::Context| {
        let r = W / 2.0;
        cr.new_path();
        cr.arc(x + r, y + r, r, std::f64::consts::PI, 0.0);
        cr.line_to(x + W, y + H - r);
        cr.arc(x + r, y + H - r, r, 0.0, std::f64::consts::PI);
        cr.close_path();
    };
    let lit = |cr: &cairo::Context| {
        cr.set_source_rgba(VIOLET.0, VIOLET.1, VIOLET.2, alpha);
        cr.fill().ok();
    };
    let wheel = (x + W / 2.0 - 2.5, y + 6.0, 5.0, 9.0);
    // Forward above Back, on its left side, where the thumb is.
    let side = |top: f64| (x - 3.0, y + top, 4.0, 7.0);
    let sides = [(9, side(20.0)), (8, side(29.0))];

    cr.save().ok();
    if pressed {
        cr.save().ok();
        body(cr);
        cr.clip();
        match button {
            3 => {
                cr.rectangle(x + W / 2.0, y, W / 2.0, BUTTONS);
                lit(cr);
            }
            2 => {
                cr.rectangle(wheel.0, wheel.1, wheel.2, wheel.3);
                lit(cr);
            }
            _ => {}
        }
        cr.restore().ok();
    }
    cr.set_source_rgba(fg.0, fg.1, fg.2, 0.8 * alpha);
    cr.set_line_width(2.0);
    body(cr);
    cr.stroke().ok();
    cr.move_to(x, y + BUTTONS);
    cr.line_to(x + W, y + BUTTONS);
    cr.move_to(x + W / 2.0, y);
    cr.line_to(x + W / 2.0, wheel.1);
    cr.move_to(x + W / 2.0, wheel.1 + wheel.3);
    cr.line_to(x + W / 2.0, y + BUTTONS);
    cr.stroke().ok();
    cr.set_line_width(1.5);
    cr.rectangle(wheel.0, wheel.1, wheel.2, wheel.3);
    cr.stroke().ok();
    if matches!(button, 8 | 9) {
        for (side_button, (sx, sy, sw, sh)) in sides {
            cr.rectangle(sx, sy, sw, sh);
            if pressed && side_button == button {
                lit(cr);
                cr.set_source_rgba(fg.0, fg.1, fg.2, 0.8 * alpha);
            } else {
                cr.stroke().ok();
            }
        }
    }
    cr.restore().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trail_ends_where_the_shape_has_got_to() {
        let shape = [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
        assert_eq!(points_along(&shape, 0.0), vec![(0.0, 0.0), (0.0, 0.0)]);
        assert_eq!(points_along(&shape, 0.75), vec![(0.0, 0.0), (0.0, 1.0), (0.5, 1.0)]);
        assert_eq!(points_along(&shape, 1.0).last(), Some(&(1.0, 1.0)));
    }
}
