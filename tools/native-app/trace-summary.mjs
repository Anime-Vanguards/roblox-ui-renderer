#!/usr/bin/env node
// Summarizes a desktop profiler trace (the app's "Export trace", or a replay run with --actions): the
// display callback intervals of the scroll frames (native.scroll_active) and where the slow ones went.
//
//   node tools/native-app/trace-summary.mjs out/native [--all] [--budget 16.7]
//
// A directory reads its newest trace-*.json (the app names every export trace-<date>-<time>.json).
//
// --all takes every frame instead of the scroll frames. An interval runs from one update's start to the
// next: the "Previous callback/..." subphases split it into that update, egui / eframe CPU and the rest
// (surface acquire, egui's queue submit, present, the event loop); "Update/glyph wait" is time an update
// blocked on glyph rasterization.
import fs from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const file = args.find(arg => !arg.startsWith("--"));
if (!file) {
	console.error("usage: node tools/native-app/trace-summary.mjs <trace.json | directory> [--all] [--budget 16.7]");
	process.exit(2);
}
const traceFile = fs.statSync(file).isDirectory()
	? path.join(file, fs.readdirSync(file).filter(name => /^trace.*\.json$/.test(name)).sort((a, b) => fs.statSync(path.join(file, a)).mtimeMs - fs.statSync(path.join(file, b)).mtimeMs).at(-1) ?? "trace.json")
	: file;
const budgetIndex = args.indexOf("--budget");
const budget = budgetIndex >= 0 ? Number(args[budgetIndex + 1]) : 16.7;
const trace = JSON.parse(fs.readFileSync(traceFile, "utf8"));
const frames = trace.rur.frames.filter(frame => args.includes("--all") || frame.native?.scroll_active);
const percentile = (values, p) => {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted.length ? sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * p))] : NaN;
};
const describe = values => ["p50", 0.5, "p95", 0.95, "p99", 0.99, "max", 1].reduce((text, _, i, list) =>
	i % 2 ? text : `${text} ${list[i]} ${percentile(values, list[i + 1]).toFixed(2)}`, "").trim();
const phase = (frame, name) => (frame.native?.phases || []).filter(p => p.name === name).reduce((sum, p) => sum + p.duration_ms, 0);
const intervals = frames.map(frame => frame.interval);
console.log(`${frames.length} frames: interval ${describe(intervals)} ms; ${intervals.filter(v => v > budget).length} over ${budget} ms`);
console.log(`  draw CPU ${describe(frames.map(frame => frame.native?.cpu_draw_ms ?? 0))}`);
const gpu = frames.map(frame => frame.native?.gpu_ms).filter(Number.isFinite);
if (gpu.length) console.log(`  GPU timestamp ${describe(gpu)} (${gpu.length} samples)`);
const latency = frames.map(frame => frame.counters?.gpuSubmitToCompleteMs).filter(Number.isFinite);
if (latency.length) console.log(`  GPU submit to complete ${describe(latency)}`);
const counter = name => frames.map(frame => frame.counters?.[name] ?? 0);
const mean = values => values.reduce((a, b) => a + b, 0) / Math.max(1, values.length);
console.log(`  layer cache misses / frame ${mean(counter("imageCache/screenMisses")).toFixed(1)}, raster px ${mean(counter("imageCache/rasterPixels")).toFixed(0)}, upload bytes ${mean(counter("preparedTextureUploadBytes")).toFixed(0)}`);
const parts = [
	"Previous callback/update",
	"Previous callback/egui and eframe CPU",
	"Previous callback/acquire, submit, present and event loop",
	"Update/receive replies and assets",
	"Update/glyph wait (included in receive)",
	"Update/UI panels",
];
const slow = frames.filter(frame => frame.interval > budget);
for (const [label, list] of [["all", frames], [`over ${budget} ms`, slow]]) {
	if (!list.length) continue;
	console.log(`  mean of ${label} (${list.length}): interval ${mean(list.map(f => f.interval)).toFixed(2)}`);
	for (const name of parts) console.log(`    ${name.padEnd(58)} ${mean(list.map(f => phase(f, name))).toFixed(2)}`);
}
