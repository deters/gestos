// SPDX-License-Identifier: GPL-2.0-or-later

import {AXES, buildTree, smallestTile} from './tile.js';

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

/**
 * Arranges windows so they don't overlap, moving them first and shrinking them only when they
 * don't fit otherwise. No window gets bigger than it is.
 *
 * Windows keep their order across the screen, as tiling does, and each gets the room it needs, as
 * close to where it was as the others let it be. The first window is the one the user moved or
 * resized last, which keeps its size and, where there's room, its place. When even the smallest
 * sizes don't fit, the windows the user touched longest ago are minimized, until the rest do.
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
export function arrangeWindows(rects, area, gap = 0, minimums = new Map()) {
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
    // Windows get no smaller than tiles do, unless they're smaller than that already.
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
