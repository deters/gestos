//! A drawn path made into curves for showing it, as a hand would have drawn it: through every
//! point, without the corners that straight lines between them make. Sharp turns stay corners, so
//! that a V doesn't look like a U. Gestures are still matched by their points.

use crate::protractor::Point2D;

/// A turn this sharp, in degrees, is a corner, and the curve doesn't round it.
const CORNER_DEGREES: f64 = 50.0;

/// One curve of the path, from where the last one ended: a cubic Bézier curve, as Cairo draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub control1: Point2D,
    pub control2: Point2D,
    pub to: Point2D,
}

/// The curves through the points, starting at the first. Points too close to the one before are
/// left out, as their direction is only jitter.
pub fn smooth(points: &[Point2D]) -> Vec<Curve> {
    let mut kept: Vec<Point2D> = Vec::with_capacity(points.len());
    for &point in points {
        if kept.last().is_none_or(|last| distance(*last, point) > 0.5) {
            kept.push(point);
        }
    }
    let corner = |i: usize| -> bool {
        i == 0 || i + 1 >= kept.len() || turn_degrees(kept[i - 1], kept[i], kept[i + 1]) >= CORNER_DEGREES
    };
    (0..kept.len().saturating_sub(1))
        .map(|i| {
            let (from, to) = (kept[i], kept[i + 1]);
            // Where the curve heads at each end: along the line through its neighbors, or at a
            // corner or an end, straight along this piece, so that the corner stays sharp.
            let before = if corner(i) { from } else { kept[i - 1] };
            let after = if corner(i + 1) { to } else { kept[i + 2] };
            Curve {
                control1: Point2D { x: from.x + (to.x - before.x) / 6.0, y: from.y + (to.y - before.y) / 6.0 },
                control2: Point2D { x: to.x - (after.x - from.x) / 6.0, y: to.y - (after.y - from.y) / 6.0 },
                to,
            }
        })
        .collect()
}

fn distance(a: Point2D, b: Point2D) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// How much the path turns at a point, from 0 going straight on to 180 going back.
fn turn_degrees(before: Point2D, at: Point2D, after: Point2D) -> f64 {
    let incoming = (at.y - before.y).atan2(at.x - before.x);
    let outgoing = (after.y - at.y).atan2(after.x - at.x);
    let turn = (outgoing - incoming).to_degrees().rem_euclid(360.0);
    turn.min(360.0 - turn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(coords: &[(f64, f64)]) -> Vec<Point2D> {
        coords.iter().map(|&(x, y)| Point2D { x, y }).collect()
    }

    #[test]
    fn the_curves_go_through_every_point() {
        let points = path(&[(0.0, 0.0), (10.0, 5.0), (20.0, 4.0), (30.0, 10.0)]);
        let curves = smooth(&points);
        assert_eq!(curves.len(), 3);
        let ends: Vec<Point2D> = curves.iter().map(|curve| curve.to).collect();
        assert_eq!(ends, points[1..]);
    }

    #[test]
    fn a_straight_line_stays_straight() {
        for curve in smooth(&path(&[(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)])) {
            assert_eq!((curve.control1.y, curve.control2.y), (0.0, 0.0));
        }
    }

    #[test]
    fn a_gentle_bend_is_rounded_and_a_corner_is_kept() {
        // A bend of 30°: the curve into it heads past it, along its neighbors.
        let bend = smooth(&path(&[(0.0, 0.0), (10.0, 0.0), (18.66, 5.0)]));
        assert!(bend[0].control2.y < 0.0, "{:?}", bend[0]);
        // A V: each side heads straight at the tip.
        let v = smooth(&path(&[(0.0, 0.0), (5.0, 10.0), (10.0, 0.0)]));
        let into_tip = v[0].control2;
        assert!((into_tip.y - into_tip.x * 2.0).abs() < 1e-9, "{into_tip:?}");
        let out_of_tip = v[1].control1;
        assert!((out_of_tip.y - (20.0 - 2.0 * out_of_tip.x)).abs() < 1e-9, "{out_of_tip:?}");
    }

    #[test]
    fn points_on_top_of_each_other_are_left_out() {
        let curves = smooth(&path(&[(0.0, 0.0), (0.1, 0.0), (10.0, 0.0)]));
        assert_eq!(curves.len(), 1);
        assert!(smooth(&path(&[(1.0, 1.0)])).is_empty());
    }
}
