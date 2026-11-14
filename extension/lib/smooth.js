// SPDX-License-Identifier: GPL-2.0-or-later

// A drawn path made into curves for showing it, as a hand would have drawn it: through every point,
// without the corners that straight lines between them make. Sharp turns stay corners, so that a V
// doesn't look like a U. The same as Gestos's own drawings do.

// A turn this sharp, in degrees, is a corner, and the curve doesn't round it.
const CORNER_DEGREES = 50;

function turnDegrees([bx, by], [ax, ay], [nx, ny]) {
    const incoming = Math.atan2(ay - by, ax - bx);
    const outgoing = Math.atan2(ny - ay, nx - ax);
    const turn = (((outgoing - incoming) * 180 / Math.PI) % 360 + 360) % 360;
    return Math.min(turn, 360 - turn);
}

/**
 * The curves through the points, after the first, each as its two control points and its end, as
 * Cairo's curveTo takes them. Points too close to the one before are left out, as their direction
 * is only jitter.
 *
 * @param {Array} points - the path's points, as [x, y]
 * @returns {Array} the curves, as [x1, y1, x2, y2, x, y]
 */
export function smoothPath(points) {
    const kept = [];
    for (const point of points) {
        const last = kept.at(-1);
        if (!last || Math.hypot(point[0] - last[0], point[1] - last[1]) > 0.5)
            kept.push(point);
    }
    const corner = i => i === 0 || i + 1 >= kept.length ||
        turnDegrees(kept[i - 1], kept[i], kept[i + 1]) >= CORNER_DEGREES;
    const curves = [];
    for (let i = 0; i + 1 < kept.length; i++) {
        const [from, to] = [kept[i], kept[i + 1]];
        // Where the curve heads at each end: along the line through its neighbors, or at a corner
        // or an end, straight along this piece, so that the corner stays sharp.
        const before = corner(i) ? from : kept[i - 1];
        const after = corner(i + 1) ? to : kept[i + 2];
        curves.push([
            from[0] + (to[0] - before[0]) / 6, from[1] + (to[1] - before[1]) / 6,
            to[0] - (after[0] - from[0]) / 6, to[1] - (after[1] - from[1]) / 6,
            to[0], to[1],
        ]);
    }
    return curves;
}
