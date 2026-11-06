// Worker thread for tools/native-app/parity.mjs: decodes a scene's native render and its reference PNG,
// compares them and writes diff.png, off the main thread (PNG decoding is the slow part).
import fs from "node:fs";
import { parentPort } from "node:worker_threads";
import { PNG } from "pngjs";
import { comparePngs } from "../../cli/compare.mjs";

// Per-pixel agreement (largest RGBA channel difference) and a diff image: pixels more than 2 levels
// off in magenta over the dimmed reference
export function compare(actual, expected) {
	if (actual.width !== expected.width || actual.height !== expected.height) throw new Error(`Image size ${actual.width}x${actual.height} differs from ${expected.width}x${expected.height}`);
	const diff = new PNG({ width: actual.width, height: actual.height });
	let identical = 0, within1 = 0, within2 = 0, maxDiff = 0, x0 = Infinity, y0 = Infinity, x1 = -1, y1 = -1;
	for (let at = 0; at < actual.data.length; at += 4) {
		let delta = 0;
		for (let c = 0; c < 4; c++) delta = Math.max(delta, Math.abs(actual.data[at + c] - expected.data[at + c]));
		if (!delta) identical++;
		if (delta <= 1) within1++;
		if (delta <= 2) within2++;
		maxDiff = Math.max(maxDiff, delta);
		if (delta > 2) {
			const p = at / 4, x = p % actual.width, y = Math.floor(p / actual.width);
			x0 = Math.min(x0, x);
			y0 = Math.min(y0, y);
			x1 = Math.max(x1, x);
			y1 = Math.max(y1, y);
		}
		diff.data[at] = delta > 2 ? 255 : expected.data[at] * 0.25;
		diff.data[at + 1] = delta > 2 ? 0 : expected.data[at + 1] * 0.25;
		diff.data[at + 2] = delta > 2 ? 255 : expected.data[at + 2] * 0.25;
		diff.data[at + 3] = 255;
	}
	const total = actual.width * actual.height;
	return { identical: identical / total * 100, within1: within1 / total * 100, within2: within2 / total * 100, maxDiff, badBox: x1 >= 0 ? [x0, y0, x1, y1] : null, diff };
}

// { referencePath, actualPath, diffPath, studioPath? } -> { reference, studio? } or { error }
export function compareScene({ referencePath, actualPath, diffPath, studioPath }) {
	const reference = PNG.sync.read(fs.readFileSync(referencePath));
	const actual = PNG.sync.read(fs.readFileSync(actualPath));
	const result = compare(actual, reference);
	fs.writeFileSync(diffPath, PNG.sync.write(result.diff));
	delete result.diff;
	const answer = { reference: result };
	if (studioPath && fs.existsSync(studioPath)) {
		const studio = PNG.sync.read(fs.readFileSync(studioPath));
		answer.studio = actual.width === studio.width && actual.height === studio.height
			? { native: comparePngs(actual, studio).exactPercent, reference: comparePngs(reference, studio).exactPercent }
			: { error: "Saved Studio viewport differs from the scene" };
	}
	return answer;
}

if (parentPort) {
	parentPort.on("message", ({ id, job }) => {
		try {
			parentPort.postMessage({ id, result: compareScene(job) });
		} catch (error) {
			parentPort.postMessage({ id, result: { error: String(error.message || error) } });
		}
	});
}
