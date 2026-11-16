// SPDX-License-Identifier: GPL-2.0-or-later

import {keepInside} from './tile.js';

// The smallest a window gets by dragging with the wheel button, in pixels, as apps may not have a
// minimum of their own.
export const MIN_RESIZED_SIZE = 120;

/**
 * Where a window goes as the pointer is dragged with the wheel button held: each of its sides moves
 * out by as much as the pointer moved up, or in by as much as it moved down, around its center. It
 * never gets bigger than the work area, or smaller than a minimum, and stays inside the work area.
 *
 * @param {{x: number, y: number, width: number, height: number}} start - where it was at the start
 * @param {number} grow - how far up the pointer moved since, in pixels, negative for down
 * @param {{x: number, y: number, width: number, height: number}} area - the work area it's in
 * @returns {{x: number, y: number, width: number, height: number}} where it goes
 */
export function grownRect(start, grow, area) {
    // One already past a limit isn't made to jump to it.
    const fit = (size, room) => {
        const smallest = Math.min(size, MIN_RESIZED_SIZE);
        const biggest = Math.max(size, room);
        return Math.round(Math.max(smallest, Math.min(biggest, size + 2 * grow)));
    };
    const width = fit(start.width, area.width);
    const height = fit(start.height, area.height);
    const centered = {
        x: Math.round(start.x + (start.width - width) / 2),
        y: Math.round(start.y + (start.height - height) / 2),
        width,
        height,
    };
    return keepInside(centered, area);
}
