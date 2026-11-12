// Integration test: built rur engine -> live instance snapshot -> 3D viewport worker -> transparent PNG.
// Needs a built viewport worker (cli/viewports.mjs) and local Studio content; no cloud assets or network
// access are required.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { PNG } from "pngjs";
import { ensureEngine, engineBaseArgs } from "../../cli/engine.mjs";
import { ViewportWorker } from "../../cli/viewports.mjs";
import { ROOT } from "../../cli/paths.mjs";

const engine = await ensureEngine();
const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rur-viewports-test-"));
const sceneFile = path.join(directory, "scene.json");
const previousState = process.env.RUR_STATE_DIR;
const worker = new ViewportWorker({ offline: true });
try {
	const result = spawnSync(engine, [...engineBaseArgs(), "--fixture", path.join(ROOT, "fixtures/viewport-models.luau"),
		"--viewport", "820x360", "--scale", "1", "--time", "0", "--dump-scene", sceneFile], { encoding: "utf8", windowsHide: true });
	assert.equal(result.status, 0, result.stderr);
	const nodes = JSON.parse(fs.readFileSync(sceneFile, "utf8")).nodes.filter(node => node.viewport);
	assert.equal(nodes.length, 3);
	process.env.RUR_STATE_DIR = directory;
	const direct = await worker.render(nodes[0].image.uri, nodes[0].viewport);
	const world = await worker.render(nodes[1].image.uri, nodes[1].viewport);
	const pixels = PNG.sync.read(fs.readFileSync(direct.file));
	assert.deepEqual(PNG.sync.read(fs.readFileSync(world.file)).data, pixels.data, "WorldModel and direct model geometry match");
	assert(pixels.data.filter((value, index) => index % 4 === 3 && value > 0).length > 1000, "model is actually visible");
	// the worker draws the world over the frame's fill (BackgroundColor3 30, 36, 50), as Studio's target holds it
	assert.deepEqual([...pixels.data.subarray(0, 4)], [30, 36, 50, 255], "the frame's fill where nothing covers it");
	assert.equal(pixels.width, 240, "the frame's logical size");
	const preserved = path.join(directory, "saved.png");
	fs.linkSync(direct.file, preserved);
	const saved = fs.readFileSync(preserved);
	const snapshot = structuredClone(nodes[0].viewport);
	const slot = nodes[0].image.uri.split("/")[2];
	for (let frame = 0; frame < 12; frame++) {
		snapshot.camera.fov = 30 + frame;
		const key = createHash("sha256").update(JSON.stringify(snapshot)).digest("hex");
		await worker.render(`rur-viewport://${slot}/${key}`, snapshot);
	}
	assert.equal(fs.readdirSync(worker.directory).length, 2, "animation stores one file per viewport");
	assert.deepEqual(fs.readFileSync(preserved), saved, "atomic replacement preserves saved hard-linked images");
	assert.notDeepEqual(fs.readFileSync(direct.file), saved, "camera changes render new pixels");
	await assert.rejects(worker.render(`rur-viewport://${slot}/${"a".repeat(64)}`, { ...snapshot, width: 2049 }), /dimensions/);
	worker.retain([slot]);
	assert.equal(fs.readdirSync(worker.directory).length, 1, "destroyed/remounted viewport slots are pruned");
	const workerDirectory = worker.directory;
	worker.close();
	assert(!fs.existsSync(workerDirectory), "worker removes its transient viewport images");

	// One-shot renders keep a snapshot's image for the next process, which then needs no worker
	const persisted = await worker.render(nodes[2].image.uri, nodes[2].viewport, { persist: true });
	worker.close();
	assert(fs.existsSync(persisted.file), "a persisted viewport image outlives its worker");
	const next = new ViewportWorker({ offline: true });
	try {
		const again = await next.render(nodes[2].image.uri, nodes[2].viewport, { persist: true });
		assert.equal(again.file, persisted.file, "the next process reuses the persisted image");
		assert.equal(next.child, null, "a persisted image starts no worker");
	} finally {
		next.close();
	}
	console.log("Viewports: direct/WorldModel parity, camera updates, the fill, bounded storage, saved-image isolation, cleanup and persisted images passed.");
} finally {
	worker.close();
	if (previousState === undefined) delete process.env.RUR_STATE_DIR;
	else process.env.RUR_STATE_DIR = previousState;
	fs.rmSync(directory, { recursive: true, force: true });
}
