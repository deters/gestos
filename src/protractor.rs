use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

pub fn path_length(points: &[Point2D]) -> f64 {
    let mut len = 0.0;
    for i in 1..points.len() {
        let dx = points[i].x - points[i - 1].x;
        let dy = points[i].y - points[i - 1].y;
        len += (dx * dx + dy * dy).sqrt();
    }
    len
}

pub fn resample_path(points: &[Point2D], n: usize) -> Vec<Point2D> {
    if n == 0 {
        return Vec::new();
    }
    if points.is_empty() {
        return vec![Point2D { x: 0.0, y: 0.0 }; n];
    }
    if points.len() == 1 {
        return vec![points[0]; n];
    }

    let len = path_length(points);
    let interval = if len > 1e-4 {
        len / (n - 1) as f64
    } else {
        0.0
    };
    let mut resampled = Vec::with_capacity(n);
    resampled.push(points[0]);

    let mut d_accum = 0.0;
    let mut pts = points.to_vec();

    let mut i = 1;
    while i < pts.len() && resampled.len() < n {
        let dx = pts[i].x - pts[i - 1].x;
        let dy = pts[i].y - pts[i - 1].y;
        let d = (dx * dx + dy * dy).sqrt();
        if (d_accum + d) >= interval && interval > 1e-4 {
            let qx = pts[i - 1].x + ((interval - d_accum) / d) * dx;
            let qy = pts[i - 1].y + ((interval - d_accum) / d) * dy;
            let q = Point2D { x: qx, y: qy };
            resampled.push(q);
            pts[i - 1] = q;
            d_accum = 0.0;
            // check the same segment again (don't increment i)
        } else {
            d_accum += d;
            i += 1;
        }
    }

    while resampled.len() < n {
        resampled.push(points[points.len() - 1]);
    }

    resampled
}

pub fn vectorize_path(points: &mut [Point2D]) {
    if points.is_empty() {
        return;
    }
    let count = points.len() as f64;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for p in points.iter() {
        sum_x += p.x;
        sum_y += p.y;
    }
    let centroid_x = sum_x / count;
    let centroid_y = sum_y / count;
    for p in points.iter_mut() {
        p.x -= centroid_x;
        p.y -= centroid_y;
    }

    let mut sum_squares = 0.0;
    for p in points.iter() {
        sum_squares += p.x * p.x + p.y * p.y;
    }
    let mut magnitude = sum_squares.sqrt();
    if magnitude < 1e-6 {
        magnitude = 1e-6;
    }
    for p in points.iter_mut() {
        p.x /= magnitude;
        p.y /= magnitude;
    }
}

pub fn compute_protractor_similarity(u: &[Point2D], w: &[Point2D], max_angle_rad: f64) -> f64 {
    let n = u.len();
    assert_eq!(n, w.len());
    let mut a = 0.0;
    let mut b = 0.0;
    for i in 0..n {
        a += u[i].x * w[i].x + u[i].y * w[i].y;
        b += u[i].x * w[i].y - u[i].y * w[i].x;
    }
    let theta = b.atan2(a);
    if theta.abs() <= max_angle_rad {
        (a * a + b * b).sqrt()
    } else {
        let phi = if theta > 0.0 {
            max_angle_rad
        } else {
            -max_angle_rad
        };
        a * phi.cos() + b * phi.sin()
    }
}

fn perpendicular_distance(p: Point2D, line_start: Point2D, line_end: Point2D) -> f64 {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let mag2 = dx * dx + dy * dy;
    if mag2 < 1e-9 {
        let dx2 = p.x - line_start.x;
        let dy2 = p.y - line_start.y;
        return (dx2 * dx2 + dy2 * dy2).sqrt();
    }
    let u = ((p.x - line_start.x) * dx + (p.y - line_start.y) * dy) / mag2;
    let u = u.clamp(0.0, 1.0);
    let intersection_x = line_start.x + u * dx;
    let intersection_y = line_start.y + u * dy;
    let diff_x = p.x - intersection_x;
    let diff_y = p.y - intersection_y;
    (diff_x * diff_x + diff_y * diff_y).sqrt()
}

fn douglas_peucker_recursive(
    points: &[Point2D],
    start: usize,
    end: usize,
    epsilon: f64,
    keep: &mut [bool],
) {
    if end <= start + 1 {
        return;
    }
    let mut max_dist = 0.0;
    let mut index = start;
    for i in (start + 1)..end {
        let dist = perpendicular_distance(points[i], points[start], points[end]);
        if dist > max_dist {
            max_dist = dist;
            index = i;
        }
    }
    if max_dist > epsilon {
        keep[index] = true;
        douglas_peucker_recursive(points, start, index, epsilon, keep);
        douglas_peucker_recursive(points, index, end, epsilon, keep);
    }
}

pub fn simplify_points(points: &[Point2D], epsilon: f64) -> Vec<Point2D> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    douglas_peucker_recursive(points, 0, points.len() - 1, epsilon, &mut keep);

    points
        .iter()
        .enumerate()
        .filter(|(idx, _)| keep[*idx])
        .map(|(_, &p)| p)
        .collect()
}

pub fn scale_to_square(points: &mut [Point2D], size: f64) {
    if points.is_empty() {
        return;
    }
    let mut min_x = points[0].x;
    let mut max_x = points[0].x;
    let mut min_y = points[0].y;
    let mut max_y = points[0].y;
    for p in points.iter() {
        if p.x < min_x {
            min_x = p.x;
        }
        if p.x > max_x {
            max_x = p.x;
        }
        if p.y < min_y {
            min_y = p.y;
        }
        if p.y > max_y {
            max_y = p.y;
        }
    }
    let w = max_x - min_x;
    let h = max_y - min_y;

    let aspect_ratio = if h.abs() > 1e-6 { w / h } else { 100.0 };
    if !(0.2..=5.0).contains(&aspect_ratio) {
        let max_dim = w.max(h);
        let scale = if max_dim > 1e-6 { size / max_dim } else { 1.0 };
        for p in points.iter_mut() {
            p.x *= scale;
            p.y *= scale;
        }
    } else {
        for p in points.iter_mut() {
            p.x *= size / w;
            p.y *= size / h;
        }
    }
}

/// How far apart, in resampled points, the stroke is looked at on each side of a point to tell how
/// much it turns there: a few points out of 64, so a corner shows as one sharp turn and a curve as
/// many small ones.
const TURN_REACH: usize = 3;
/// A turn this sharp is a corner, and one this gentle is a curve. Those in between could be either,
/// as a corner drawn by hand is a little round, so they don't tell shapes apart.
const CORNER_DEGREES: f64 = 75.0;
const CURVE_DEGREES: f64 = 55.0;
/// How many points a corner and the curve it's compared with can be apart, as hands don't put
/// them in quite the same place.
const CORNER_SLACK: usize = 6;

/// How much the stroke turns at each of its points, in degrees, looking a few points to each side.
fn turns(points: &[Point2D]) -> Vec<f64> {
    let direction = |from: Point2D, to: Point2D| {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        (dx.abs() > 1e-9 || dy.abs() > 1e-9).then(|| dy.atan2(dx))
    };
    (0..points.len())
        .map(|i| {
            if i < TURN_REACH || i + TURN_REACH >= points.len() {
                return 0.0;
            }
            let before = direction(points[i - TURN_REACH], points[i]);
            let after = direction(points[i], points[i + TURN_REACH]);
            match (before, after) {
                (Some(before), Some(after)) => {
                    let turn = (after - before).to_degrees().rem_euclid(360.0);
                    turn.min(360.0 - turn)
                }
                _ => 0.0,
            }
        })
        .collect()
}

/// How much a stroke turns along its length, as `turns` says, once smoothed of wiggles under 2% of
/// its size, like the steps of whole pixels a short diagonal is drawn in, which would look like
/// corners.
fn turn_profile(points: &[Point2D], n: usize) -> Vec<f64> {
    let (mut min, mut max) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for p in points {
        min = (min.0.min(p.x), min.1.min(p.y));
        max = (max.0.max(p.x), max.1.max(p.y));
    }
    let size = (max.0 - min.0).hypot(max.1 - min.1);
    turns(&resample_path(&simplify_points(points, size * 0.02), n))
}

/// Whether one stroke has a sharp corner where the other only curves, as a V and a U, which are
/// otherwise much alike. Both are resampled to the same number of points.
fn corners_differ(a: &[f64], b: &[f64]) -> bool {
    let sharp_where_round = |sharp: &[f64], round: &[f64]| {
        sharp.iter().enumerate().any(|(i, turn)| {
            let near = &round[i.saturating_sub(CORNER_SLACK)..(i + CORNER_SLACK + 1).min(round.len())];
            *turn >= CORNER_DEGREES && near.iter().all(|other| *other <= CURVE_DEGREES)
        })
    };
    sharp_where_round(a, b) || sharp_where_round(b, a)
}

/// Scores this close together are too near to tell apart with any confidence.
const SIMILAR_SCORE_MARGIN: f64 = 0.03;
const MAX_CANDIDATES: usize = 3;

/// The templates matching a drawn path, best first: only the best one, plus any others close
/// enough that the drawing could equally have meant them.
pub fn match_gesture_candidates(
    captured_points: &[Point2D],
    templates: &[(String, Vec<Point2D>)],
) -> Vec<String> {
    if captured_points.len() < 2 {
        return Vec::new();
    }
    let n = 64;
    let threshold_score = 0.90;
    let max_angle = 15.0 * (std::f64::consts::PI / 180.0);

    let mut input_vector = resample_path(captured_points, n);
    let input_turns = turn_profile(captured_points, n);
    scale_to_square(&mut input_vector, 250.0);
    vectorize_path(&mut input_vector);

    let mut scored: Vec<(f64, &String)> = templates
        .iter()
        .filter(|(_, template_points)| template_points.len() >= 2)
        .filter_map(|(name, template_points)| {
            if corners_differ(&input_turns, &turn_profile(template_points, n)) {
                return None;
            }
            let mut temp_vector = resample_path(template_points, n);
            scale_to_square(&mut temp_vector, 250.0);
            vectorize_path(&mut temp_vector);
            let score = compute_protractor_similarity(&input_vector, &temp_vector, max_angle);
            Some((score, name))
        })
        .filter(|(score, _)| *score >= threshold_score)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    let best = scored.first().map(|(score, _)| *score).unwrap_or_default();
    scored
        .into_iter()
        .take_while(|(score, _)| best - *score <= SIMILAR_SCORE_MARGIN)
        .take(MAX_CANDIDATES)
        .map(|(_, name)| name.clone())
        .collect()
}

pub fn match_gesture(
    captured_points: &[Point2D],
    templates: &[(String, Vec<Point2D>)],
) -> Option<String> {
    match_gesture_candidates(captured_points, templates)
        .into_iter()
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_length() {
        let pts = vec![Point2D { x: 0.0, y: 0.0 }, Point2D { x: 3.0, y: 4.0 }];
        assert_eq!(path_length(&pts), 5.0);
    }

    #[test]
    fn test_resample_path() {
        let pts = vec![Point2D { x: 0.0, y: 0.0 }, Point2D { x: 10.0, y: 0.0 }];
        let resampled = resample_path(&pts, 5);
        assert_eq!(resampled.len(), 5);
        assert_eq!(resampled[0], Point2D { x: 0.0, y: 0.0 });
        assert_eq!(resampled[1], Point2D { x: 2.5, y: 0.0 });
        assert_eq!(resampled[2], Point2D { x: 5.0, y: 0.0 });
        assert_eq!(resampled[3], Point2D { x: 7.5, y: 0.0 });
        assert_eq!(resampled[4], Point2D { x: 10.0, y: 0.0 });
    }

    fn path(coords: &[(f64, f64)]) -> Vec<Point2D> {
        coords.iter().map(|&(x, y)| Point2D { x, y }).collect()
    }

    fn templates() -> Vec<(String, Vec<Point2D>)> {
        vec![
            ("right".to_string(), path(&[(0.0, 50.0), (100.0, 50.0)])),
            ("left".to_string(), path(&[(100.0, 50.0), (0.0, 50.0)])),
            ("down".to_string(), path(&[(50.0, 0.0), (50.0, 100.0)])),
            (
                "corner".to_string(),
                path(&[(0.0, 0.0), (0.0, 100.0), (100.0, 100.0)]),
            ),
        ]
    }

    fn matched(points: &[(f64, f64)]) -> Option<String> {
        match_gesture(&path(points), &templates())
    }

    #[test]
    fn matches_a_stroke_by_direction() {
        assert_eq!(matched(&[(0.0, 0.0), (80.0, 0.0)]).as_deref(), Some("right"));
        assert_eq!(matched(&[(80.0, 0.0), (0.0, 0.0)]).as_deref(), Some("left"));
        assert_eq!(matched(&[(0.0, 0.0), (0.0, 80.0)]).as_deref(), Some("down"));
    }

    #[test]
    fn similar_templates_are_all_candidates_with_the_closest_first() {
        let similar = vec![
            ("tilted".to_string(), path(&[(0.0, 50.0), (100.0, 44.0)])),
            ("flat".to_string(), path(&[(0.0, 50.0), (100.0, 50.0)])),
            ("down".to_string(), path(&[(50.0, 0.0), (50.0, 100.0)])),
        ];
        let drawn = path(&[(0.0, 0.0), (80.0, 0.0)]);
        assert_eq!(match_gesture_candidates(&drawn, &similar), ["flat", "tilted"]);
    }

    #[test]
    fn a_clear_match_has_one_candidate() {
        let drawn = path(&[(0.0, 0.0), (80.0, 0.0)]);
        assert_eq!(match_gesture_candidates(&drawn, &templates()), ["right"]);
    }

    #[test]
    fn candidates_are_limited() {
        let many: Vec<_> = (0..6)
            .map(|i| (format!("t{i}"), path(&[(0.0, 50.0), (100.0, 50.0)])))
            .collect();
        let drawn = path(&[(0.0, 0.0), (80.0, 0.0)]);
        assert_eq!(match_gesture_candidates(&drawn, &many).len(), MAX_CANDIDATES);
    }

    #[test]
    fn matching_ignores_size_and_position() {
        let tiny = [(500.0, 500.0), (500.0, 520.0), (520.0, 520.0)];
        let large = [(-900.0, 0.0), (-900.0, 900.0), (0.0, 900.0)];
        assert_eq!(matched(&tiny).as_deref(), Some("corner"));
        assert_eq!(matched(&large).as_deref(), Some("corner"));
    }

    #[test]
    fn matching_tolerates_a_slight_tilt() {
        assert_eq!(matched(&[(0.0, 0.0), (100.0, 10.0)]).as_deref(), Some("right"));
    }

    #[test]
    fn rejects_a_shape_unlike_any_template() {
        let zigzag = [
            (0.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (100.0, 0.0),
            (50.0, 200.0),
        ];
        assert_eq!(matched(&zigzag), None);
    }

    /// Points along a curve from one angle to another around a center, in degrees.
    fn arc(center: (f64, f64), radius: f64, from: f64, to: f64, steps: usize) -> Vec<(f64, f64)> {
        (0..=steps)
            .map(|i| {
                let angle = (from + (to - from) * i as f64 / steps as f64).to_radians();
                (center.0 + radius * angle.cos(), center.1 + radius * angle.sin())
            })
            .collect()
    }

    fn v() -> Vec<(f64, f64)> {
        vec![(0.0, 0.0), (50.0, 100.0), (100.0, 0.0)]
    }

    /// Down, round at the bottom, and up. On screen, y grows downward, so the arc through 90°
    /// is its bottom.
    fn u() -> Vec<(f64, f64)> {
        let mut points = vec![(0.0, 0.0)];
        points.extend(arc((50.0, 50.0), 50.0, 180.0, 0.0, 24));
        points.push((100.0, 0.0));
        points
    }

    fn templates_v_and_u() -> Vec<(String, Vec<Point2D>)> {
        vec![("V".to_string(), path(&v())), ("U".to_string(), path(&u()))]
    }

    #[test]
    fn a_v_and_a_u_are_told_apart() {
        assert_eq!(match_gesture_candidates(&path(&v()), &templates_v_and_u()), ["V"]);
        assert_eq!(match_gesture_candidates(&path(&u()), &templates_v_and_u()), ["U"]);
        // Each alone isn't taken for the other, as the editor's warning checks.
        let only_v = vec![("V".to_string(), path(&v()))];
        assert_eq!(match_gesture(&path(&u()), &only_v), None);
        let only_u = vec![("U".to_string(), path(&u()))];
        assert_eq!(match_gesture(&path(&v()), &only_u), None);
    }

    #[test]
    fn a_v_drawn_a_little_round_is_still_a_v() {
        // Its tip is a small arc from one side's end to the other's.
        let mut rounded = vec![(0.0, 0.0)];
        rounded.extend(arc((50.0, 87.5), 5.6, 153.4, 26.6, 6));
        rounded.push((100.0, 0.0));
        assert_eq!(match_gesture(&path(&rounded), &templates_v_and_u()).as_deref(), Some("V"));
    }

    #[test]
    fn a_u_with_tight_bends_is_not_a_v() {
        // Both drawn by hand in the editor. The U turns at two bends instead of one round
        // bottom, and more tightly than the curve of a circle.
        let u = path(&[
            (137.0, 28.0), (136.0, 33.0), (135.0, 64.0), (139.0, 91.0), (154.0, 119.0),
            (175.0, 144.0), (198.0, 161.0), (220.0, 167.0), (238.0, 169.0), (255.0, 169.0),
            (273.0, 153.0), (288.0, 121.0), (297.0, 86.0), (299.0, 19.0), (299.0, -10.0),
            (299.0, -26.0), (299.0, -30.0),
        ]);
        let v = path(&[
            (162.0, 36.0), (166.0, 42.0), (174.0, 56.0), (208.0, 104.0), (231.0, 135.0),
            (251.0, 167.0), (268.0, 190.0), (280.0, 206.0), (283.0, 209.0), (291.0, 198.0),
            (311.0, 169.0), (334.0, 136.0), (352.0, 105.0), (364.0, 82.0), (372.0, 68.0),
            (376.0, 58.0),
        ]);
        assert_eq!(match_gesture(&u, &[("V".to_string(), v.clone())]), None);
        assert_eq!(match_gesture(&v, &[("U".to_string(), u)]), None);
    }

    #[test]
    fn turns_are_sharp_at_a_corner_and_gentle_along_a_curve() {
        let corner = turn_profile(&path(&v()), 64);
        assert!(corner.iter().cloned().fold(0.0, f64::max) >= CORNER_DEGREES);
        let curve = turn_profile(&path(&u()), 64);
        assert!(curve.iter().all(|turn| *turn <= CURVE_DEGREES), "{curve:?}");
    }

    #[test]
    fn a_short_line_in_steps_of_whole_pixels_is_still_a_line() {
        // A diagonal 20 pixels long, as a mouse reports it: a pixel across, then a pixel down.
        let mut steps = vec![(0.0, 0.0)];
        for i in 1..=20 {
            steps.push((f64::from(i), f64::from(i - 1)));
            steps.push((f64::from(i), f64::from(i)));
        }
        let diagonal = vec![("diagonal".to_string(), path(&[(0.0, 0.0), (100.0, 100.0)]))];
        assert_eq!(match_gesture(&path(&steps), &diagonal).as_deref(), Some("diagonal"));
    }

    #[test]
    fn handles_degenerate_input() {
        assert_eq!(matched(&[(1.0, 1.0)]), None);
        assert_eq!(matched(&[]), None);
        assert_eq!(match_gesture(&path(&[(0.0, 0.0), (9.0, 9.0)]), &[]), None);
        // A template with no usable path is skipped rather than matched.
        let unusable = vec![("dot".to_string(), path(&[(0.0, 0.0)]))];
        assert_eq!(match_gesture(&path(&[(0.0, 0.0), (9.0, 9.0)]), &unusable), None);
    }
}
