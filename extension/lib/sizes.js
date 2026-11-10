// SPDX-License-Identifier: GPL-2.0-or-later

import {hasSize} from './tile.js';

// How many windows of one app have their size remembered, the ones changed last kept.
export const MAX_SIZES_PER_APP = 10;

/*
 * The sizes tiling gave windows, with the size each had before, for each app. Apps save the size
 * their windows have, so a window that opens again at a size this gave it gets its own size back.
 * They're kept while the window is open too, so a session that ends without the windows getting
 * their sizes back, as when the Shell crashes, is undone in the next one.
 */

/**
 * Reads the sizes as they're saved, leaving out ones older than a time.
 *
 * @param {{[appId: string]: Array<Array<number>>}} saved - for each app, the original width and
 *   height, the width and height given, and when, in seconds since 1970
 * @param {number} oldest - the oldest time to keep
 * @returns {Map<string, Array<object>>} each app's sizes
 */
export function readSizes(saved, oldest) {
    const sizes = new Map();
    for (const [appId, entries] of Object.entries(saved)) {
        const kept = entries
            .filter(([, , , , time]) => time >= oldest)
            .map(([width, height, givenWidth, givenHeight, time]) => ({
                rect: {width, height},
                size: {width: givenWidth, height: givenHeight},
                time,
            }));
        if (kept.length > 0)
            sizes.set(appId, kept);
    }
    return sizes;
}

/**
 * The sizes as they're saved, the opposite of `readSizes`.
 *
 * @param {Map<string, Array<object>>} sizes - each app's sizes
 * @returns {{[appId: string]: Array<Array<number>>}} the saved form
 */
export function writeSizes(sizes) {
    return Object.fromEntries([...sizes]
        .filter(([, entries]) => entries.length > 0)
        .map(([appId, entries]) => [appId, entries.map(({rect, size, time}) =>
            [rect.width, rect.height, size.width, size.height, time])]));
}

/**
 * Remembers a size given to a window of an app. The app's oldest is forgotten past the most kept.
 *
 * @param {Map<string, Array<object>>} sizes - each app's sizes
 * @param {string} appId - the app
 * @param {object} entry - the window's original size as `rect`, the size given, and the time
 */
export function addSize(sizes, appId, entry) {
    const entries = [...sizes.get(appId) ?? [], entry];
    entries.sort((a, b) => b.time - a.time);
    sizes.set(appId, entries.slice(0, MAX_SIZES_PER_APP));
}

/**
 * Forgets a size remembered with `addSize`.
 *
 * @param {Map<string, Array<object>>} sizes - each app's sizes
 * @param {string} appId - the app
 * @param {object} entry - the one to forget
 */
export function removeSize(sizes, appId, entry) {
    const entries = (sizes.get(appId) ?? []).filter(e => e !== entry);
    if (entries.length > 0)
        sizes.set(appId, entries);
    else
        sizes.delete(appId);
}

/**
 * Takes the size remembered for a window of an app that's at a size this gave, the one given last
 * if several were. It's forgotten, as it's only used once.
 *
 * @param {Map<string, Array<object>>} sizes - each app's sizes
 * @param {string} appId - the app
 * @param {{width: number, height: number}} frame - the window's size now
 * @returns {object|null} the entry, with the window's original size as `rect`
 */
export function takeSize(sizes, appId, frame) {
    const entries = sizes.get(appId) ?? [];
    const found = entries
        .filter(entry => hasSize(frame, entry.size))
        .reduce((last, entry) => (!last || entry.time > last.time ? entry : last), null);
    if (found)
        removeSize(sizes, appId, found);
    return found;
}
