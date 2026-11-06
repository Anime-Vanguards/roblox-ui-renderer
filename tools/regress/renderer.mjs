// Pixel regression check for the native renderer: draws saved scenes (render output directories holding
// scene.json + manifest.json + their assets and glyph atlas) with `rur-native --batch`
// (tools/native-app/batch.mjs) and compares a hash of every frame's RGBA bytes with a baseline, so
// refactors and speed-ups can be proven bit-identical.
//
//   node tools/regress/renderer.mjs                                       check against the baseline
//   node tools/regress/renderer.mjs --write                              re-record the baseline's scenes
//   node tools/regress/renderer.mjs --write --dirs out/sweep/stories [--every 5]   a new scene list
//   --only dir1,dir2   renders just those (with --write: re-records just those, keeping the others)
//   --png <dir>        keeps each frame as <dir>/<scene>.png
//   --shuffle <seed>   renders in another order (the result must not depend on it)
//   --cold             every scene starts with cold draw caches (RUR_BATCH_RESET=1); by default the
//                      caches carry over from scene to scene as in the render service, and both must
//                      give the same hashes
//   --jobs N           batch processes (RUR_JOBS; default from the core count)
//   --baseline <file>  default out/regress/renderer.json
//
// The 205 frozen scenes of out/regress/scenes are the baseline's list. Their baseline was re-recorded
// on 2026-09-30 when the native renderer replaced the Canvas2D reference (viewer/renderer.js, whose
// hashes are kept in out/regress/renderer-js.json); tools/native-app/parity.mjs compares the native
// frames with the reference's last PNGs (out/regress/reference-js).

import fs from "node:fs";
import path from "node:path";
import { ROOT } from "../native-app/config.mjs";
import { defaultJobs, renderScenes } from "../native-app/batch.mjs";

const args = {};
for (let i = 2; i < process.argv.length; i++) {
	const key = process.argv[i].replace(/^--/, "");
	const next = process.argv[i + 1];
	if (next !== undefined && !next.startsWith("--")) {
		args[key] = next;
		i++;
	} else {
		args[key] = true;
	}
}

const baselineFile = path.resolve(ROOT, String(args.baseline || "out/regress/renderer.json"));
const FROZEN = path.join(ROOT, "out", "regress", "scenes");

// The scenes: --only, else --dirs <root> sampled every --every th directory (a new list), else the
// baseline's list, else the frozen corpus out/regress/scenes (a --write never resamples silently)
function listDirs() {
	if (args.only) {
		return String(args.only).split(",");
	}
	const usable = dir => fs.existsSync(path.join(ROOT, dir, "scene.json")) && fs.existsSync(path.join(ROOT, dir, "manifest.json"));
	const relative = dir => path.relative(ROOT, dir).replace(/\\/g, "/");
	if (args.dirs) {
		const root = path.resolve(ROOT, String(args.dirs));
		const every = Math.max(1, Number(args.every || 5));
		return fs.readdirSync(root).filter((_, index) => index % every === 0).map(name => relative(path.join(root, name))).filter(usable);
	}
	if (fs.existsSync(baselineFile)) {
		return Object.keys(JSON.parse(fs.readFileSync(baselineFile, "utf8")).scenes);
	}
	return fs.readdirSync(FROZEN).sort().map(name => relative(path.join(FROZEN, name))).filter(usable);
}

const dirs = listDirs();
if (args.cold) process.env.RUR_BATCH_RESET = "1";
if (args.shuffle) {
	// Seeded Fisher-Yates (mulberry32)
	let seed = Number(args.shuffle) >>> 0 || 1;
	const random = () => {
		seed = (seed + 0x6d2b79f5) >>> 0;
		let t = seed;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
	for (let i = dirs.length - 1; i > 0; i--) {
		const j = Math.floor(random() * (i + 1));
		[dirs[i], dirs[j]] = [dirs[j], dirs[i]];
	}
}
const baseline = !args.write && fs.existsSync(baselineFile) ? JSON.parse(fs.readFileSync(baselineFile, "utf8")) : null;
const started = Date.now();
const work = path.join(ROOT, "out", "regress", "native");
const captures = await renderScenes(dirs.map(dir => ({ scene: path.join(ROOT, dir, "scene.json"), out: path.join(work, path.basename(dir)) })), { jobs: Number(args.jobs) || defaultJobs() });
const results = {};
let mismatches = 0;
let failures = 0;
dirs.forEach((dir, index) => {
	const capture = captures[index];
	if (!capture.ok) {
		failures++;
		console.log(`FAILED ${dir}: ${capture.error}`);
		return;
	}
	if (args.png) {
		fs.mkdirSync(path.resolve(ROOT, String(args.png)), { recursive: true });
		fs.copyFileSync(path.join(capture.out, "render.png"), path.resolve(ROOT, String(args.png), `${path.basename(dir)}.png`));
	}
	results[dir] = { hash: capture.fnv1a, width: capture.width, height: capture.height };
	const expected = baseline && baseline.scenes[dir];
	if (expected && expected.hash !== capture.fnv1a) {
		mismatches++;
		console.log(`MISMATCH ${dir} (${expected.hash} → ${capture.fnv1a})`);
	}
});
const wall = ((Date.now() - started) / 1000).toFixed(1);
if (args.write || !baseline) {
	fs.mkdirSync(path.dirname(baselineFile), { recursive: true });
	// --write --only re-records just those scenes: the baseline's other scenes are kept
	const kept = args.only && fs.existsSync(baselineFile) ? JSON.parse(fs.readFileSync(baselineFile, "utf8")).scenes : {};
	fs.writeFileSync(baselineFile, JSON.stringify({ created: new Date().toISOString(), renderer: "native", scenes: { ...kept, ...results } }, null, 1));
	console.log(`baseline: ${Object.keys(results).length} scenes written to ${path.relative(ROOT, baselineFile)} (${failures} failed, ${wall} s)`);
} else {
	console.log(`${dirs.length} scenes, ${mismatches} mismatches, ${failures} failed (${wall} s)`);
}
process.exit(mismatches || failures ? 1 : 0);
