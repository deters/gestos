// SPDX-License-Identifier: GPL-2.0-or-later

import {exit} from 'system';

import {MIN_RESIZED_SIZE, grownRect} from '../lib/resize.js';

const area = {x: 0, y: 32, width: 1920, height: 1048};
const start = {x: 600, y: 300, width: 800, height: 500};
const results = [];

results.push(['grows every side by as much as the pointer moved up',
    grownRect(start, 50, area), {x: 550, y: 250, width: 900, height: 600}]);
results.push(['shrinks every side by as much as the pointer moved down',
    grownRect(start, -100, area), {x: 700, y: 400, width: 600, height: 300}]);
results.push(['stops at the smallest size',
    grownRect(start, -1000, area),
    {x: 1000 - MIN_RESIZED_SIZE / 2, y: 550 - MIN_RESIZED_SIZE / 2, width: MIN_RESIZED_SIZE, height: MIN_RESIZED_SIZE}]);
results.push(['stops at the size of the work area',
    grownRect(start, 2000, area), area]);
results.push(['is moved back inside the work area',
    grownRect({x: 0, y: 32, width: 800, height: 500}, 50, area), {x: 0, y: 32, width: 900, height: 600}]);
results.push(['leaves a window smaller than the smallest size as it is',
    grownRect({x: 100, y: 100, width: 80, height: 60}, -10, area), {x: 100, y: 100, width: 80, height: 60}]);
results.push(['grows a window smaller than the smallest size',
    grownRect({x: 100, y: 100, width: 80, height: 60}, 10, area), {x: 90, y: 90, width: 100, height: 80}]);

let failures = 0;
for (const [name, actual, expected] of results) {
    if (JSON.stringify(actual) === JSON.stringify(expected))
        continue;
    failures++;
    print(`FAIL ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}
print(`${results.length - failures} of ${results.length} passed`);
exit(failures ? 1 : 0);
