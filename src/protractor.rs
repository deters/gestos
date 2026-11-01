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
    scale_to_square(&mut input_vector, 250.0);
    vectorize_path(&mut input_vector);

    let mut scored: Vec<(f64, &String)> = templates
        .iter()
        .filter(|(_, template_points)| template_points.len() >= 2)
        .map(|(name, template_points)| {
            let mut temp_vector = resample_path(template_points, n);
            scale_to_square(&mut temp_vector, 250.0);
            vectorize_path(&mut temp_vector);
            let score = compute_protractor_similarity(&input_vector, &temp_vector, max_angle);
            (score, name)
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
