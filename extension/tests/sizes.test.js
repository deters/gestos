// SPDX-License-Identifier: GPL-2.0-or-later

import {exit} from 'system';

import {MAX_SIZES_PER_APP, addSize, readSizes, removeSize, takeSize, writeSizes} from '../lib/sizes.js';

const entry = (width, height, givenWidth, givenHeight, time) =>
    ({rect: {width, height}, size: {width: givenWidth, height: givenHeight}, time});
const results = [];

const saved = {'org.mozilla.firefox': [[1200, 800, 600, 768, 500], [900, 700, 400, 768, 50]]};
const sizes = readSizes(saved, 100);
results.push(['leaves out sizes older than the oldest kept',
    sizes.get('org.mozilla.firefox'), [entry(1200, 800, 600, 768, 500)]]);
results.push(['leaves out apps with nothing left',
    readSizes({'org.gnome.Nautilus': [[1, 1, 1, 1, 0]]}, 100).size, 0]);
results.push(['writes what it reads', writeSizes(readSizes(saved, 0)), saved]);

// Three windows of one app closed at sizes tiling gave them, as when Firefox quits.
const firefox = new Map();
addSize(firefox, 'org.mozilla.firefox', entry(1200, 800, 600, 768, 1));
addSize(firefox, 'org.mozilla.firefox', entry(1000, 700, 400, 768, 2));
addSize(firefox, 'org.mozilla.firefox', entry(800, 600, 600, 384, 3));
results.push(['gives each window of an app its own size back',
    [takeSize(firefox, 'org.mozilla.firefox', {width: 400, height: 768})?.rect,
        takeSize(firefox, 'org.mozilla.firefox', {width: 600, height: 384})?.rect,
        takeSize(firefox, 'org.mozilla.firefox', {width: 600, height: 768})?.rect],
    [{width: 1000, height: 700}, {width: 800, height: 600}, {width: 1200, height: 800}]]);
results.push(['forgets the app once every size is taken', firefox.size, 0]);

const twice = new Map();
addSize(twice, 'app', entry(500, 500, 300, 300, 1));
addSize(twice, 'app', entry(700, 700, 300, 300, 2));
results.push(['takes the one given last when two have the same size',
    takeSize(twice, 'app', {width: 301, height: 300})?.rect, {width: 700, height: 700}]);
results.push(['takes nothing at a size it never gave',
    takeSize(twice, 'app', {width: 640, height: 480}), null]);
results.push(['takes nothing for an app it knows nothing of',
    takeSize(twice, 'other', {width: 300, height: 300}), null]);

const many = new Map();
for (let time = 0; time < MAX_SIZES_PER_APP + 2; time++)
    addSize(many, 'app', entry(time, time, time, time, time));
results.push(['keeps only the newest sizes of an app',
    many.get('app').map(e => e.time),
    Array.from({length: MAX_SIZES_PER_APP}, (_, i) => MAX_SIZES_PER_APP + 1 - i)]);

const removing = new Map();
const kept = entry(1, 1, 1, 1, 1);
addSize(removing, 'app', kept);
removeSize(removing, 'app', entry(1, 1, 1, 1, 1));
results.push(['forgets only the very entry given', removing.get('app'), [kept]]);
removeSize(removing, 'app', kept);
results.push(['forgets the app with its last size', removing.has('app'), false]);

let failures = 0;
for (const [name, actual, expected] of results) {
    if (JSON.stringify(actual) === JSON.stringify(expected))
        continue;
    failures++;
    print(`FAIL ${name}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}
print(`${results.length - failures} of ${results.length} passed`);
exit(failures ? 1 : 0);
