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
 * The biggest part of an area that none of the rectangles given cover, such as the room snapped
 * windows leave. With none of it free, the whole area.
 *
 * @param {{x: number, y: number, width: number, height: number}} area - the room there is
 * @param {Array<{x: number, y: number, width: number, height: number}>} taken - what's covered
 * @returns {{x: number, y: number, width: number, height: number}} the free part
 */
export function freeArea(area, taken) {
    let free = [{x: area.x, y: area.y, width: area.width, height: area.height}];
    for (const t of taken) {
        free = free.flatMap(f => {
            const left = Math.max(f.x, t.x);
            const right = Math.min(f.x + f.width, t.x + t.width);
            const top = Math.max(f.y, t.y);
            const bottom = Math.min(f.y + f.height, t.y + t.height);
            if (left >= right || top >= bottom)
                return [f];
            // What's beside it on each side, as far as the free part goes.
            return [
                {...f, width: left - f.x},
                {...f, x: right, width: f.x + f.width - right},
                {...f, height: top - f.y},
                {...f, y: bottom, height: f.y + f.height - bottom},
            ].filter(r => r.width > 0 && r.height > 0);
        });
    }
    const size = r => r.width * r.height;
    return free.reduce((best, r) => (!best || size(r) > size(best) ? r : best), null) ??
        {x: area.x, y: area.y, width: area.width, height: area.height};
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
 * Whether a window's smallest size can be believed. Some apps say the size they open at is their
 * smallest one until they're shown, and a size that takes up most of the screen is likely that.
 *
 * @param {{width: number, height: number}} size - the smallest size the window has
 * @param {{width: number, height: number}} area - the room the tiles have
 * @returns {boolean} whether it's no more than half the area's width and half its height
 */
export function isBelievableMinimum(size, area) {
    return size.width <= area.width / 2 && size.height <= area.height / 2;
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

// The most windows that fit across and down the area. No window gets smaller than the area split
// this way, unless it's smaller already, as some apps say they can get smaller than they can.
// Windows that can't get that small have their own smallest size, which takes over.
const MAX_COLUMNS = 4;
const MAX_ROWS = 3;

const AXES = [
    {start: 'x', size: 'width', most: MAX_COLUMNS},
    {start: 'y', size: 'height', most: MAX_ROWS},
];

// The smallest a window gets along an axis, so the most windows fit, with the gap between them.
function smallestTile(axis, plan) {
    return Math.floor(plan.area[axis.size] / axis.most) - plan.gap;
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
    return {axis, before: buildTree(before), after: buildTree(after)};
}

function leaves(node) {
    return node.index !== undefined ? [node.index] : [...leaves(node.before), ...leaves(node.after)];
}

// How much room a part of the tree takes along an axis, with its share of the gap, when its windows
// have the sizes given: side by side ones add up, and ones across the axis don't. No window takes
// more than the whole area.
function needed(node, axis, sizes, plan) {
    if (node.index !== undefined)
        return Math.min(sizes[node.index][axis.size], plan.area[axis.size] - plan.gap) + plan.gap;
    const before = needed(node.before, axis, sizes, plan);
    const after = needed(node.after, axis, sizes, plan);
    return node.axis === axis ? before + after : Math.max(before, after);
}

// Where the split between two groups of windows would be, as they are: halfway between where the
// first group ends and the second one starts. The window the user moved last stays where it is, so
// the split goes right past it instead.
function wantedCut(node, plan) {
    const {start, size} = node.axis;
    const before = leaves(node.before);
    const after = leaves(node.after);
    const end = rect => rect[start] + rect[size];
    if (before.includes(0))
        return end(plan.rects[0]) + plan.gap / 2;
    if (after.includes(0))
        return plan.rects[0][start] - plan.gap / 2;
    return (Math.max(...before.map(i => end(plan.rects[i]))) +
        Math.min(...after.map(i => plan.rects[i][start]))) / 2;
}

// Splits the area where the windows would have it, moved as far as needed for both sides to keep
// their sizes. When they can't, each side gives up room in proportion to how far it can shrink.
function splitArea(node, area, plan) {
    const {start, size} = node.axis;
    const wantBefore = needed(node.before, node.axis, plan.rects, plan);
    const wantAfter = needed(node.after, node.axis, plan.rects, plan);
    let cut;
    if (wantBefore + wantAfter <= area[size]) {
        cut = Math.min(area[size] - wantAfter,
            Math.max(wantBefore, wantedCut(node, plan) - area[start]));
    } else {
        const slackBefore = wantBefore - needed(node.before, node.axis, plan.least, plan);
        const slackAfter = wantAfter - needed(node.after, node.axis, plan.least, plan);
        const short = wantBefore + wantAfter - area[size];
        cut = wantBefore - short * slackBefore / (slackBefore + slackAfter);
    }
    return [
        {...area, [size]: cut},
        {...area, [start]: area[start] + cut, [size]: area[size] - cut},
    ];
}

// A window gets its own size, or its room if that's smaller, as close to where it was as the room
// lets it be.
function placeTree(node, area, plan, cells) {
    if (node.index !== undefined) {
        const rect = plan.rects[node.index];
        const cell = {};
        for (const {start, size} of AXES) {
            cell[size] = Math.min(rect[size] + plan.gap, area[size]);
            cell[start] = Math.min(area[start] + area[size] - cell[size],
                Math.max(area[start], rect[start] - plan.gap / 2));
        }
        cells[node.index] = cell;
        return;
    }
    const [before, after] = splitArea(node, area, plan);
    placeTree(node.before, before, plan, cells);
    placeTree(node.after, after, plan, cells);
}

// Whether two cells cover some of the same room, not only touch.
function overlap(a, b) {
    return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

// Where a window goes along an axis, sliding from `from` towards `end`: past what it's on top of,
// at its size if the room there holds it, or as small as it may get if that's what holds it. It
// doesn't go past a window in its way. Null when it can't get small enough.
function slide(from, size, least, blockers, end) {
    let start = from;
    for (const blocker of [...blockers].sort((a, b) => a.start - b.start)) {
        if (blocker.start < start + Math.min(size, least) && blocker.end > start)
            start = Math.max(start, blocker.end);
    }
    const next = Math.min(end, ...blockers.filter(b => b.start >= start).map(b => b.start));
    const room = next - start;
    if (room >= size)
        return {start, size};
    return room >= least ? {start, size: room} : null;
}

// Pushes a window out of the way of one already placed: along the axis it overlaps it less on, so
// the way the other one was moved or resized, away from it, past every window placed in its way.
function pushOut(cell, pusher, placed, least, area) {
    const overlaps = AXES.map(({start, size}) =>
        Math.min(cell[start] + cell[size], pusher[start] + pusher[size]) -
        Math.max(cell[start], pusher[start]));
    const axis = overlaps[0] <= overlaps[1] ? AXES[0] : AXES[1];
    const across = axis === AXES[0] ? AXES[1] : AXES[0];
    const {start, size} = axis;
    const center = c => c[start] + c[size] / 2;
    const forward = center(cell) >= center(pusher);
    // Going back is going forward along the axis turned around.
    const along = v => forward ? v : -v;
    const blockers = placed
        .filter(other => other[across.start] < cell[across.start] + cell[across.size] &&
            cell[across.start] < other[across.start] + other[across.size])
        .map(other => {
            const [a, b] = [along(other[start]), along(other[start] + other[size])];
            return {start: Math.min(a, b), end: Math.max(a, b)};
        });
    const pusherEnd = forward ? pusher[start] + pusher[size] : -pusher[start];
    const ownStart = forward ? cell[start] : -(cell[start] + cell[size]);
    const areaEnd = forward ? area[start] + area[size] : -area[start];
    const spot = slide(Math.max(ownStart, pusherEnd), cell[size], least[size], blockers, areaEnd);
    if (!spot)
        return null;
    return {
        ...cell,
        [start]: forward ? spot.start : -(spot.start + spot.size),
        [size]: spot.size,
    };
}

// Fits windows together by pushing them out of each other's way, starting from the one the user
// moved last: each one it covers slides away from it, in the way it was moved or resized, until it
// has room, and pushes the ones it covers then in turn. One that reaches the edge of the area, or
// a window in its way, gets smaller. Windows nothing pushes stay where they are. Null when one
// can't get small enough, for the windows to be fitted together another way.
function pushWindows(plan) {
    const {area, gap, rects} = plan;
    const cells = rects.map(rect => keepInside({
        x: rect.x - gap / 2,
        y: rect.y - gap / 2,
        width: Math.min(rect.width + gap, area.width),
        height: Math.min(rect.height + gap, area.height),
    }, area));
    const least = plan.least.map(size => ({width: size.width + gap, height: size.height + gap}));
    const placed = new Set();
    for (let first = 0; first < cells.length; first++) {
        if (placed.has(first))
            continue;
        placed.add(first);
        const pushers = [first];
        while (pushers.length > 0) {
            const pusher = pushers.shift();
            for (let i = 0; i < cells.length; i++) {
                if (placed.has(i) || !overlap(cells[i], cells[pusher]))
                    continue;
                const moved = pushOut(cells[i], cells[pusher],
                    [...placed].map(j => cells[j]), least[i], area);
                if (!moved)
                    return null;
                cells[i] = moved;
                placed.add(i);
                pushers.push(i);
            }
        }
    }
    return cells;
}

// Fits windows together by splitting the area between them, when pushing them doesn't fit them.
function sliceWindows(plan) {
    const {rects} = plan;
    let shown = rects.map((_, i) => i);
    let tree = null;
    while (shown.length > 0) {
        tree = buildTree(shown.map(index => ({rect: rects[index], index})));
        if (shown.length === 1 ||
            AXES.every(axis => needed(tree, axis, plan.least, plan) <= plan.area[axis.size]))
            break;
        shown = shown.slice(0, -1);
    }
    const cells = rects.map(() => null);
    if (tree)
        placeTree(tree, plan.area, plan, cells);
    return cells;
}

/**
 * Fits windows together so they don't overlap, moving them first and shrinking them only when they
 * don't fit otherwise. No window gets bigger than it is.
 *
 * The first window is the one the user moved or resized last, which keeps its size and place. The
 * windows it covers are pushed out of its way, in the way it was moved or resized, and push the
 * ones they cover in turn, getting smaller only at the edge of the area or against a window in
 * their way. Windows nothing pushes stay where they are.
 *
 * When that doesn't fit them, the area is split in two again and again instead, each time along
 * the line that best separates the windows, so they keep their order across the screen. Each gets
 * the room it needs, as close to where it was as the others let it be, and the first one keeps its
 * place where there's room. When even the smallest sizes don't fit, the windows the user touched
 * longest ago are minimized, until the rest do.
 *
 * @param {Array<{x: number, y: number, width: number, height: number}>} rects - the windows at the
 *   size and place they'd have on their own, the one the user touched last first
 * @param {{x: number, y: number, width: number, height: number}} area - where they fit
 * @param {number} [gap] - the pixels to leave between windows and around the area
 * @param {Map<number, {width: number, height: number}>} [minimums] - the smallest size of windows,
 *   by their index
 * @returns {Array<{x: number, y: number, width: number, height: number}|null>} where each window
 *   goes, in the same order, or null for the ones to minimize
 */
export function tileWindows(rects, area, gap = 0, minimums = new Map()) {
    // Half the gap inside the area, and half around every window, adds up to the full gap both
    // between windows and at the edges.
    const plan = {
        area: {
            x: area.x + gap / 2,
            y: area.y + gap / 2,
            width: area.width - gap,
            height: area.height - gap,
        },
        gap,
        rects,
    };
    // Windows get no smaller than the area split into the most of them, unless they're smaller
    // than that already.
    plan.least = rects.map((rect, i) => {
        if (i === 0)
            return rect;
        const least = {};
        for (const axis of AXES) {
            least[axis.size] = Math.min(rect[axis.size],
                Math.max(smallestTile(axis, plan), minimums.get(i)?.[axis.size] ?? 0));
        }
        return least;
    });

    const cells = pushWindows(plan) ?? sliceWindows(plan);
    const half = gap / 2;
    // Rounded edge by edge, so that neighbors stay exactly the gap apart, odd ones too.
    return cells.map(c => {
        if (!c)
            return null;
        const x = Math.round(c.x + half);
        const y = Math.round(c.y + half);
        return {
            x,
            y,
            width: Math.round(c.x + c.width - half) - x,
            height: Math.round(c.y + c.height - half) - y,
        };
    });
}
