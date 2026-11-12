// A ViewportFrame's picture laid over the frame's fill by the client (cli/viewports.mjs), for a worker that
// returns a transparent picture: Studio 0.747's target averages a part's edge with the clear colour at full
// strength whatever the fill's opacity, at the mean opacity.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { PNG } from "pngjs";
import { layViewportBackground, viewportBackground } from "../../cli/viewports.mjs";

const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rur-viewport-layer-"));
try {
	assert.equal(viewportBackground({}), null, "a snapshot without a fill is left as it is");
	const fill = viewportBackground({ background: { color: [30 / 255, 36 / 255, 50 / 255], transparency: 0.5 } });
	assert.deepEqual(fill, { colour: [30, 36, 50], alpha: 128 });
	const png = new PNG({ width: 3, height: 1 });
	// clear, a quarter covered white, opaque red
	png.data.set([0, 0, 0, 0, 255, 255, 255, 64, 255, 0, 0, 255]);
	const file = path.join(directory, "picture.png");
	fs.writeFileSync(file, PNG.sync.write(png));
	layViewportBackground(file, fill);
	const out = PNG.sync.read(fs.readFileSync(file)).data;
	assert.deepEqual([...out.subarray(0, 4)], [30, 36, 50, 128], "the fill where nothing covers it");
	const a = 64 / 255;
	const mean = [255, 255, 255].map((c, i) => Math.round(c * a + fill.colour[i] * (1 - a)));
	assert.deepEqual([...out.subarray(4, 8)], [...mean, Math.round((a + (128 / 255) * (1 - a)) * 255)], "an edge: the straight mean at the mean opacity");
	assert.deepEqual([...out.subarray(8, 12)], [255, 0, 0, 255], "what the world covers replaces the fill");
	// fully transparent fill: the edge still takes its colour
	const clear = viewportBackground({ background: { color: [0, 0, 0], transparency: 1 } });
	fs.writeFileSync(file, PNG.sync.write(png));
	layViewportBackground(file, clear);
	assert.deepEqual([...PNG.sync.read(fs.readFileSync(file)).data.subarray(4, 8)], [64, 64, 64, 64], "a dark fringe at BackgroundTransparency 1");
	console.log("Viewport layer: fill, edges at the mean, covered texels and a clear fill's fringe passed.");
} finally {
	fs.rmSync(directory, { recursive: true, force: true });
}
