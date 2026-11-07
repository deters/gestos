// SPDX-License-Identifier: GPL-2.0-or-later

import {exit} from 'system';

import {fitSize, hasSize, isBelievableMinimum, keepInside, restoreRect, tileWindows} from '../lib/tile.js';

const rect = (x, y, width, height) => ({x, y, width, height});
const bounds = rect(0, 0, 1000, 800);

const restoreCases = [
    ['keeps the pointer in the middle', rect(0, 32, 1000, 768), 500, rect(350, 32, 300, 200)],
    ['keeps the pointer near the left', rect(0, 32, 1000, 768), 100, rect(70, 32, 300, 200)],
    ['keeps the pointer near the right', rect(200, 0, 800, 800), 900, rect(638, 0, 300, 200)],
];

const results = [
    ...restoreCases.map(([name, tile, pointerX, expected]) =>
        [name, restoreRect(tile, {width: 300, height: 200}, pointerX), expected]),
];

results.push(['leaves windows that fit alone',
    tileWindows([rect(0, 0, 300, 200), rect(500, 100, 300, 400)], bounds),
    [rect(0, 0, 300, 200), rect(500, 100, 300, 400)]]);
results.push(['moves a window into the area', tileWindows([rect(-50, 700, 300, 200)], bounds),
    [rect(0, 600, 300, 200)]]);
results.push(['shrinks a window bigger than the area', tileWindows([rect(0, 0, 1400, 300)], bounds),
    [rect(0, 0, 1000, 300)]]);

// The first window is the one moved last, which stays, and the other one moves out of its way.
results.push(['moves the other window out of the way',
    tileWindows([rect(200, 100, 400, 300), rect(400, 100, 400, 300)], bounds),
    [rect(200, 100, 400, 300), rect(600, 100, 400, 300)]]);
results.push(['moves the window moved last only when there is no room for the other',
    tileWindows([rect(500, 100, 400, 300), rect(300, 100, 400, 300)], bounds),
    [rect(500, 100, 400, 300), rect(100, 100, 400, 300)]]);
results.push(['moves the window moved last as little as there is no room',
    tileWindows([rect(200, 100, 400, 300), rect(100, 100, 400, 300)], bounds),
    [rect(400, 100, 400, 300), rect(0, 100, 400, 300)]]);
results.push(['moves both windows before shrinking either, with a gap',
    tileWindows([rect(200, 100, 400, 300), rect(400, 100, 400, 300)], bounds, 10),
    [rect(180, 100, 400, 300), rect(590, 100, 400, 300)]]);

results.push(['moves a window below when it overlaps more across',
    tileWindows([rect(0, 0, 600, 300), rect(400, 200, 600, 500)], bounds),
    [rect(0, 0, 600, 300), rect(400, 300, 600, 500)]]);
const three = tileWindows([rect(0, 0, 500, 300), rect(300, 0, 400, 300), rect(600, 0, 400, 300)],
    bounds);
results.push(['shrinks the others in proportion', three.map(r => r.width), [500, 250, 250]]);

// Windows get no smaller than a quarter of the width, as tiles do.
// The one moved last keeps its 300, and the other three can't fit in the 700 left.
const row = Array.from({length: 4}, (_, i) => rect(i * 200, 0, 300, 300));
results.push(['minimizes the window touched longest ago',
    tileWindows(row, bounds).map(r => r !== null),
    [true, true, true, false]]);
results.push(['minimizes a window that cannot get small enough',
    tileWindows([rect(0, 0, 600, 300), rect(400, 0, 600, 300)], bounds, 0,
        new Map([[1, {width: 500, height: 100}]])), [rect(0, 0, 600, 300), null]]);

const overlaps = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width &&
    a.y < b.y + b.height && b.y < a.y + a.height;
const inside = (t, area) => t.x >= area.x && t.y >= area.y &&
    t.x + t.width <= area.x + area.width && t.y + t.height <= area.y + area.height;
const grownBy = (t, by) => rect(t.x - by, t.y - by, t.width + 2 * by, t.height + 2 * by);
// What any arrangement must do: no window bigger than it was, none overlapping or closer than the
// gap, all inside the area, the one moved last never minimized, and its size kept where it fits.
function problemsWith(rects, placed, gap) {
    const shown = placed.filter(p => p);
    if (!placed[0])
        return 'the window moved last is minimized';
    if (placed.some((p, i) => p && (p.width > rects[i].width || p.height > rects[i].height)))
        return 'a window got bigger';
    if (rects[0].width <= bounds.width - 2 * gap && rects[0].height <= bounds.height - 2 * gap &&
        (placed[0].width !== rects[0].width || placed[0].height !== rects[0].height))
        return 'the window moved last changed size';
    if (shown.some(p => !inside(p, grownBy(bounds, -gap))))
        return 'a window is outside the area';
    const cells = shown.map(p => grownBy(p, gap / 2 - 0.01));
    if (cells.some((a, i) => cells.some((b, j) => i < j && overlaps(a, b))))
        return 'windows overlap or are closer than the gap';
    return null;
}

// The same layouts on every run.
let seed = 7;
const random = () => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
};
const randomRect = () => rect(Math.round(random() * 1200 - 200), Math.round(random() * 1000 - 200),
    Math.round(50 + random() * 900), Math.round(50 + random() * 700));
const layouts = [];
for (let i = 0; i < 300; i++)
    layouts.push(Array.from({length: 1 + Math.floor(random() * 8)}, randomRect));
for (let i = 0; i < 30; i++) {
    const spot = randomRect();
    layouts.push(Array.from({length: 2 + Math.floor(random() * 4)}, () => ({...spot})));
}
for (const gap of [0, 10, 41]) {
    const failed = layouts
        .map(rects => [rects, problemsWith(rects, tileWindows(rects, bounds, gap), gap)])
        .find(([, problem]) => problem);
    results.push([`arranges every layout with a gap of ${gap}`,
        failed ? `${failed[1]} in ${JSON.stringify(failed[0])}` : 'ok', 'ok']);
}

// Some apps say the size they open at is their smallest one, which is likely when it's most of the
// screen.
results.push(['believes a smallest size of up to half the screen',
    [isBelievableMinimum({width: 500, height: 400}, bounds),
        isBelievableMinimum({width: 900, height: 300}, bounds),
        isBelievableMinimum({width: 300, height: 700}, bounds)], [true, false, false]]);


results.push(['has a size an app rounded', hasSize(rect(0, 0, 690, 410), rect(0, 0, 700, 419)), true]);
results.push(['has not a size far from it', hasSize(rect(0, 0, 400, 300), rect(0, 0, 700, 419)), false]);

results.push(['moves a rect out from under the top bar',
    keepInside(rect(0, 0, 300, 200), rect(0, 32, 1000, 768)), rect(0, 32, 300, 200)]);
results.push(['leaves a rect that is inside alone',
    keepInside(rect(50, 100, 400, 300), rect(0, 32, 1000, 768)), rect(50, 100, 400, 300)]);

results.push(['fits a size that is too big inside the bounds',
    fitSize({width: 1400, height: 300}, bounds), {width: 1000, height: 300}]);

// The Shell gives rectangles as boxed structs, whose fields spreading doesn't copy.
class BoxedRect {
    #rect;
    constructor(x, y, width, height) {
        this.#rect = {x, y, width, height};
    }

    get x() {
        return this.#rect.x;
    }

    get y() {
        return this.#rect.y;
    }

    get width() {
        return this.#rect.width;
    }

    get height() {
        return this.#rect.height;
    }
}
results.push(['moves a boxed rect out from under the top bar',
    keepInside(new BoxedRect(0, 0, 300, 200), rect(0, 32, 1000, 768)), rect(0, 32, 300, 200)]);

let failures = 0;
for (const [name, actual, expected] of results) {
    if (JSON.stringify(actual) === JSON.stringify(expected))
        continue;
    failures++;
    print(`FAIL ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}
print(`${results.length - failures} of ${results.length} passed`);
exit(failures ? 1 : 0);
