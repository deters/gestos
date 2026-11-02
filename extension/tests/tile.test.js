// SPDX-License-Identifier: GPL-2.0-or-later

import {exit} from 'system';

import {hasSize, keepInside, makeTiling, moveTileEdge, restoreRect, tileRects, tilingRects} from '../lib/tile.js';

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

const overlaps = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width &&
    a.y < b.y + b.height && b.y < a.y + a.height;
const covers = (tiles, area) => tiles.reduce((sum, t) => sum + t.width * t.height, 0) ===
    area.width * area.height;
const fitsTogether = tiles =>
    tiles.every((a, i) => tiles.every((b, j) => i === j || !overlaps(a, b))) && covers(tiles, bounds);

const tileCases = [
    ['fills the area with one window', [rect(100, 100, 300, 200)], [rect(0, 0, 1000, 800)]],
    ['splits side by side windows by their widths', [rect(0, 0, 300, 200), rect(400, 0, 600, 200)],
        [rect(0, 0, 333, 800), rect(333, 0, 667, 800)]],
    ['splits stacked windows top and bottom', [rect(0, 500, 300, 200), rect(0, 0, 300, 400)],
        [rect(0, 533, 1000, 267), rect(0, 0, 1000, 533)]],
    ['keeps a small window usable', [rect(0, 0, 50, 50), rect(100, 0, 800, 50)],
        [rect(0, 0, 200, 800), rect(200, 0, 800, 800)]],
];
results.push(...tileCases.map(([name, rects, expected]) =>
    [name, tileRects(rects, bounds), expected]));

const crowded = [rect(50, 100, 400, 300), rect(300, 200, 400, 300), rect(600, 50, 300, 500),
    rect(100, 450, 500, 300)];
results.push(['fits overlapping windows together', fitsTogether(tileRects(crowded, bounds)), true]);
results.push(['keeps the gap between tiles and at the edges',
    tileRects([rect(0, 0, 400, 800), rect(500, 0, 500, 800)], bounds, 10),
    [rect(10, 10, 430, 780), rect(450, 10, 540, 780)]]);

const resized = (rects, index, edge, position, gap = 0) => {
    const tiling = makeTiling(rects, bounds, gap);
    moveTileEdge(tiling, index, edge, position);
    return tilingRects(tiling);
};
const threeTiles = [rect(50, 100, 400, 300), rect(550, 100, 400, 300), rect(300, 500, 400, 250)];
results.push(['moves the dragged edge exactly', resized(threeTiles, 0, 'right', 700),
    [rect(0, 0, 700, 436), rect(700, 0, 300, 436), rect(0, 436, 1000, 364)]]);
results.push(['moves the neighbor edge from the other side', resized(threeTiles, 1, 'left', 300),
    [rect(0, 0, 300, 436), rect(300, 0, 700, 436), rect(0, 436, 1000, 364)]]);
results.push(['moves a split between groups', resized(threeTiles, 2, 'top', 300),
    [rect(0, 0, 500, 300), rect(500, 0, 500, 300), rect(0, 300, 1000, 500)]]);
results.push(['leaves an edge at the screen alone', resized(threeTiles, 0, 'left', 100),
    tileRects(threeTiles, bounds)]);
results.push(['keeps the gap while moving an edge', resized(threeTiles, 0, 'right', 690, 10)[1].x, 700]);

// What any layout must do, with any gap and after any edges are dragged: no overlaps, every tile
// inside the area and at least the smallest size, the gap kept between tiles, and with no gap, the
// whole area filled.
const inside = (t, area) => t.x >= area.x && t.y >= area.y &&
    t.x + t.width <= area.x + area.width && t.y + t.height <= area.y + area.height;
const grownBy = (t, by) => rect(t.x - by, t.y - by, t.width + 2 * by, t.height + 2 * by);
function problemsWith(tiles, gap) {
    if (tiles.some(t => t.width < 100 || t.height < 100))
        return 'a tile is smaller than 100 pixels';
    if (tiles.some(t => !inside(t, grownBy(bounds, -gap))))
        return 'a tile is outside the area';
    const cells = tiles.map(t => grownBy(t, gap / 2 - 0.01));
    if (cells.some((a, i) => cells.some((b, j) => i < j && overlaps(a, b))))
        return 'tiles overlap or are closer than the gap';
    if (gap === 0 && tiles.reduce((sum, t) => sum + t.width * t.height, 0) !==
        bounds.width * bounds.height)
        return "the area isn't filled";
    return null;
}

// The same layouts on every run.
let seed = 42;
const random = () => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
};
const randomRect = () => rect(Math.round(random() * 1200 - 200), Math.round(random() * 1000 - 200),
    Math.round(50 + random() * 900), Math.round(50 + random() * 700));
const layouts = [];
for (let i = 0; i < 300; i++)
    layouts.push(Array.from({length: 1 + Math.floor(random() * 6)}, randomRect));
for (let i = 0; i < 30; i++) {
    const spot = randomRect();
    layouts.push(Array.from({length: 2 + Math.floor(random() * 4)}, () => ({...spot})));
}

for (const gap of [0, 10, 41]) {
    const failed = layouts.map(rects => {
        const tiling = makeTiling(rects, bounds, gap);
        const placed = problemsWith(tilingRects(tiling), gap);
        for (let drag = 0; drag < 4; drag++) {
            const edge = ['left', 'right', 'top', 'bottom'][Math.floor(random() * 4)];
            moveTileEdge(tiling, Math.floor(random() * rects.length), edge,
                Math.round(random() * (edge === 'left' || edge === 'right' ? 1000 : 800)));
        }
        return [rects, placed ?? problemsWith(tilingRects(tiling), gap)];
    }).find(([, problem]) => problem);
    results.push([`fits every layout together with a gap of ${gap}, also after dragging edges`,
        failed ? `${failed[1]} in ${JSON.stringify(failed[0])}` : 'ok', 'ok']);
}

const withMinimum = tilingRects(makeTiling([rect(0, 0, 400, 800), rect(500, 0, 500, 800)], bounds, 0,
    new Map([[1, {width: 700, height: 100}]])));
results.push(["makes room for a window that can't get as small as its tile", withMinimum[1].width, 700]);

results.push(['has a size an app rounded', hasSize(rect(0, 0, 690, 410), rect(0, 0, 700, 419)), true]);
results.push(['has not a size far from it', hasSize(rect(0, 0, 400, 300), rect(0, 0, 700, 419)), false]);

results.push(['moves a rect out from under the top bar',
    keepInside(rect(0, 0, 300, 200), rect(0, 32, 1000, 768)), rect(0, 32, 300, 200)]);
results.push(['leaves a rect that is inside alone',
    keepInside(rect(50, 100, 400, 300), rect(0, 32, 1000, 768)), rect(50, 100, 400, 300)]);

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
