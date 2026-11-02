// SPDX-License-Identifier: GPL-2.0-or-later

// How far from a size a window can be and still count as having it. Apps can round their size,
// like terminals do to fit whole characters.
const SIZE_TOLERANCE = 32;

/**
 * Whether a window has about the size given, as apps round their size to their own steps.
 *
 * @param {{width: number, height: number}} frame - the window
 * @param {{width: number, height: number}} size - the size it might have
 * @returns {boolean} whether its width and height are each within the tolerance
 */
export function hasSize(frame, size) {
    return Math.abs(frame.width - size.width) <= SIZE_TOLERANCE &&
        Math.abs(frame.height - size.height) <= SIZE_TOLERANCE;
}

/**
 * Moves a rectangle as little as needed to be inside the bounds, where it fits.
 *
 * @param {{x: number, y: number, width: number, height: number}} rect - the rectangle to move
 * @param {{x: number, y: number, width: number, height: number}} bounds - where it should be
 * @returns {{x: number, y: number, width: number, height: number}} the moved rectangle
 */
export function keepInside(rect, bounds) {
    const clamp = (start, size, min, max) => Math.max(min, Math.min(start, max - size));
    // Rectangles from the Shell are boxed structs, so their fields are copied one by one.
    return {
        x: clamp(rect.x, rect.width, bounds.x, bounds.x + bounds.width),
        y: clamp(rect.y, rect.height, bounds.y, bounds.y + bounds.height),
        width: rect.width,
        height: rect.height,
    };
}

/**
 * Where a tiled window goes when a drag brings back its original size.
 *
 * The pointer stays at the same relative spot across the window, so the window doesn't slip out from
 * under it, and the top stays where it is, so the pointer stays on the titlebar.
 *
 * @param {{x: number, y: number, width: number, height: number}} tile - the window as it is now
 * @param {{width: number, height: number}} previous - the size to go back to
 * @param {number} pointerX - where the drag started
 * @returns {{x: number, y: number, width: number, height: number}} the restored rectangle
 */
export function restoreRect(tile, previous, pointerX) {
    const across = (pointerX - tile.x) / tile.width;
    return {
        x: Math.round(pointerX - across * previous.width),
        y: tile.y,
        width: previous.width,
        height: previous.height,
    };
}

// No side of a split gets less than this share, so that a small window still gets a usable tile.
const MIN_SPLIT_SHARE = 0.2;
// How far a resize can push a split, so that the tiles beyond it keep some room.
const MIN_RESIZED_SHARE = 0.05;

const AXES = [
    {start: 'x', size: 'width'},
    {start: 'y', size: 'height'},
];

function span(rects, {start, size}) {
    const first = Math.min(...rects.map(r => r[start]));
    const last = Math.max(...rects.map(r => r[start] + r[size]));
    return last - first;
}

// Splits the windows in two along the line that best separates them, and recurses into each side.
// Windows stay on the side their center is on, so they keep their order across the screen.
function buildTree(items) {
    if (items.length === 1)
        return {index: items[0].index};

    let best = null;
    for (const axis of AXES) {
        const center = r => r[axis.start] + r[axis.size] / 2;
        const sorted = [...items].sort((a, b) => center(a.rect) - center(b.rect));
        for (let k = 1; k < sorted.length; k++) {
            const before = sorted.slice(0, k);
            const after = sorted.slice(k);
            // How far the two groups overlap along this axis, or how far apart they are if
            // negative: the lower, the cleaner the split.
            const overlap = Math.max(...before.map(i => i.rect[axis.start] + i.rect[axis.size])) -
                Math.min(...after.map(i => i.rect[axis.start]));
            if (!best || overlap < best.overlap)
                best = {axis, before, after, overlap};
        }
    }

    const {axis, before, after} = best;
    const beforeSpan = span(before.map(i => i.rect), axis);
    const afterSpan = span(after.map(i => i.rect), axis);
    return {
        axis,
        // Each side gets room in proportion to how much its windows took along the split.
        share: Math.min(1 - MIN_SPLIT_SHARE,
            Math.max(MIN_SPLIT_SHARE, beforeSpan / (beforeSpan + afterSpan))),
        before: buildTree(before),
        after: buildTree(after),
    };
}

function splitArea(node, area) {
    const {start, size} = node.axis;
    const cut = Math.round(area[size] * node.share);
    return [
        {...area, [size]: cut},
        {...area, [start]: area[start] + cut, [size]: area[size] - cut},
    ];
}

function placeTree(node, area, tiles) {
    if (node.index !== undefined) {
        tiles[node.index] = area;
        return;
    }
    const [before, after] = splitArea(node, area);
    placeTree(node.before, before, tiles);
    placeTree(node.after, after, tiles);
}

// Half the gap around every tile, and the other half inside the area, adds up to the full gap
// both between tiles and at the edges.
function insets(gap) {
    const half = Math.floor(gap / 2);
    return {half, outer: gap - half};
}

/**
 * Plans how windows fit into an area without overlaps, keeping each near where it was.
 *
 * The area is split in two again and again, each time along the line that best separates the
 * windows, and each side gets room in proportion to how much its windows took along that line.
 *
 * @param {Array<{x: number, y: number, width: number, height: number}>} rects - the windows
 * @param {{x: number, y: number, width: number, height: number}} area - where they fit
 * @param {number} [gap] - the pixels to leave between tiles and around the area
 * @returns {object} the tiling, for tilingRects and moveTileEdge
 */
export function makeTiling(rects, area, gap = 0) {
    const {outer} = insets(gap);
    return {
        tree: rects.length > 0 ? buildTree(rects.map((rect, index) => ({rect, index}))) : null,
        area: {
            x: area.x + outer,
            y: area.y + outer,
            width: area.width - 2 * outer,
            height: area.height - 2 * outer,
        },
        gap,
    };
}

/**
 * @param {object} tiling - from makeTiling
 * @returns {Array<{x: number, y: number, width: number, height: number}>} a tile for each window,
 *   in the order they were given
 */
export function tilingRects(tiling) {
    if (!tiling.tree)
        return [];
    const {half} = insets(tiling.gap);
    const tiles = [];
    placeTree(tiling.tree, tiling.area, tiles);
    return tiles.map(t => ({
        x: t.x + half,
        y: t.y + half,
        width: t.width - 2 * half,
        height: t.height - 2 * half,
    }));
}

/**
 * Moves the split along one edge of a window's tile to where that edge was dragged. The tiles on the
 * other side of it make room, and the tiles within each side keep their proportions.
 *
 * @param {object} tiling - from makeTiling, which this changes
 * @param {number} index - the window whose tile was resized
 * @param {string} edge - 'left', 'right', 'top' or 'bottom'
 * @param {number} position - where that edge of the tile is now
 */
export function moveTileEdge(tiling, index, edge, position) {
    const axis = edge === 'left' || edge === 'right' ? AXES[0] : AXES[1];
    const side = edge === 'right' || edge === 'bottom' ? 'before' : 'after';

    // The edge is the split nearest the tile on its way down from the whole area.
    let found = null;
    const walk = (node, area) => {
        if (node.index !== undefined)
            return node.index === index;
        const [before, after] = splitArea(node, area);
        for (const [name, child, childArea] of [['before', node.before, before], ['after', node.after, after]]) {
            if (walk(child, childArea)) {
                if (!found && node.axis === axis && name === side)
                    found = {node, area};
                return true;
            }
        }
        return false;
    };
    if (!tiling.tree)
        return;
    walk(tiling.tree, tiling.area);
    if (!found)
        return;

    const {half} = insets(tiling.gap);
    const cut = side === 'before' ? position + half : position - half;
    const {node, area} = found;
    node.share = Math.min(1 - MIN_RESIZED_SHARE,
        Math.max(MIN_RESIZED_SHARE, (cut - area[axis.start]) / area[axis.size]));
}

/**
 * Fits windows into the area without overlaps, keeping each near where it was.
 *
 * @param {Array<{x: number, y: number, width: number, height: number}>} rects - the windows
 * @param {{x: number, y: number, width: number, height: number}} area - where they fit
 * @param {number} [gap] - the pixels to leave between tiles and around the area
 * @returns {Array<{x: number, y: number, width: number, height: number}>} a tile for each window,
 *   in the same order
 */
export function tileRects(rects, area, gap = 0) {
    return tilingRects(makeTiling(rects, area, gap));
}
