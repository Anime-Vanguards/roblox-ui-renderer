// The native renderer against the retired Canvas2D reference renderer (viewer/renderer.js, removed
// 2026-09-30): saved scenes are drawn with `rur-native --batch` (tools/native-app/batch.mjs) and
// compared with the reference's last PNG of the same scene. The native renderer replaced it within 2
// channel levels on every frozen scene; a change that moves pixels further shows up here, with a diff.
//
//   node tools/native-app/parity.mjs [--scenes out/regress/scenes] [--out out/native-parity]
//        reference PNGs: out/regress/reference-js/<scene>.png (tools/regress/renderer.mjs --png of the
//        reference, recorded before its removal)
//   node tools/native-app/parity.mjs --fixtures corners,gradients | all [--compare-root out/compare/parity]
//        <compare root>/<name>/rur (default root out/compare): the compare's saved scene against its
//        render.png as the compare made it (the reference renderer for compares older than 2026-09-30)
//        and against studio.png
//   --dir <scene dir> --reference <png>   one scene;  --jobs N
//
// Prints a line per scene and a summary (scenes, exact, within2, mean identical %); exit 1 when a scene
// fails or exceeds 2 levels anywhere.

import fs from "node:fs";
import path from "node:path";
import { Worker } from "node:worker_threads";
import { ROOT } from "./config.mjs";
import { defaultJobs, renderScenes } from "./batch.mjs";

const args = {};
for (let i = 2; i < process.argv.length; i += 2) {
	if (!process.argv[i].startsWith("--") || process.argv[i + 1] === undefined) throw new Error("Options require --name value");
	args[process.argv[i].slice(2)] = process.argv[i + 1];
}
const out = path.resolve(args.out || "out/native-parity");
const source = path.resolve(args.scenes || "out/regress/scenes");
const references = path.resolve(ROOT, "out/regress/reference-js");
const fixtureRoot = path.resolve(ROOT, args["compare-root"] || "out/compare");
const fixtureNames = args.fixtures ? (args.fixtures === "all" ? fs.readdirSync(fixtureRoot).sort()
	.filter(name => fs.existsSync(path.join(fixtureRoot, name, "rur/scene.json")) && fs.existsSync(path.join(fixtureRoot, name, "rur/render.png"))) : args.fixtures.split(",")) : null;
let targets;
if (fixtureNames) {
	targets = fixtureNames.map(name => ({ name, dir: path.join(fixtureRoot, name, "rur"), reference: path.join(fixtureRoot, name, "rur/render.png"), studio: path.join(fixtureRoot, name, "studio.png") }));
} else if (args.dir) {
	const dir = path.resolve(args.dir);
	targets = [{ name: path.basename(dir), dir, reference: path.resolve(args.reference || path.join(references, `${path.basename(dir)}.png`)) }];
} else {
	targets = fs.readdirSync(source, { withFileTypes: true })
		.filter(entry => entry.isDirectory() && fs.existsSync(path.join(source, entry.name, "scene.json")) && fs.existsSync(path.join(source, entry.name, "manifest.json")))
		.map(entry => ({ name: entry.name, dir: path.join(source, entry.name), reference: path.join(references, `${entry.name}.png`) }))
		.sort((a, b) => a.name.localeCompare(b.name));
}
if (!targets.length) throw new Error("No saved scenes found");
fs.mkdirSync(out, { recursive: true });
for (const target of targets) target.output = path.join(out, target.name);

const jobs = Number(args.jobs) || defaultJobs();
const captures = await renderScenes(targets.map(target => ({ scene: path.join(target.dir, "scene.json"), out: target.output })), { jobs });

// PNG decoding and comparison on worker threads
const workers = Array.from({ length: Math.max(1, jobs) }, () => new Worker(new URL("./compare-worker.mjs", import.meta.url)));
const idle = [...workers];
const waiting = [];
const pending = new Map();
let nextJob = 0;
for (const worker of workers) {
	worker.on("message", ({ id, result }) => {
		pending.get(id)(result);
		pending.delete(id);
		const queued = waiting.shift();
		if (queued) queued(worker);
		else idle.push(worker);
	});
}
const compareOnWorker = async job => {
	const worker = idle.pop() || await new Promise(resolve => waiting.push(resolve));
	const id = nextJob++;
	return new Promise(resolve => {
		pending.set(id, resolve);
		worker.postMessage({ id, job });
	});
};

const results = await Promise.all(targets.map(async (target, index) => {
	const capture = captures[index];
	if (!capture.ok) return { name: target.name, error: capture.error || "native capture failed" };
	if (!fs.existsSync(target.reference)) return { name: target.name, error: `no reference PNG ${path.relative(ROOT, target.reference)}` };
	const compared = await compareOnWorker({ referencePath: target.reference, actualPath: path.join(target.output, "render.png"), diffPath: path.join(target.output, "diff.png"), studioPath: target.studio });
	return compared.error ? { name: target.name, error: compared.error } : { name: target.name, ...compared };
}));
await Promise.all(workers.map(worker => worker.terminate()));
for (const row of results) {
	if (row.error) console.log(`${row.name}: FAILED ${String(row.error).split("\n")[0]}`);
	else console.log(`${row.name}: ${row.reference.identical.toFixed(6)}% exact / ${row.reference.within2.toFixed(6)}% <=2, max ${row.reference.maxDiff}${row.studio ? `; Studio exact ${row.studio.native?.toFixed(4)}% (reference ${row.studio.reference?.toFixed(4)}%)` : ""}`);
}
fs.writeFileSync(path.join(out, "report.json"), JSON.stringify(results, null, 2));
const valid = results.filter(row => !row.error);
const summary = {
	scenes: results.length, errors: results.length - valid.length,
	exact: valid.filter(row => row.reference.identical === 100).length,
	within2: valid.filter(row => row.reference.within2 === 100).length,
	meanIdentical: valid.length ? valid.reduce((n, row) => n + row.reference.identical, 0) / valid.length : null,
};
fs.writeFileSync(path.join(out, "summary.json"), JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary));
if (summary.errors || summary.within2 !== summary.scenes) process.exitCode = 1;
