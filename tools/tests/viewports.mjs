// End to end: a render of fixtures/viewport-models.luau draws its three ViewportFrames' models through the 3D
// worker the native app starts itself (docs/viewports.md: ror on the PATH, beside the checkout or
// RUR_VIEWPORT_WORKER). Needs ror and local Studio content; no cloud assets or network access. The worker
// protocol and its edge cases are covered by native/src/viewports.rs' tests.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { PNG } from "pngjs";
import { ROOT } from "../../cli/paths.mjs";

const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rur-viewports-test-"));
try {
	const out = path.join(directory, "out");
	// a state dir of its own, so no picture kept by an earlier render stands in for the worker's
	const env = { ...process.env, RUR_STATE_DIR: path.join(directory, "state") };
	const result = spawnSync(process.execPath, ["cli/rur.mjs", "render", "--fixture", "fixtures/viewport-models.luau", "--viewport", "820x360",
		"--scale", "1", "--offline", "--no-service", "--json", "--out", out], { cwd: ROOT, env, encoding: "utf8", windowsHide: true });
	assert.equal(result.status, 0, result.stderr);
	const summary = JSON.parse(fs.readFileSync(path.join(out, "summary.json"), "utf8"));
	assert.deepEqual(summary.missingImages ?? [], [], "every ViewportFrame was drawn");
	const png = PNG.sync.read(fs.readFileSync(path.join(out, "render.png")));
	// The frames: 240 x 280 at (20, 30) and (280, 30); the first holds its model directly, the second in a WorldModel
	const region = x0 => {
		const pixels = [];
		for (let y = 30; y < 310; y++) for (let x = x0; x < x0 + 240; x++) pixels.push(...png.data.subarray((y * png.width + x) * 4, (y * png.width + x) * 4 + 4));
		return pixels;
	};
	const direct = region(20), world = region(280);
	assert.deepEqual(world, direct, "a WorldModel's model draws as a direct child's");
	// the fill is BackgroundColor3 (30, 36, 50); the model covers a good part of the frame
	let covered = 0;
	for (let index = 0; index < direct.length; index += 4) {
		if (Math.abs(direct[index] - 30) + Math.abs(direct[index + 1] - 36) + Math.abs(direct[index + 2] - 50) > 30) covered++;
	}
	assert(covered > 5000, `the models are drawn (${covered} pixels differ from the fill)`);
	console.log(`Viewports: three frames drawn by the worker, WorldModel = direct, ${covered} model pixels.`);
} finally {
	fs.rmSync(directory, { recursive: true, force: true });
}
