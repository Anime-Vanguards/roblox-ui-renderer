// Frame identity of the browser build: replays scripted input on a story natively (rur-native
// --headless --benchmark --frame-hashes) and in headless Chrome / Edge through the web module
// (native/web/host.js, the same frame commands), and compares every frame's pixels (FNV-1a of the RGBA
// render.png would hold).
//
//   node tools/web/frame-identity.mjs --story <file> --replay name=<file>:<frames> [--replay ...]
//       [--bundle <dir>] [--out <dir>] [--only name,name]
//
// A replay file is a timed input list as `rur-native --actions` takes it. --out defaults to a folder in the
// system temp directory: the bundle holds Roblox Studio's fonts, so it never goes inside this checkout.
// Needs a current native build and web module (node tools/native-app/build.mjs [--wasm]). Exit code 1
// when any frame differs; <out>/report.json holds per-script results and browser timings.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { bundle, defaultProject } from "./bundle.mjs";
import { serve } from "./serve.mjs";
import { launch } from "./browser.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const DT = 1 / 120;

function parseArgs(argv) {
	const args = {
		story: null,
		replays: [],
		out: path.join(os.tmpdir(), "rur-web-identity"),
		project: defaultProject(),
	};
	let custom = false;
	for (let i = 0; i < argv.length; i++) {
		const key = argv[i];
		const value = () => argv[++i];
		if (key === "--story") args.story = value();
		else if (key === "--bundle") args.bundle = path.resolve(value());
		else if (key === "--out") args.out = path.resolve(value());
		else if (key === "--project") args.project = path.resolve(value());
		else if (key === "--only") args.only = value().split(",");
		else if (key === "--replay") {
			const match = /^([^=]+)=(.+):(\d+)$/.exec(value());
			if (!match) throw new Error("--replay name=<file>:<frames>");
			if (!custom) args.replays = [];
			custom = true;
			args.replays.push([match[1], path.resolve(match[2]), Number(match[3])]);
		} else throw new Error(`unknown argument ${key}`);
	}
	if (args.only) args.replays = args.replays.filter(([name]) => args.only.includes(name));
	if (!args.story || !args.replays.length) throw new Error("usage: node tools/web/frame-identity.mjs --story <file> --replay name=<file>:<frames> [--replay ...] [--out <dir>]");
	return args;
}

// headless.rs Replay::load: timed events into frames of DT; a benchmark of `frames` frames runs that
// many (events after the last are never sent)
function frameCommands(actions, frames) {
	const commands = Array.from({ length: frames }, () => []);
	for (const action of actions) {
		const { t = 0, ...event } = action;
		const frame = Math.floor(t / DT);
		if (frame < frames) commands[frame].push(event);
	}
	return commands;
}

function pngOf(width, height, rgba) {
	const crcTable = Array.from({ length: 256 }, (_, n) => {
		let c = n;
		for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
		return c >>> 0;
	});
	const crc = bytes => {
		let c = 0xffffffff;
		for (const byte of bytes) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
		return (c ^ 0xffffffff) >>> 0;
	};
	const chunk = (type, data) => {
		const length = Buffer.alloc(4);
		length.writeUInt32BE(data.length);
		const body = Buffer.concat([Buffer.from(type), data]);
		const sum = Buffer.alloc(4);
		sum.writeUInt32BE(crc(body));
		return Buffer.concat([length, body, sum]);
	};
	const header = Buffer.alloc(13);
	header.writeUInt32BE(width, 0);
	header.writeUInt32BE(height, 4);
	header.set([8, 6, 0, 0, 0], 8);
	const raw = Buffer.alloc((width * 4 + 1) * height);
	for (let y = 0; y < height; y++) Buffer.from(rgba.buffer, rgba.byteOffset + y * width * 4, width * 4).copy(raw, y * (width * 4 + 1) + 1);
	return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header), chunk("IDAT", zlib.deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

const percentile = (values, p) => {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))] : null;
};

async function main() {
	const args = parseArgs(process.argv.slice(2));
	const { findNativeApp } = await import(pathToFileURL(path.join(REPO, "tools", "native-app", "config.mjs")).href);
	const { stateDir } = await import(pathToFileURL(path.join(REPO, "cli", "paths.mjs")).href);
	const engine = findNativeApp();
	if (!engine) throw new Error("no current native build: node tools/native-app/build.mjs");
	const story = path.resolve(fs.existsSync(args.story) ? args.story : path.join(args.project, args.story));
	fs.mkdirSync(args.out, { recursive: true });
	const bundleDir = args.bundle || path.join(args.out, "bundle");
	if (!args.bundle) {
		console.error("bundling the story");
		await bundle({ stories: [story], project: args.project, out: bundleDir });
	}
	const index = JSON.parse(fs.readFileSync(path.join(bundleDir, "bundle.json"), "utf8"));
	const config = index.stories[0].config;
	const snapshot = null; // bundles carry no Studio snapshots (tools/web/bundle.mjs)

	// Native: one headless replay per script, a hash per frame
	const native = {};
	for (const [name, file, frames] of args.replays) {
		const out = path.join(args.out, `native-${name}`);
		const argv = ["--headless", "--story", story, "--project", args.project, "--actions", file, "--benchmark", String(frames), "--warmup", "0", "--frame-hashes",
			"--out", out, "--viewport", config.viewport.join("x"), "--scale", String(config.scale), "--time", String(config.time), "--margin", String(config.margin || 0), "--offline", "--dt", String(DT)];
		if (snapshot) argv.push("--snapshot-dir", snapshot);
		else argv.push("--no-snapshots");
		const started = Date.now();
		const run = spawnSync(engine, argv, { cwd: REPO, encoding: "utf8", maxBuffer: 1 << 28, windowsHide: true });
		if (run.status !== 0) throw new Error(`native replay ${name} failed: ${(run.stderr || "").slice(-2000)}`);
		native[name] = JSON.parse(fs.readFileSync(path.join(out, "frames.json"), "utf8")).frames;
		const benchmark = JSON.parse(fs.readFileSync(path.join(out, "benchmark.json"), "utf8"));
		native[`${name}:timing`] = benchmark.summary.completedFrameMs;
		console.error(`native ${name}: ${frames} frames in ${((Date.now() - started) / 1000).toFixed(1)} s`);
	}

	// Browser: the same frame commands through the web module
	const { server, url } = await serve(bundleDir);
	const browser = await launch();
	const report = { story: path.relative(args.project, story).split(path.sep).join("/"), bundle: bundleDir, replays: {} };
	let failed = false;
	try {
		await browser.goto(`${url}index.html?manual`);
		await browser.waitFor("window.rur && (window.rur.state === 'ready' || window.rur.state === 'failed') && window.rur.state");
		const state = await browser.evaluate("window.rur.state");
		if (state !== "ready") throw new Error(`the page failed: ${await browser.evaluate("window.rur.error")}\n${browser.console.join("\n")}`);
		report.adapter = await browser.evaluate("window.rur.ready.adapter");
		report.loadMs = await browser.evaluate("window.rur.loadMs");
		report.mountMs = await browser.evaluate("window.rur.ready.timings.totalMs");
		report.mountTimings = await browser.evaluate("window.rur.ready.timings");
		console.error(`browser: ${report.adapter}; load ${report.loadMs.toFixed(0)} ms, mount ${report.mountMs.toFixed(0)} ms`);
		for (const [name, file, frames] of args.replays) {
			const commands = frameCommands(JSON.parse(fs.readFileSync(file, "utf8")), frames);
			const expected = native[name];
			const started = Date.now();
			const result = await browser.evaluate(`(async () => {
				const host = window.rur.host;
				const commands = ${JSON.stringify(commands)};
				const expected = ${JSON.stringify(expected)};
				const ready = host.mount(0, { firstDraw: false });
				const hashes = [], wall = [], engine = [], draw = [];
				let first = null;
				for (let i = 0; i < commands.length; i++) {
					const at = performance.now();
					const stats = host.frame(${DT}, commands[i], true);
					wall.push(performance.now() - at);
					engine.push(stats.engineMs);
					draw.push(stats.drawMs);
					const pixels = host.readPixels();
					const hash = RurHost.fnv1a(pixels.rgba);
					hashes.push(hash);
					if (first === null && hash !== expected[i]) {
						let binary = "";
						for (let j = 0; j < pixels.rgba.length; j += 32768) binary += String.fromCharCode.apply(null, pixels.rgba.subarray(j, j + 32768));
						first = { index: i, width: pixels.width, height: pixels.height, rgba: btoa(binary) };
					}
				}
				return { hashes, wall, engine, draw, first, mountMs: ready.timings.totalMs };
			})()`);
			const mismatched = result.hashes.map((hash, i) => (hash === expected[i] ? -1 : i)).filter(i => i >= 0);
			const summary = {
				frames: result.hashes.length,
				identical: result.hashes.length - mismatched.length,
				mismatched: mismatched.length,
				firstMismatch: mismatched[0] ?? null,
				mismatchedFrames: mismatched.slice(0, 50),
				browserMountMs: result.mountMs,
				browserFrameMs: { median: percentile(result.wall, 0.5), p95: percentile(result.wall, 0.95), max: Math.max(...result.wall) },
				browserEngineMs: { median: percentile(result.engine, 0.5), p95: percentile(result.engine, 0.95), max: Math.max(...result.engine) },
				browserDrawMs: { median: percentile(result.draw, 0.5), p95: percentile(result.draw, 0.95), max: Math.max(...result.draw) },
				nativeFrameMs: native[`${name}:timing`],
				note: "browser frame times include a forced draw every frame (the page itself skips unchanged frames); readback and hashing excluded",
			};
			if (result.first) {
				const png = path.join(args.out, `web-${name}-${String(result.first.index).padStart(5, "0")}.png`);
				fs.writeFileSync(png, pngOf(result.first.width, result.first.height, new Uint8Array(Buffer.from(result.first.rgba, "base64"))));
				summary.firstMismatchPng = png;
				failed = true;
			}
			report.replays[name] = summary;
			console.error(`${name}: ${summary.identical}/${summary.frames} frames identical${summary.mismatched ? `, first mismatch at frame ${summary.firstMismatch} (${summary.firstMismatchPng})` : ""}; browser frame median ${summary.browserFrameMs.median.toFixed(1)} ms, p95 ${summary.browserFrameMs.p95.toFixed(1)} ms (${((Date.now() - started) / 1000).toFixed(0)} s)`);
		}
	} finally {
		await browser.close();
		server.close();
	}
	fs.writeFileSync(path.join(args.out, "report.json"), JSON.stringify(report, null, "\t"));
	console.log(JSON.stringify(report, null, "\t"));
	return failed ? 1 : 0;
}

// (explicit exit: the browser's pipes can keep the event loop alive after it closed)
process.exit(await main());
