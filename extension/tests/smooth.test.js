// SPDX-License-Identifier: GPL-2.0-or-later

import {exit} from 'system';

import {smoothPath} from '../lib/smooth.js';

const results = [];

const wavy = [[0, 0], [10, 5], [20, 4], [30, 10]];
results.push(['goes through every point',
    smoothPath(wavy).map(curve => curve.slice(4)), wavy.slice(1)]);
results.push(['keeps a straight line straight',
    smoothPath([[0, 0], [10, 0], [20, 0], [30, 0]]).map(curve => [curve[1], curve[3]]),
    [[0, 0], [0, 0], [0, 0]]]);
// A V: the side into the tip heads straight at it.
const [intoTip] = smoothPath([[0, 0], [5, 10], [10, 0]]);
results.push(['keeps a corner sharp', Math.abs(intoTip[3] - 2 * intoTip[2]) < 1e-9, true]);
results.push(['rounds a gentle bend', smoothPath([[0, 0], [10, 0], [18.66, 5]])[0][3] < 0, true]);
results.push(['leaves out points on top of each other',
    smoothPath([[0, 0], [0.1, 0], [10, 0]]).length, 1]);

let failures = 0;
for (const [name, actual, expected] of results) {
    if (JSON.stringify(actual) === JSON.stringify(expected))
        continue;
    failures++;
    print(`FAIL ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}
print(`${results.length - failures} of ${results.length} passed`);
exit(failures ? 1 : 0);
