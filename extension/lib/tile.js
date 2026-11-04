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
 * A size made no bigger than the bounds, along each side.
 *
 * @param {{width: number, height: number}} size - the size to fit
 * @param {{width: number, height: number}} bounds - the most it can be
 * @returns {{width: number, height: number}} the fitted size
 */
export function fitSize(size, bounds) {
    return {
        width: Math.min(size.width, bounds.width),
        height: Math.min(size.height, bounds.height),
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
// The smallest a tile gets, along each side, while the area has room for all of them. Windows that
// can't get that small have their own smallest size, which takes over.
const MIN_TILE = 100;

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

// How much room a part of the tree needs along an axis for each of its tiles to get the smallest
// size, with its share of the gap: side by side ones add up, and ones across the axis don't. No
// window needs more than the whole area, however big it says its smallest size is.
function needed(node, axis, tiling) {
    if (node.index !== undefined) {
        const minimum = Math.max(MIN_TILE, tiling.minimums.get(node.index)?.[axis.size] ?? 0);
        return Math.min(minimum, tiling.area[axis.size]) + tiling.gap;
    }
    const before = needed(node.before, axis, tiling);
    const after = needed(node.after, axis, tiling);
    return node.axis === axis ? before + after : Math.max(before, after);
}

// Splits the area at the node's share, moved as far as needed for both sides to have room for
// their tiles. When the area can't fit them all, each side gets room in proportion to its needs.
function splitArea(node, area, tiling) {
    const {start, size} = node.axis;
    const beforeNeeds = needed(node.before, node.axis, tiling);
    const afterNeeds = needed(node.after, node.axis, tiling);
    const cut = beforeNeeds + afterNeeds <= area[size]
        ? Math.min(area[size] - afterNeeds, Math.max(beforeNeeds, area[size] * node.share))
        : area[size] * beforeNeeds / (beforeNeeds + afterNeeds);
    return [
        {...area, [size]: cut},
        {...area, [start]: area[start] + cut, [size]: area[size] - cut},
    ];
}

function placeTree(node, area, tiling, tiles) {
    if (node.index !== undefined) {
        tiles[node.index] = area;
        return;
    }
    const [before, after] = splitArea(node, area, tiling);
    placeTree(node.before, before, tiling, tiles);
    placeTree(node.after, after, tiling, tiles);
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
 * @param {Map<number, {width: number, height: number}>} [minimums] - the smallest size of windows
 *   that can't get as small as tiles can, by their index
 * @returns {object} the tiling, for tilingRects and moveTileEdge
 */
export function makeTiling(rects, area, gap = 0, minimums = new Map()) {
    // Half the gap inside the area, and half around every tile, adds up to the full gap both
    // between tiles and at the edges.
    const tiling = {
        tree: null,
        area: {
            x: area.x + gap / 2,
            y: area.y + gap / 2,
            width: area.width - gap,
            height: area.height - gap,
        },
        gap,
        minimums,
    };
    if (rects.length === 0)
        return tiling;
    const items = rects.map((rect, index) => ({rect, index}));
    tiling.tree = buildTree(items);
    // Windows that can't get small enough for the splits that follow where they were, like ones
    // stacked in a cascade, get a grid instead, if one gives each of them room.
    if (overflow(tiling.tree, tiling) > 1)
        tiling.tree = bestGrid(items, tiling) ?? tiling.tree;
    return tiling;
}

// How far over the area the tiles' smallest sizes go: 1 or less when they fit.
function overflow(tree, tiling) {
    return Math.max(needed(tree, AXES[0], tiling) / tiling.area.width,
        needed(tree, AXES[1], tiling) / tiling.area.height);
}

// A run of tiles along one axis, each with the same share of it.
function evenRun(nodes, axis) {
    if (nodes.length === 1)
        return nodes[0];
    return {axis, share: 1 / nodes.length, before: nodes[0], after: evenRun(nodes.slice(1), axis)};
}

// How much room a window needs, for putting the biggest ones where there's the most room.
function roomNeeded(item, tiling) {
    const minimum = tiling.minimums.get(item.index);
    return Math.max(MIN_TILE, minimum?.width ?? 0) * Math.max(MIN_TILE, minimum?.height ?? 0);
}

// Windows in rows, top to bottom, and each row left to right, as they were on the screen. A last
// row that isn't full has wider tiles, so the windows that need the most room go there.
function gridTree(items, columns, tiling) {
    const rows = Math.ceil(items.length / columns);
    const lastRow = items.length - (rows - 1) * columns;
    const biggest = lastRow < columns
        ? [...items].sort((a, b) => roomNeeded(b, tiling) - roomNeeded(a, tiling)).slice(0, lastRow)
        : [];
    const rest = items.filter(item => !biggest.includes(item));
    const center = (item, start, size) => item.rect[start] + item.rect[size] / 2;
    const byRow = [...rest].sort((a, b) => center(a, 'y', 'height') - center(b, 'y', 'height'));
    const runs = [];
    for (let i = 0; i < byRow.length; i += columns)
        runs.push(byRow.slice(i, i + columns));
    if (biggest.length > 0)
        runs.push(biggest);
    return evenRun(runs.map(run => evenRun(
        [...run].sort((a, b) => center(a, 'x', 'width') - center(b, 'x', 'width'))
            .map(item => ({index: item.index})), AXES[0])), AXES[1]);
}

// Of the grids where every window has room, the one closest to square. If none has room for all,
// the one that goes over the least.
function bestGrid(items, tiling) {
    let best = null;
    for (let columns = 1; columns <= items.length; columns++) {
        const rows = Math.ceil(items.length / columns);
        const tree = gridTree(items, columns, tiling);
        const candidate = {tree, over: Math.max(1, overflow(tree, tiling)),
            off: Math.abs(columns - rows)};
        if (!best || candidate.over < best.over ||
            (candidate.over === best.over && candidate.off < best.off))
            best = candidate;
    }
    return best?.tree;
}

/**
 * @param {object} tiling - from makeTiling
 * @returns {Array<{x: number, y: number, width: number, height: number}>} a tile for each window,
 *   in the order they were given
 */
export function tilingRects(tiling) {
    if (!tiling.tree)
        return [];
    const half = tiling.gap / 2;
    const tiles = [];
    placeTree(tiling.tree, tiling.area, tiling, tiles);
    // Rounded edge by edge, so that neighbors stay exactly the gap apart, odd ones too.
    return tiles.map(t => {
        const x = Math.round(t.x + half);
        const y = Math.round(t.y + half);
        return {
            x,
            y,
            width: Math.round(t.x + t.width - half) - x,
            height: Math.round(t.y + t.height - half) - y,
        };
    });
}

/**
 * Moves the split along one edge of a window's tile to where that edge was dragged. The tiles on the
 * other side of it make room, and the tiles within each side keep their proportions. It stops
 * where the tiles beyond it would get smaller than the smallest size.
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
        const [before, after] = splitArea(node, area, tiling);
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

    const cut = side === 'before' ? position + tiling.gap / 2 : position - tiling.gap / 2;
    const {node, area} = found;
    // How far it can go is up to splitArea, which keeps room for the tiles beyond it.
    node.share = Math.min(1, Math.max(0, (cut - area[axis.start]) / area[axis.size]));
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
