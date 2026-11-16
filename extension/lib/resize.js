// SPDX-License-Identifier: GPL-2.0-or-later

import {keepInside} from './tile.js';

// The smallest a window gets by scrolling with the middle button held, in pixels, as apps may not
// have a minimum of their own.
export const MIN_RESIZED_SIZE = 120;
// How far each side of a window moves for each notch scrolled, in pixels.
export const RESIZE_STEP = 40;

/**
 * Where a window goes as it's scrolled with the middle button held: each of its sides moves out by a
 * step for each notch up, or in for each notch down, around its center.
 *
 * @param {{x: number, y: number, width: number, height: number}} rect - where it is
 * @param {number} notches - how many notches the wheel turned, up for positive
 * @param {{x: number, y: number, width: number, height: number}} area - the work area it's in
 * @returns {{x: number, y: number, width: number, height: number}} where it goes
 */
export function steppedRect(rect, notches, area) {
    return grownRect(rect, notches * RESIZE_STEP, area);
}

/**
 * Where a window goes as each of its sides moves out by as much, or in for a negative amount,
 * around its center. It never gets bigger than the work area, or smaller than a minimum, and stays
 * inside the work area.
 *
 * @param {{x: number, y: number, width: number, height: number}} start - where it is
 * @param {number} grow - how far each side moves out, in pixels, negative for in
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
