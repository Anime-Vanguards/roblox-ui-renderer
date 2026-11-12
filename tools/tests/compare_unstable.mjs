// A fixture's `-- compare-unstable` header line leaves its pixels (or the rects it names) out of what a
// comparison counts, with the reason.
import assert from "node:assert/strict";
import { PNG } from "pngjs";
import { comparePngs, pixelLine, unstableOf } from "../../cli/compare.mjs";
import { totals } from "../analysis/compare_total.mjs";

const whole = unstableOf("-- A fixture\n-- compare-unstable: the order changes\n-- compare: --viewport 4x4\nreturn nil\n");
assert.deepEqual(whole, { reason: "the order changes", regions: null });
const part = unstableOf("-- compare-unstable 0,2,4,2 1,0,1,1: two rects\nreturn nil\n");
assert.deepEqual(part, { reason: "two rects", regions: [[0, 2, 4, 2], [1, 0, 1, 1]] });
assert.equal(unstableOf("-- compare: --viewport 4x4\nreturn nil\n-- compare-unstable: too late\n"), null);
assert.equal(unstableOf(null), null);

// 8 x 8 device px at scale 2: differences at (0, 0), (2, 0), (3, 5) and (7, 7)
const image = marks => {
	const png = new PNG({ width: 8, height: 8 });
	png.data.fill(255);
	for (const [x, y] of marks) png.data[(y * 8 + x) * 4] = 0;
	return png;
};
const a = image([]), b = image([[0, 0], [2, 0], [3, 5], [7, 7]]);
const plain = comparePngs(a, b, null);
assert.equal(plain.over2, 4);
assert.equal(plain.counted, 4);
assert.equal(plain.unstable, undefined);
assert.equal(pixelLine(plain), "4 px over 2 levels");

const all = comparePngs(a, b, null, whole, 2);
assert.equal(all.over2, 4);
assert.equal(all.counted, 0);
assert.deepEqual(all.unstable, { reason: "the order changes", regions: null, over2: 4 });
assert.equal(pixelLine(all), "0 px over 2 levels counted; not counted: 4 px in the whole fixture (the order changes)");

// The rects are logical: 0,2,4,2 covers device rows 4 to 7, and 1,0,1,1 device (2..3, 0..1)
const some = comparePngs(a, b, null, part, 2);
assert.equal(some.over2, 4);
assert.equal(some.counted, 1);
assert.equal(some.unstable.over2, 3);
assert.equal(pixelLine(some), "1 px over 2 levels counted; not counted: 3 px in 2 marked rects (two rects)");

const sum = totals([{ name: "one", pixels: plain }, { name: "two", pixels: all }, { name: "three", pixels: some }]);
assert.equal(sum.total, 5);
assert.equal(sum.excluded, 7);
assert.equal(sum.lines.at(-1), "total: 5 px over 2 levels in 3 comparisons (7 px not counted: unstable in Studio)");
assert.equal(sum.lines[1], "two: 0 px over 2 levels counted; not counted: 4 px in the whole fixture (the order changes)");
console.log("compare-unstable: ok");
