#!/usr/bin/env node
// Engine regression check against golden output (notes/rust-engine.md, Verification). The golden data
// is the engine's output recorded when it was verified identical to the Luau reference runtime
// (2026-09-27, tools/engine/diff.mjs over every corpus below) and after each Studio-verified change
// since. Every run of the engine must reproduce it exactly: an intended change is re-recorded with
// --write only once Studio comparisons (rur compare) show it is right.
//
//   node tools/engine/golden.mjs [--corpus fixtures,stories,compares,live | --all] [--fast]
//        [--filter text] [--jobs N] [--timeout s] [--verbose] [--rerun] [--write] [--freeze] [--no-batch]
//
//   --corpus   fixtures: fixtures/*.luau and tools/engine/probes/*.luau in edit and play mode (a
//                fixture with a sibling <name>.actions.json also runs its actions);
//              stories:  every *.story.luau of the frozen project (every place);
//              compares: the `rur compare` replays frozen from out/compare (their options, Studio
//                clock RUR_CLOCK, sibling order RUR_ORDER, player, camera, place id, snapshots);
//              live:     live sessions over the stdio protocol (fixtures x 60 Hz / 120 / 144 Hz display
//                cadence x the default hover / scroll script, tools/engine/live/click.events.json,
//                --no-input, the input probe and presentation mode): every reply (timings aside), the
//                full scene after every frame (digest; the last one whole) and the verbose output.
//              Default: fixtures,live (a few minutes); --all: every corpus.
//   --fast     the npm test subset (corpus "fast"): edit-mode fixtures and probes mounted alone (no
//              project, so no frozen copy needed) plus three live runs
//   --no-batch runs every entry in a process of its own. By default scene entries (all but live
//              sessions) run in batch workers, `rur-native engine-batch`: one process per job slot runs
//              entry after entry, each exactly as a process of its own would (an engine and a VM of its
//              own, freed after it; its RUR_* environment); a process start, its exit and the engine's
//              reflection load were most of a small entry's time. An entry that fails runs again in a
//              process of its own, so its message is the one that process prints.
//   --rerun    runs every entry: without it, an entry that an earlier check found identical is not run
//              again while nothing it reads changed (the engine binary, the entry and its files, the
//              frozen inputs, data/, fixtures/, this script; out/golden/verified.json)
//   --write    records the selected entries (out/golden/<corpus>/<name>.json.gz and their digests in
//              tools/engine/golden.json) instead of checking them
//   --freeze   (normally with --write) copies the project (RUR_PROJECT: its rur.json, the roots of its
//              places, cli/places.mjs, and external/*/src) and the compare replays into out/golden
//              first. Stories and compares (and the fixtures / live runs, which mount the default
//              place's roots beside them)
//              always run against that frozen copy, so project edits never show up as engine
//              regressions; without it only --fast runs. It also freezes the fonts (--freeze-fonts).
//   --freeze-fonts [<Studio version dir>]   copies the font files the engine reads into out/golden/fonts:
//              the Studio install's content/fonts and BuilderIcons package (default: the newest
//              install) and the cloud fonts and family files of the asset cache (<repo>/.cache/assets).
//              Every corpus then reads them (RUR_FONT_ROOT) instead of the install and the cache, so a
//              Studio update (BuilderIcons gains glyphs about monthly, renumbering the rest) or a
//              cleared cache never shows up as an engine regression. The text corpus test
//              (native/tests/text_corpus.rs) reads the same set. A freeze is right when every corpus
//              and the text corpus stay identical with it.
//
// Cloud fonts: a machine's asset cache holds only the faces its renders drew, and a face it lacks is
// replaced by a substitute. So a run that differs from its golden (or is being recorded) and reports cloud
// fonts missing gets them as a render does (native/src/story_render.rs), in the cache and in the frozen set
// if there is one, and runs again; what cannot be fetched is named in the verdict.
//
// A check compares each entry's normalized output digest with tools/engine/golden.json (tracked);
// when out/golden holds the recorded output, a mismatch is explained field by field (status,
// script output, structure, properties, geometry, draw lists, diagnostics; for live runs the first
// differing reply). Normalization: timings removed, the repository / frozen project paths as <repo>
// / <project>, the Studio install as <studio-content>/ (its content folder) and <studio>/ (the rest of
// its version folder), the frozen fonts as the files they froze, printed table / function addresses as
// 0x?, a scene's editable-image files by their order (their names carry the process id). Every run starts its wall clock at the same moment (RUR_EPOCH), so dates and countdowns stories
// print are part of the output.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { DEFAULT_STATE_DIR, sharedStatePath } from "../../cli/paths.mjs";
import { fetchAssetId, studioContentRoots } from "../../cli/assets.mjs";

import { engineBaseArgs, ensureEngine } from "../../cli/engine.mjs";
import { placeOf, placeRoots, readPlaces } from "../../cli/places.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const GOLDEN = path.join(ROOT, "out", "golden");
const PROJECT = path.join(GOLDEN, "project");
const SNAPSHOTS = path.join(GOLDEN, "snapshots", "default");
const FONTS = path.join(GOLDEN, "fonts");
const MANIFEST = path.join(ROOT, "tools", "engine", "golden.json");
// The project --freeze copies: RUR_PROJECT (there is no default)
const SOURCE_PROJECT = process.env.RUR_PROJECT ? path.resolve(process.env.RUR_PROJECT) : null;
// The place fixtures and live runs mount beside them: the frozen project's first
const DEFAULT_PLACE = undefined;

function parseArgs(argv) {
	const args = {};
	for (let index = 0; index < argv.length; index++) {
		const key = argv[index];
		if (!key.startsWith("--")) continue;
		const next = argv[index + 1];
		if (next === undefined || next.startsWith("--")) args[key.slice(2)] = true;
		else {
			args[key.slice(2)] = next;
			index++;
		}
	}
	return args;
}

// Freezing

function copyTree(from, to, counts) {
	let entries;
	try {
		entries = fs.readdirSync(from, { withFileTypes: true });
	} catch {
		counts.skipped++;
		return;
	}
	fs.mkdirSync(to, { recursive: true });
	for (const entry of entries) {
		const source = path.join(from, entry.name), target = path.join(to, entry.name);
		try {
			if (entry.isDirectory()) copyTree(source, target, counts);
			else if (entry.isFile()) {
				fs.copyFileSync(source, target);
				counts.files++;
			}
		} catch {
			// names Windows cannot address (a directory "X."): the engine's scan skips them too
			counts.skipped++;
		}
	}
}

function gitState(dir) {
	const head = spawnSync("git", ["log", "-1", "--format=%H %cI"], { cwd: dir, encoding: "utf8", windowsHide: true });
	const status = spawnSync("git", ["status", "--porcelain"], { cwd: dir, encoding: "utf8", windowsHide: true, maxBuffer: 64 * 1024 * 1024 });
	return { head: head.stdout.trim() || null, dirty: status.stdout ? status.stdout.split("\n").filter(Boolean).length : 0 };
}

function freeze(studio) {
	if (!SOURCE_PROJECT) throw new Error("--freeze copies RUR_PROJECT: set it to the project to freeze");
	const started = Date.now();
	fs.rmSync(PROJECT, { recursive: true, force: true });
	const counts = { files: 0, skipped: 0 };
	for (const name of ["rur.json"]) {
		if (fs.existsSync(path.join(SOURCE_PROJECT, name))) {
			fs.mkdirSync(PROJECT, { recursive: true });
			fs.copyFileSync(path.join(SOURCE_PROJECT, name), path.join(PROJECT, name));
		}
	}
	// Every place's roots inside the project, once
	const rootDirs = [...new Set(readPlaces(SOURCE_PROJECT).flatMap(place => place.roots).map(root => path.relative(SOURCE_PROJECT, root)))];
	for (const dir of rootDirs.filter(dir => dir && !dir.startsWith("..") && !path.isAbsolute(dir))) {
		copyTree(path.join(SOURCE_PROJECT, dir), path.join(PROJECT, dir), counts);
	}
	const external = path.join(SOURCE_PROJECT, "external");
	for (const name of fs.existsSync(external) ? fs.readdirSync(external) : []) {
		if (fs.existsSync(path.join(external, name, "src"))) copyTree(path.join(external, name, "src"), path.join(PROJECT, "external", name, "src"), counts);
	}
	fs.rmSync(path.dirname(SNAPSHOTS), { recursive: true, force: true });

	// Compare replays: options, recorded clock, order dump and sources outside fixtures/ and the project
	const compares = [];
	const replays = path.join(GOLDEN, "compares-input");
	fs.rmSync(replays, { recursive: true, force: true });
	const root = path.join(ROOT, "out", "compare");
	for (const name of fs.existsSync(root) ? fs.readdirSync(root).sort() : []) {
		const reportPath = path.join(root, name, "report.json");
		if (!fs.existsSync(reportPath)) continue;
		const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
		const o = report.options;
		if (!o || !report.source) continue;
		let file = path.resolve(ROOT, report.source);
		if (!fs.existsSync(file)) continue;
		const dir = path.join(replays, name);
		fs.mkdirSync(dir, { recursive: true });
		const inProject = path.relative(SOURCE_PROJECT, file);
		if (!inProject.startsWith("..") && !path.isAbsolute(inProject)) file = { project: inProject.split(path.sep).join("/") };
		else if (!path.relative(path.join(ROOT, "fixtures"), file).startsWith("..")) file = { repo: path.relative(ROOT, file).split(path.sep).join("/") };
		else {
			fs.copyFileSync(file, path.join(dir, path.basename(file)));
			file = { replay: path.basename(file) };
		}
		const copy = (value, as) => {
			if (!value || !fs.existsSync(value)) return null;
			fs.copyFileSync(value, path.join(dir, as));
			return as;
		};
		const normalized = (file.project || "").toLowerCase();
		compares.push({
			name: `compare-${name}`,
			file,
			place: o.place || (file.project ? placeOf(SOURCE_PROJECT, path.join(SOURCE_PROJECT, file.project)) : readPlaces(SOURCE_PROJECT)[0].name),
			options: {
				viewport: String(o.viewport), scale: String(o.scale), time: String(o.time),
				...(o.camera ? { camera: String(o.camera) } : {}),
				...(o.placeId !== undefined && o.placeId !== null ? { "place-id": String(o.placeId) } : {}),
				...(Array.isArray(o.background) ? { background: o.background.join(",") } : {}),
				...(o.rotationClipping ? { "rotation-clipping": String(o.rotationClipping) } : {}),
				...(o.controls ? { controls: String(o.controls) } : {}),
			},
			clock: copy(o.clock, "clock.json"),
			order: copy(o.order, "order.json"),
			player: o.player ?? null,
			snapshots: false, // a compare's Studio snapshots are not frozen
		});
	}
	fs.writeFileSync(path.join(GOLDEN, "compares.json"), JSON.stringify(compares, null, 1));
	const state = { frozen: new Date().toISOString(), project: SOURCE_PROJECT, git: gitState(SOURCE_PROJECT), files: counts.files, skipped: counts.skipped, compares: compares.length };
	fs.writeFileSync(path.join(GOLDEN, "frozen.json"), JSON.stringify(state, null, 1));
	console.log(`frozen ${counts.files} files (${counts.skipped} unreadable names skipped) and ${compares.length} compare replays in ${((Date.now() - started) / 1000).toFixed(0)} s -> ${path.relative(ROOT, GOLDEN)}`);
	freezeFonts(studio);
}

// The files the engine's font registry indexes (native/src/engine/text/fonts.rs preload): font files and
// family JSONs; the cache's fonts.json is its download index
const FONT_FILE = name => /\.(ttf|otf|ttc|json)$/i.test(name) && name !== "fonts.json";
const BUILDER_ICONS = path.join("ExtraContent", "LuaPackages", "Packages", "_Index", "BuilderIcons");

// out/golden/fonts: studio/ laid out like the install, assets/ like the cache, manifest.json (sources and
// SHA-256 of every file). Written beside it and renamed over it, so a failed copy leaves the old set.
function freezeFonts(studio) {
	const version = studio ? path.resolve(studio) : studioContentRoots()[0];
	if (!version || !fs.existsSync(path.join(version, "content", "fonts"))) {
		throw new Error(`--freeze-fonts: ${version ? `${version} has no content/fonts` : "no Studio install found"} (pass a …/Roblox/Versions/version-<hash> directory)`);
	}
	const cache = path.join(DEFAULT_STATE_DIR, "assets");
	const target = `${FONTS}.${process.pid}.tmp`;
	fs.rmSync(target, { recursive: true, force: true });
	const files = {};
	const copy = (from, relative) => {
		const to = path.join(target, relative);
		fs.mkdirSync(path.dirname(to), { recursive: true });
		fs.copyFileSync(from, to);
		files[relative.split(path.sep).join("/")] = createHash("sha256").update(fs.readFileSync(to)).digest("hex");
	};
	const walk = (dir, relative) => {
		for (const entry of fs.existsSync(dir) ? fs.readdirSync(dir, { withFileTypes: true }) : []) {
			if (entry.isDirectory()) walk(path.join(dir, entry.name), path.join(relative, entry.name));
			else if (entry.isFile() && FONT_FILE(entry.name)) copy(path.join(dir, entry.name), path.join(relative, entry.name));
		}
	};
	walk(path.join(version, "content", "fonts"), path.join("studio", "content", "fonts"));
	walk(path.join(version, BUILDER_ICONS), path.join("studio", BUILDER_ICONS));
	for (const entry of fs.existsSync(cache) ? fs.readdirSync(cache, { withFileTypes: true }) : []) {
		if (entry.isFile() && FONT_FILE(entry.name)) copy(path.join(cache, entry.name), path.join("assets", entry.name));
	}
	const manifest = { frozen: new Date().toISOString(), studio: version, cache, files };
	fs.writeFileSync(path.join(target, "manifest.json"), JSON.stringify(manifest, null, 1));
	fs.rmSync(FONTS, { recursive: true, force: true });
	fs.renameSync(target, FONTS);
	FROZEN_FONTS = manifest;
	const count = prefix => Object.keys(files).filter(name => name.startsWith(prefix)).length;
	console.log(`froze ${count("studio/")} Studio font files (${version}) and ${count("assets/")} cached cloud font files -> ${path.relative(ROOT, FONTS)}`);
}

function frozenFonts() {
	try {
		return JSON.parse(fs.readFileSync(path.join(FONTS, "manifest.json"), "utf8"));
	} catch {
		return null;
	}
}

// A cloud font the frozen set did not get (the cache lacked it when it was frozen)
function addFrozenFont(file) {
	const relative = path.posix.join("assets", path.basename(file));
	const temporary = path.join(FONTS, `${relative}.${process.pid}.tmp`);
	fs.mkdirSync(path.dirname(temporary), { recursive: true });
	fs.copyFileSync(file, temporary);
	fs.renameSync(temporary, path.join(FONTS, relative));
	FROZEN_FONTS.files[relative] = fileDigest(file);
	writeJson(path.join(FONTS, "manifest.json"), FROZEN_FONTS);
}

const fontFailures = new Map();
let fontFetches = Promise.resolve();

// One download at a time: job slots ask for the same fonts. Whether any arrived.
function fetchFonts(ids) {
	fontFetches = fontFetches.then(async () => {
		let arrived = false;
		for (const id of ids) {
			try {
				const { kind, file } = await fetchAssetId(id, { allowDecal: false });
				if (kind !== "font" && kind !== "json") throw new Error(`asset ${id} is a ${kind}, not a font`);
				if (FROZEN_FONTS && !FROZEN_FONTS.files[path.posix.join("assets", path.basename(file))]) addFrozenFont(file);
				fontFailures.delete(id);
				arrived = true;
			} catch (error) {
				fontFailures.set(id, error.message);
			}
		}
		return arrived;
	});
	return fontFetches;
}

const missingFonts = now => asArray((now.scene ?? now.lastScene)?.diagnostics?.missingFontAssets);

// A family's JSON arrives first, then its faces
async function withCloudFonts(entry, first) {
	let now = first;
	for (let round = 0; round < 3; round++) {
		const missing = missingFonts(now);
		if (!missing.length || !(await fetchFonts(missing))) break;
		now = entry.live ? await runLive(entry) : await runScene(entry);
	}
	return now;
}

// Entries

const projectRoots = place => placeRoots(PROJECT, place).filter(dir => fs.existsSync(dir));
const snapshotsFor = () => null;
const frozen = () => fs.existsSync(path.join(GOLDEN, "frozen.json")) && fs.existsSync(PROJECT);

function listStories(place) {
	const files = [];
	const walk = dir => {
		for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
			const full = path.join(dir, item.name);
			if (item.isDirectory()) walk(full);
			else if (item.name.endsWith(".story.luau")) files.push(full);
		}
	};
	// A story under a root that several places mount belongs to the first of them
	const places = readPlaces(PROJECT);
	const earlier = places.slice(0, places.findIndex(item => item.name === place)).flatMap(item => item.roots);
	for (const dir of placeRoots(PROJECT, place).filter(root => !earlier.includes(root))) {
		if (fs.existsSync(dir)) walk(dir);
	}
	return files.sort();
}

// Fixtures mount beside the frozen project's default place, as `rur render` mounts them; the fast subset
// (corpus "fast") mounts them alone, so it needs no frozen project
// The text fixtures: the ones also recorded in the quarter-pixel text mode (RUR_TEXT_SUBPIXEL=on, docs/text.md
// "Text modes"), as `edit/<name>@quarter`, so the mode that is not the default keeps its goldens too
const TEXT_FIXTURE = /^(text-|layout-(richscaled|text|truncate)|stroke-text-)/;

function fixtureEntries(fast) {
	const entries = [];
	const files = [
		...fs.readdirSync(path.join(ROOT, "fixtures")).filter(name => name.endsWith(".luau")).sort().map(name => [name, path.join(ROOT, "fixtures", name)]),
		// created-trace.luau is a standalone script, not a fixture
		...fs.readdirSync(path.join(ROOT, "tools", "engine", "probes")).filter(name => name.endsWith(".luau") && name !== "created-trace.luau").sort()
			.map(name => [`probe-${name}`, path.join(ROOT, "tools", "engine", "probes", name)]),
	];
	for (const mode of fast ? ["edit"] : ["edit", "play"]) {
		for (const [name, file] of files) {
			const base = { corpus: fast ? "fast" : "fixtures", kind: "fixture", file, roots: fast ? [path.join(ROOT, "fixtures")] : projectRoots(DEFAULT_PLACE),
				snapshots: fast ? null : snapshotsFor(DEFAULT_PLACE), options: { viewport: "1200x900", scale: "1.5", time: "1", mode }, env: {} };
			const stem = name.replace(/\.luau$/, "");
			entries.push({ ...base, name: `${mode}/${stem}` });
			const actions = file.replace(/\.luau$/, ".actions.json");
			if (fs.existsSync(actions)) entries.push({ ...base, name: `${mode}/${stem}+actions`, options: { ...base.options, actions } });
			if (mode === "edit" && TEXT_FIXTURE.test(stem)) entries.push({ ...base, name: `${mode}/${stem}@quarter`, env: { RUR_TEXT_SUBPIXEL: "on" } });
		}
	}
	return entries;
}

function storyEntries() {
	const entries = [];
	for (const place of readPlaces(PROJECT).map(item => item.name)) {
		for (const file of listStories(place)) {
			const relative = path.relative(PROJECT, file).split(path.sep).join("/").replace(/\.story\.luau$/, "");
			entries.push({ corpus: "stories", name: relative, kind: "story", file, roots: projectRoots(place), snapshots: snapshotsFor(place),
				options: { viewport: "1200x900", scale: "1.5", time: "1" }, env: {} });
		}
	}
	return entries;
}

function compareEntries() {
	const list = JSON.parse(fs.readFileSync(path.join(GOLDEN, "compares.json"), "utf8"));
	return list.map(item => {
		const dir = path.join(GOLDEN, "compares-input", item.name.replace(/^compare-/, ""));
		const file = item.file.project ? path.join(PROJECT, item.file.project) : item.file.repo ? path.join(ROOT, item.file.repo) : path.join(dir, item.file.replay);
		const options = { ...item.options };
		if (options.controls && !options.controls.trim().startsWith("{")) options.controls = path.resolve(ROOT, options.controls);
		return {
			corpus: "compares", name: item.name, kind: file.endsWith(".story.luau") ? "story" : "fixture", file, roots: projectRoots(item.place),
			snapshots: item.snapshots ? SNAPSHOTS : null, options,
			env: {
				RUR_CLOCK: item.clock ? path.join(dir, item.clock) : null,
				RUR_ORDER: item.order ? path.join(dir, item.order) : null,
				RUR_PLAYER: item.player?.name ?? null,
				RUR_PLAYER_ID: item.player?.id !== undefined && item.player?.id !== null ? String(item.player.id) : null,
				RUR_PLAYER_DISPLAY: item.player?.display ?? null,
			},
		};
	});
}

function readEvents(file) {
	return JSON.parse(fs.readFileSync(path.join(ROOT, file), "utf8"));
}

function liveEntries(fast) {
	const click = "tools/engine/live/click.events.json";
	const probeEvents = "tools/engine/probes/input.events.json";
	const cadences = [["60hz", null, 1 / 60], ["120hz", "display", 0.008333333333], ["144hz", "display", 0.006944444444]];
	const entries = [];
	const add = (file, label, options = {}) => {
		const kind = file.endsWith(".story.luau") ? "story" : "fixture";
		const inProject = !path.relative(PROJECT, file).startsWith("..");
		const place = inProject ? placeOf(PROJECT, file) : DEFAULT_PLACE;
		entries.push({ live: true, corpus: fast ? "fast" : "live", name: `${fast ? "live/" : ""}${path.basename(file).replace(/\.(story\.)?luau$/, "")}/${label}`, kind, file,
			roots: fast ? [path.join(ROOT, "fixtures")] : projectRoots(place), snapshots: fast ? null : snapshotsFor(place),
			options: { viewport: "1200x900", scale: "1.5", time: "0" }, env: {}, frames: 90, dt: 1 / 60, cadence: null, events: null, ...options });
	};
	const fixture = name => path.join(ROOT, "fixtures", name);
	if (fast) {
		add(fixture("live-scroll.luau"), "60hz-default");
		add(fixture("interaction-textbox.luau"), "120hz-click", { cadence: "display", dt: 0.008333333333, events: readEvents(click) });
		add(path.join(ROOT, "tools", "engine", "probes", "input.luau"), "60hz-input-probe", { events: readEvents(probeEvents), frames: 120 });
		return entries;
	}
	const sources = ["live-scroll.luau", "live-rotated-cull.luau", "live-stress.luau", "stories-autosize-cull.luau",
		"interaction-scroll.luau", "interaction-textbox.luau", "interaction-autobuttoncolor.luau"].map(fixture);
	for (const file of sources) {
		for (const [label, cadence, dt] of cadences) {
			add(file, `${label}-default`, { cadence, dt });
			add(file, `${label}-click`, { cadence, dt, events: readEvents(click) });
		}
		add(file, "60hz-no-input", { noInput: true });
	}
	for (const [label, cadence, dt] of cadences) {
		add(path.join(ROOT, "tools", "engine", "probes", "input.luau"), `${label}-input-probe`, { cadence, dt, events: readEvents(probeEvents), frames: 120 });
	}
	for (const file of ["live-scroll.luau", "live-rotated-cull.luau", "stories-autosize-cull.luau", "live-stress.luau"].map(fixture)) {
		add(file, "60hz-presentation", { presentation: true });
		add(file, "144hz-presentation", { presentation: true, cadence: "display", dt: 0.006944444444 });
	}
	return entries;
}

// Running the engine

let TIMEOUT = 180;
// The wall clock every run starts at (RUR_EPOCH, Unix seconds: 2026-09-26 22:40 UTC)
const EPOCH = "1790460000";
let ENGINE = null;

function run(argv, { env, input } = {}) {
	return new Promise(resolve => {
		const child = spawn(ENGINE, argv, { cwd: ROOT, env, windowsHide: true });
		const out = [], err = [];
		let timedOut = false;
		const timer = setTimeout(() => {
			timedOut = true;
			spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
			child.kill();
		}, TIMEOUT * 1000);
		child.stdout.on("data", chunk => out.push(chunk));
		child.stderr.on("data", chunk => err.push(chunk));
		child.on("error", error => resolve({ status: -1, stdout: "", stderr: String(error) }));
		child.on("close", status => {
			clearTimeout(timer);
			resolve({ status, timedOut, stdout: Buffer.concat(out).toString("utf8"), stderr: Buffer.concat(err).toString("utf8") });
		});
		child.stdin.on("error", () => {});
		child.stdin.end(input ?? "");
	});
}

// Batch workers (--no-batch turns them off): `rur-native engine-batch` reads one job a line,
// {argv, env}, and answers each with a line starting BATCH_ANSWER (engine/cli.rs batch)
const BATCH_ANSWER = "@@RUR-BATCH ";
let BATCH = true;

class BatchWorker {
	constructor() {
		this.child = null;
		this.buffer = "";
		this.waiting = null;
	}

	start() {
		const env = {};
		for (const [key, value] of Object.entries(process.env)) if (!key.startsWith("RUR_") || key === "RUR_NATIVE_ENGINE") env[key] = value;
		const child = spawn(ENGINE, ["engine-batch"], { cwd: ROOT, env, windowsHide: true });
		this.child = child;
		this.buffer = "";
		child.stdout.setEncoding("utf8");
		child.stdout.on("data", chunk => {
			this.buffer += chunk;
			let newline;
			while ((newline = this.buffer.indexOf("\n")) >= 0) {
				const line = this.buffer.slice(0, newline).replace(/\r$/, "");
				this.buffer = this.buffer.slice(newline + 1);
				if (line.startsWith(BATCH_ANSWER)) this.answer(JSON.parse(line.slice(BATCH_ANSWER.length)));
			}
		});
		child.stderr.on("data", () => {});
		child.stdin.on("error", () => {});
		child.on("error", () => {});
		child.on("close", () => {
			if (this.child === child) this.child = null;
			this.answer(null);
		});
	}

	answer(value) {
		const waiting = this.waiting;
		this.waiting = null;
		if (waiting) waiting(value);
	}

	// The run's answer ({code, error}), "timeout", or null when the worker is gone
	run(argv, env) {
		if (!this.child) this.start();
		const child = this.child;
		return new Promise(resolve => {
			const timer = setTimeout(() => {
				this.stop();
				resolve("timeout");
			}, TIMEOUT * 1000);
			this.waiting = value => {
				clearTimeout(timer);
				resolve(value);
			};
			child.stdin.write(JSON.stringify({ argv, env }) + "\n");
		});
	}

	stop() {
		const child = this.child;
		this.child = null;
		if (!child) return;
		child.stdin.end();
		spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
		child.kill();
	}

	close() {
		if (this.child) this.child.stdin.end();
		this.child = null;
	}
}

// The RUR_* part of an entry's environment (a batch job's)
function batchEnv(entry) {
	return Object.fromEntries(Object.entries(entryEnv(entry)).filter(([key]) => key.startsWith("RUR_") && key !== "RUR_NATIVE_ENGINE"));
}

// Environment of a run: only the entry's own RUR_* settings (a caller's RUR_CLOCK must not leak in)
function entryEnv(entry) {
	const env = {};
	for (const [key, value] of Object.entries(process.env)) if (!key.startsWith("RUR_") || key === "RUR_NATIVE_ENGINE") env[key] = value;
	// A fixed wall clock (os.time, DateTime.now, ...): stories print dates and countdowns
	env.RUR_EPOCH = EPOCH;
	// The frozen fonts (--freeze-fonts), not the Studio install's and the cache's
	if (FROZEN_FONTS) env.RUR_FONT_ROOT = FONTS;
	for (const [key, value] of Object.entries(entry.env)) if (value !== null && value !== undefined) env[key] = value;
	return env;
}

function engineArgs(entry, extra) {
	const argv = [...engineBaseArgs(), `--${entry.kind}`, entry.file, "--roots", entry.roots.join(";")];
	for (const [key, value] of Object.entries(entry.options)) argv.push(`--${key}`, String(value));
	if (entry.snapshots) argv.push("--snapshot-dir", entry.snapshots);
	else argv.push("--no-snapshots");
	return [...argv, ...extra];
}

// Normalization

const PLACEHOLDERS = [[PROJECT, "<project>"], [ROOT, "<repo>"]].flatMap(([dir, name]) => [
	[dir, name], [dir.split(path.sep).join("/"), name], [dir.split(path.sep).join("\\\\"), name],
]);

// The Studio install's version folder changes with every Studio update
// (…/Roblox/Versions/version-<hash>/content/, with or without a doubled slash): outputs name its content
// folder <studio-content>/ and the rest of it (ExtraContent/: BuilderIcons) <studio>/, so an update
// doesn't change every digest (2026-09-30: 81 fast entries differed only in the content path; 2026-10-08:
// 71 stories and compares in the ExtraContent path)
const STUDIO = String.raw`[A-Za-z]:[\\/]+(?:[^"\\/]+[\\/]+)*?Roblox[\\/]+Versions[\\/]+version-[0-9a-f]+[\\/]+`;
const STUDIO_CONTENT = new RegExp(`${STUDIO}content[\\\\/]+`, "gi");
const STUDIO_ROOT = new RegExp(STUDIO, "gi");

// The frozen fonts read as the files they froze: studio/ as the install, assets/ as the default cache
let FROZEN_FONTS = frozenFonts();
const fontsPath = (...parts) => new RegExp(path.join(FONTS, ...parts).split(/[\\/]+/).map(part => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join(String.raw`[\\/]+`) + String.raw`[\\/]+`, "gi");
const FONT_PLACEHOLDERS = [
	[fontsPath("studio", "content"), "<studio-content>/"],
	[fontsPath("studio"), "<studio>/"],
	[fontsPath("assets"), "<repo>/.cache/assets/"],
];

function normalizeText(text) {
	let result = text;
	for (const [from, to] of FONT_PLACEHOLDERS) result = result.replace(from, to);
	for (const [from, to] of PLACEHOLDERS) result = result.split(from).join(to);
	result = result.replace(STUDIO_CONTENT, "<studio-content>/").replace(STUDIO_ROOT, "<studio>/");
	return result.replace(/\b(table|function|thread|userdata|buffer): 0x[0-9a-fA-F]+/g, "$1: 0x?");
}

function normalizeValue(value) {
	if (typeof value === "string") return normalizeText(value);
	if (Array.isArray(value)) return value.map(normalizeValue);
	if (value && typeof value === "object") {
		const result = {};
		for (const key of Object.keys(value).sort()) result[key] = normalizeValue(value[key]);
		return result;
	}
	return value;
}

// An EditableImage a scene draws is a file named by the engine's process id and its count of such images
// (<repo>/.cache/editable-images/<pid>-<n>.png), so by the batch worker and by what it ran before: a scene
// names its editable images by their order of creation, editable-images/0.png onwards
const EDITABLE_IMAGE = /editable-images\/\d+-\d+\.png/g;
function nameEditableImages(scene) {
	if (!Array.isArray(scene.images) || !scene.images.some(uri => typeof uri === "string" && uri.search(EDITABLE_IMAGE) >= 0)) return scene;
	const text = JSON.stringify(scene);
	const count = name => Number(name.slice(name.lastIndexOf("-") + 1, -".png".length));
	const names = [...new Set(text.match(EDITABLE_IMAGE))].sort((a, b) => count(a) - count(b));
	const named = JSON.parse(text.replace(EDITABLE_IMAGE, name => `editable-images/${names.indexOf(name)}.png`));
	named.images.sort();
	return named;
}

function normalizeScene(scene) {
	const copy = nameEditableImages(normalizeValue(scene));
	if (copy.diagnostics) {
		delete copy.diagnostics.timings;
		// (advice on what to mount, made from the failure the output already records: native/src/rojo.rs)
		delete copy.diagnostics.hint;
		for (const key of ["fonts", "missingFontAssets"]) {
			if (Array.isArray(copy.diagnostics[key])) copy.diagnostics[key] = [...copy.diagnostics[key]].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
		}
	}
	return copy;
}

function normalizeReply(reply) {
	const copy = { ...reply };
	delete copy.timings;
	delete copy.stepTime;
	// The simulation profile's counters (engine::profile; counters.inputDetail / simulationDetail) are
	// profiling data like the timings: deferredJobs varies between runs of one build (handler jobs of
	// instances the collector freed or not yet, which the Luau GC schedules), scenes do not
	if (copy.counters) {
		copy.counters = { ...copy.counters };
		delete copy.counters.inputDetail;
		delete copy.counters.simulationDetail;
	}
	if (copy.diagnostics) copy.diagnostics = normalizeScene({ diagnostics: copy.diagnostics }).diagnostics;
	return normalizeValue(copy);
}

const digest = value => createHash("sha256").update(JSON.stringify(value)).digest("hex");

async function runScene(entry, worker) {
	const scratch = path.join(GOLDEN, "tmp", String(process.pid), `${entry.corpus}-${entry.name.replace(/[\\/:*?"<>|]/g, "_")}.json`);
	fs.mkdirSync(path.dirname(scratch), { recursive: true });
	fs.rmSync(scratch, { force: true });
	if (worker) {
		const answer = await worker.run(engineArgs(entry, ["--out", scratch]), batchEnv(entry));
		if (answer === "timeout") return { status: "timeout" };
		if (answer && answer.code === 0 && fs.existsSync(scratch)) {
			const scene = JSON.parse(fs.readFileSync(scratch, "utf8"));
			fs.rmSync(scratch, { force: true });
			return { status: "scene", scene: normalizeScene(scene) };
		}
		// A failed run (or a worker that went away): again in a process of its own, whose message
		// (its stderr) is the entry's
		fs.rmSync(scratch, { force: true });
	}
	const result = await run(engineArgs(entry, ["--out", scratch]), { env: entryEnv(entry) });
	if (result.timedOut) return { status: "timeout" };
	if (result.status !== 0 || !fs.existsSync(scratch)) {
		// A failed run is recorded as such (with the engine's message): it must fail the same way
		return { status: "failed", exit: result.status, message: normalizeText(result.stderr.trim().split(/\r?\n/).filter(line => !line.startsWith("rur: ")).slice(-3).join("\n")) };
	}
	const scene = JSON.parse(fs.readFileSync(scratch, "utf8"));
	fs.rmSync(scratch, { force: true });
	return { status: "scene", scene: normalizeScene(scene) };
}

function sessionReplies(stdout) {
	return stdout.split("\n").filter(line => line.startsWith("@@RUR ")).map(line => {
		try {
			return JSON.parse(line.slice(6));
		} catch (error) {
			return { type: "error", message: `unparsable reply line (${line.length} bytes): ${error.message}` };
		}
	});
}

async function runLive(entry) {
	const [width, height] = entry.options.viewport.split("x").map(Number);
	const events = entry.events ?? [
		// the retired live-protocol check's default script: hover, scroll, hover elsewhere
		{ frame: 5, type: "move", x: width / 2, y: height / 2 },
		...Array.from({ length: 12 }, (_, i) => ({ frame: 8 + i * 3, type: "wheel", notches: -1, dx: 0, dy: 100, x: width / 2, y: height / 2 })),
		{ frame: 50, type: "move", x: width / 3, y: height / 3 },
		{ frame: 60, type: "move", x: width / 4, y: height / 5 },
	];
	const commands = [];
	for (let frame = 0; frame < entry.frames; frame++) {
		const batch = events.filter(event => event.frame === frame && (!entry.noInput || event.type === "control")).map(({ frame: _, ...event }) => event);
		const presentation = entry.presentation ? { presentation: { version: 1, seq: frame, scrolls: [] } } : {};
		commands.push(JSON.stringify({ cmd: "frame", dt: entry.dt, cadence: entry.cadence ?? undefined, events: batch, ...presentation }));
		commands.push(JSON.stringify({ cmd: "scene" }));
	}
	commands.push(JSON.stringify({ cmd: "quit" }));
	const result = await run(engineArgs(entry, ["--session", "1", "--verbose"]), { env: entryEnv(entry), input: commands.join("\n") + "\n" });
	if (result.timedOut) return { status: "timeout" };
	const replies = sessionReplies(result.stdout);
	if (!replies.length) return { status: "failed", exit: result.status, message: normalizeText(result.stderr.trim().split(/\r?\n/).slice(-3).join("\n")) };
	// Presentation snapshots list every ScrollingFrame with a layout record, unparented ones included
	// until the engine's instance collector frees them (its timing follows the Luau GC): only rows of
	// frames the client holds are compared (as the former Luau-vs-Rust harness did)
	const held = new Set();
	const track = reply => {
		for (const node of asArray(reply.delta?.nodes)) held.add(node.id);
		for (const id of asArray(reply.delta?.removed)) held.delete(id);
		if (reply.presentation && Array.isArray(reply.presentation.scrolls)) {
			reply = { ...reply, presentation: { ...reply.presentation, scrolls: reply.presentation.scrolls.filter(row => held.has(row.id)) } };
		}
		return normalizeReply(reply);
	};
	const ready = track(replies[0]);
	const frames = [], scenes = [];
	let lastScene = null;
	for (const reply of replies.slice(1)) {
		if (reply.type === "scene") {
			lastScene = normalizeScene(reply.scene);
			scenes.push(digest(lastScene));
		} else frames.push(track(reply));
	}
	const output = result.stderr.split(/\r?\n/).filter(line => line.startsWith("[warn] ") || line.startsWith("[error] ")).map(normalizeText);
	return { status: "live", ready, frames, scenes, lastScene, output };
}

// Comparison (the categories of the former Luau-vs-Rust harness)

const isEmptyContainer = value => value !== null && typeof value === "object" && Object.keys(value).length === 0;
const asArray = value => (Array.isArray(value) ? value : isEmptyContainer(value) || value === undefined || value === null ? [] : [value]);

function deepEqual(a, b) {
	if (a === b) return true;
	if (isEmptyContainer(a) && isEmptyContainer(b)) return true;
	if (typeof a === "number" && typeof b === "number") return Object.is(a, b) || (Number.isNaN(a) && Number.isNaN(b));
	if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
	if (Array.isArray(a) !== Array.isArray(b)) return false;
	const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
	for (const key of keys) if (!deepEqual(a[key], b[key])) return false;
	return true;
}

const GEOMETRY_FIELDS = new Set(["rect", "clip", "matrix", "absolute", "scale", "text", "scroll", "clipOwners", "rotationCull", "culled", "rotation"]);

function compareScenes(golden, now) {
	const diffs = { status: [], output: [], structure: [], properties: [], geometry: [], layers: [], scene: [], diagnostics: [] };
	if (golden.status !== now.status) diffs.status.push({ golden: golden.status, now: now.status });
	if (golden.failure !== now.failure) diffs.status.push({ field: "failure", golden: golden.failure, now: now.failure });
	const outA = asArray(golden.diagnostics?.output), outB = asArray(now.diagnostics?.output);
	for (let index = 0; index < Math.max(outA.length, outB.length); index++) {
		if (!deepEqual(outA[index], outB[index])) diffs.output.push({ index, golden: outA[index] ?? null, now: outB[index] ?? null });
	}
	const nodesA = asArray(golden.nodes), nodesB = asArray(now.nodes);
	if (nodesA.length !== nodesB.length) diffs.structure.push({ field: "nodes", golden: nodesA.length, now: nodesB.length });
	for (let index = 0; index < Math.min(nodesA.length, nodesB.length); index++) {
		const a = nodesA[index], b = nodesB[index];
		for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
			if (deepEqual(a[key], b[key])) continue;
			const kind = GEOMETRY_FIELDS.has(key) ? "geometry" : ["id", "class", "name", "path", "parent", "order", "visible"].includes(key) ? "structure" : "properties";
			diffs[kind].push({ node: `${a.path ?? a.name} (#${index + 1})`, field: key, golden: a[key] ?? null, now: b[key] ?? null });
		}
	}
	if (!deepEqual(golden.layers, now.layers)) diffs.layers.push({ golden: golden.layers, now: now.layers });
	for (const key of ["version", "viewport", "background", "fonts", "images", "source"]) {
		if (!deepEqual(golden[key], now[key])) diffs.scene.push({ field: key, golden: golden[key] ?? null, now: now[key] ?? null });
	}
	for (const key of new Set([...Object.keys(golden.diagnostics ?? {}), ...Object.keys(now.diagnostics ?? {})])) {
		if (key !== "output" && !deepEqual(golden.diagnostics?.[key], now.diagnostics?.[key])) diffs.diagnostics.push({ field: key, golden: golden.diagnostics?.[key] ?? null, now: now.diagnostics?.[key] ?? null });
	}
	return Object.fromEntries(Object.entries(diffs).filter(([, list]) => list.length));
}

function explain(golden, now) {
	if (golden.status !== now.status) return { result: { golden: golden.status, now: now.status, message: now.message ?? golden.message } };
	if (golden.status === "scene") return compareScenes(golden.scene, now.scene);
	if (golden.status === "failed") return { failure: { golden: golden.message, now: now.message } };
	const found = {};
	if (!deepEqual(golden.ready, now.ready)) found.ready = keysDiffering(golden.ready, now.ready);
	const count = Math.max(golden.frames.length, now.frames.length);
	for (let index = 0; index < count; index++) {
		if (!deepEqual(golden.frames[index], now.frames[index])) {
			found.reply = { frame: index, keys: keysDiffering(golden.frames[index] ?? {}, now.frames[index] ?? {}) };
			break;
		}
	}
	const scene = golden.scenes.findIndex((value, index) => value !== now.scenes[index]);
	if (scene >= 0 || golden.scenes.length !== now.scenes.length) found.scene = { frame: scene >= 0 ? scene : Math.min(golden.scenes.length, now.scenes.length) };
	if (golden.lastScene && now.lastScene && !deepEqual(golden.lastScene, now.lastScene)) found.lastScene = compareScenes(golden.lastScene, now.lastScene);
	if (!deepEqual(golden.output, now.output)) {
		const at = golden.output.findIndex((line, index) => line !== now.output[index]);
		found.output = { index: at, golden: golden.output[at] ?? null, now: now.output[at] ?? null, counts: [golden.output.length, now.output.length] };
	}
	return found;
}

function keysDiffering(a, b) {
	const keys = [];
	for (const key of new Set([...Object.keys(a ?? {}), ...Object.keys(b ?? {})])) {
		if (deepEqual(a?.[key], b?.[key])) continue;
		if (key === "delta" && a?.delta && b?.delta) keys.push(...keysDiffering(a.delta, b.delta).map(inner => `delta.${inner}`));
		else keys.push(key);
	}
	return keys;
}

// Golden files

const goldenFile = entry => path.join(GOLDEN, entry.corpus, `${entry.name}.json.gz`);
const keyOf = entry => `${entry.corpus}/${entry.name}`;

// Unchanged entries (out/golden/verified.json): an entry whose output was identical to its golden output is
// not run again while nothing it reads has changed: the engine binary and its base arguments, this script,
// the entry (its file, options, environment, the files they name, a live run's script), its golden digest,
// the frozen project, snapshots and fonts (frozen.json, compares.json, the fonts manifest), data/, fixtures/,
// the probes and the Studio install (the engine looks for its @2x textures). --rerun runs every entry.
const VERIFIED = path.join(GOLDEN, "verified.json");
// Each entry's last run time (out/golden/durations.json): the longest start first, so the run does not end
// waiting on a few long ones
const DURATIONS = path.join(GOLDEN, "durations.json");

const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");

function readJson(file, fallback) {
	try {
		return JSON.parse(fs.readFileSync(file, "utf8"));
	} catch {
		return fallback;
	}
}

function writeJson(file, value) {
	const temporary = `${file}.${process.pid}.tmp`;
	fs.writeFileSync(temporary, JSON.stringify(value));
	fs.renameSync(temporary, file);
}

function fileDigest(file) {
	try {
		return sha256(fs.readFileSync(file));
	} catch {
		return null;
	}
}

function treeDigest(dir) {
	const hash = createHash("sha256");
	const walk = relative => {
		let items;
		try {
			items = fs.readdirSync(path.join(dir, relative), { withFileTypes: true });
		} catch {
			return;
		}
		for (const item of items.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))) {
			const child = path.join(relative, item.name);
			if (item.isDirectory()) walk(child);
			else if (item.isFile()) hash.update(child).update("\0").update(fs.readFileSync(path.join(dir, child))).update("\0");
		}
	};
	walk("");
	return hash.digest("hex");
}

// What every entry of this run reads besides its own files
function runContext() {
	const text = file => {
		try {
			return fs.readFileSync(file, "utf8");
		} catch {
			return null;
		}
	};
	return sha256(JSON.stringify({
		engine: fileDigest(ENGINE),
		arguments: engineBaseArgs(),
		script: fileDigest(fileURLToPath(import.meta.url)),
		frozen: text(path.join(GOLDEN, "frozen.json")),
		compares: text(path.join(GOLDEN, "compares.json")),
		fonts: text(path.join(FONTS, "manifest.json")),
		data: treeDigest(path.join(ROOT, "data")),
		fixtures: treeDigest(path.join(ROOT, "fixtures")),
		probes: treeDigest(path.join(ROOT, "tools", "engine", "probes")),
		studio: studioContentRoots(),
	}));
}

function entryDigest(entry, context, expected) {
	const relative = file => path.relative(ROOT, file).split(path.sep).join("/");
	const named = [entry.file, entry.options.actions, entry.options.controls, entry.env.RUR_CLOCK, entry.env.RUR_ORDER].filter(value => typeof value === "string" && value);
	const live = entry.live ? { frames: entry.frames, dt: entry.dt, cadence: entry.cadence, events: entry.events, noInput: Boolean(entry.noInput), presentation: Boolean(entry.presentation) } : null;
	return sha256(JSON.stringify({
		context, expected, key: keyOf(entry), kind: entry.kind, file: relative(entry.file), roots: entry.roots.map(relative),
		snapshots: entry.snapshots ? relative(entry.snapshots) : null, options: entry.options, env: entry.env, live,
		files: named.map(file => [file, fileDigest(file)]),
	}));
}

function readManifest() {
	try {
		return JSON.parse(fs.readFileSync(MANIFEST, "utf8"));
	} catch {
		return { entries: {} };
	}
}

function writeManifest(manifest) {
	const entries = Object.fromEntries(Object.keys(manifest.entries).sort().map(key => [key, manifest.entries[key]]));
	fs.writeFileSync(MANIFEST, JSON.stringify({ ...manifest, entries }, null, "\t") + "\n");
}

function summarize(found, limit) {
	const lines = [];
	for (const [kind, list] of Object.entries(found)) {
		const items = Array.isArray(list) ? list : [list];
		lines.push(`    ${kind}${Array.isArray(list) ? `: ${list.length}` : ""}`);
		for (const item of items.slice(0, limit)) lines.push(`      ${JSON.stringify(item).slice(0, 400)}`);
	}
	return lines.join("\n");
}

async function main() {
	const args = parseArgs(process.argv.slice(2));
	if (args.timeout && args.timeout !== true) TIMEOUT = Number(args.timeout);
	if (args["no-batch"]) BATCH = false;
	if (args.freeze || args["freeze-fonts"]) {
		const studio = typeof args["freeze-fonts"] === "string" ? args["freeze-fonts"] : undefined;
		if (args.freeze) freeze(studio);
		else freezeFonts(studio);
		// Freezing alone (before recording): nothing else to do
		if (!args.write) return;
	}
	ENGINE = await ensureEngine();
	console.log(FROZEN_FONTS ? `fonts: ${path.relative(ROOT, FONTS)} (frozen ${FROZEN_FONTS.frozen} from ${path.basename(FROZEN_FONTS.studio)})`
		: "fonts: the Studio install's and the asset cache's (not frozen: --freeze-fonts)");
	const corpora = args.all ? ["fixtures", "stories", "compares", "live"]
		: args.corpus && args.corpus !== true ? String(args.corpus).split(",") : ["fixtures", "live"];
	const fast = Boolean(args.fast);
	let entries = [];
	for (const corpus of fast ? ["fixtures", "live"] : corpora) {
		if (!fast && !frozen()) {
			console.log(`${corpus}: skipped (no frozen project in ${path.relative(ROOT, GOLDEN)}; record one with --write --freeze)`);
			continue;
		}
		if (corpus === "fixtures") entries.push(...fixtureEntries(fast));
		else if (corpus === "stories") entries.push(...storyEntries());
		else if (corpus === "compares") entries.push(...compareEntries());
		else if (corpus === "live") entries.push(...liveEntries(fast));
		else throw new Error(`unknown corpus ${corpus}`);
	}
	const filter = args.filter && args.filter !== true ? String(args.filter).toLowerCase() : null;
	if (filter) entries = entries.filter(entry => keyOf(entry).toLowerCase().includes(filter));
	const manifest = readManifest();
	const write = Boolean(args.write);
	if (!write) fs.rmSync(path.join(GOLDEN, "failed"), { recursive: true, force: true });
	const limit = args.verbose ? 40 : 4;
	// Engine runs are separate processes: about half the cores by default (RUR_JOBS / --jobs override)
	const jobs = Math.max(1, Number(args.jobs) || Number(process.env.RUR_JOBS) || Math.max(6, Math.min(16, Math.floor(os.availableParallelism() / 2))));
	const started = Date.now();
	let same = 0, differ = 0, missing = 0, unchanged = 0, next = 0, printed = 0;
	const results = new Array(entries.length);
	const print = () => {
		while (printed < entries.length && results[printed]) {
			const { entry, verdict, detail } = results[printed++];
			if (verdict !== "same" || args.verbose) console.log(`${keyOf(entry)}: ${verdict}${detail ? `\n${detail}` : ""}`);
		}
	};
	// Entries verified identical by an earlier run of this engine on these inputs are not run again
	const reuse = !write && !args.rerun;
	const verified = readJson(VERIFIED, { schema: 1, entries: {} });
	if (verified.schema !== 1 || typeof verified.entries !== "object") Object.assign(verified, { schema: 1, entries: {} });
	const context = runContext();
	const digests = entries.map(entry => (manifest.entries[keyOf(entry)] ? entryDigest(entry, context, manifest.entries[keyOf(entry)].digest) : null));
	// Longest first by the last run's times, live sessions first among entries never timed (results are
	// still reported in entry order)
	const durations = readJson(DURATIONS, {});
	const estimate = index => durations[keyOf(entries[index])] ?? (entries[index].live ? Infinity : 0);
	const order = entries.map((_, index) => index).sort((a, b) => estimate(b) - estimate(a) || a - b);
	await Promise.all(Array.from({ length: Math.min(jobs, entries.length) }, async () => {
		const worker = BATCH ? new BatchWorker() : null;
		try {
		while (next < entries.length) {
			const index = order[next++];
			const entry = entries[index];
			const key = keyOf(entry);
			if (reuse && digests[index] && verified.entries[key] === digests[index]) {
				same++;
				unchanged++;
				results[index] = { entry, verdict: "same", detail: "" };
				print();
				continue;
			}
			const t0 = performance.now();
			let now = entry.live ? await runLive(entry) : await runScene(entry, worker);
			if (write || digest(now) !== manifest.entries[key]?.digest) now = await withCloudFonts(entry, now);
			durations[key] = Math.round(performance.now() - t0);
			const value = digest(now);
			let verdict, detail = "";
			delete verified.entries[key];
			if (write) {
				fs.mkdirSync(path.dirname(goldenFile(entry)), { recursive: true });
				fs.writeFileSync(goldenFile(entry), zlib.gzipSync(JSON.stringify(now)));
				manifest.entries[key] = { digest: value, status: now.status === "scene" ? now.scene.status : now.status };
				verdict = `recorded (${manifest.entries[key].status})`;
				same++;
			} else if (!manifest.entries[key]) {
				verdict = "no golden (record with --write)";
				missing++;
			} else if (manifest.entries[key].digest === value) {
				verdict = "same";
				same++;
				if (digests[index]) verified.entries[key] = digests[index];
			} else {
				differ++;
				const file = goldenFile(entry);
				if (fs.existsSync(file)) {
					const golden = JSON.parse(zlib.gunzipSync(fs.readFileSync(file)).toString("utf8"));
					verdict = digest(golden) === manifest.entries[key].digest ? "DIFFERS" : "DIFFERS (out/golden is not the recorded output: digest only)";
					detail = summarize(explain(golden, now), limit);
				} else {
					verdict = "DIFFERS (digest; no recorded output in out/golden to explain it)";
				}
				const absent = missingFonts(now).map(id => `${id}${fontFailures.has(id) ? ` (${fontFailures.get(id)})` : ""}`);
				if (absent.length) verdict += `; cloud fonts it could not get: ${absent.join(", ")}`;
				fs.mkdirSync(path.join(GOLDEN, "failed", entry.corpus), { recursive: true });
				fs.writeFileSync(path.join(GOLDEN, "failed", entry.corpus, `${entry.name.replace(/[\\/]/g, "__")}.json.gz`), zlib.gzipSync(JSON.stringify(now)));
			}
			results[index] = { entry, verdict, detail };
			print();
		}
		} finally {
			worker?.close();
		}
	}));
	if (write) {
		manifest.recorded = new Date().toISOString();
		if (frozen()) manifest.frozen = JSON.parse(fs.readFileSync(path.join(GOLDEN, "frozen.json"), "utf8"));
		if (FROZEN_FONTS) manifest.fonts = { frozen: FROZEN_FONTS.frozen, studio: path.basename(FROZEN_FONTS.studio), files: Object.keys(FROZEN_FONTS.files).length };
		writeManifest(manifest);
	}
	fs.rmSync(path.join(GOLDEN, "tmp", String(process.pid)), { recursive: true, force: true });
	writeJson(VERIFIED, verified);
	writeJson(DURATIONS, durations);
	const seconds = ((Date.now() - started) / 1000).toFixed(1);
	console.log(`\n${write ? `${same} recorded` : `${same}/${entries.length} identical to the golden output${differ ? `, ${differ} differ (now: out/golden/failed)` : ""}${missing ? `, ${missing} without golden` : ""}`} in ${seconds} s`
		+ (unchanged ? ` (${unchanged} not run again: identical in an earlier run of this engine on unchanged inputs; --rerun runs them)` : ""));
	process.exit(differ || missing ? 1 : 0);
}

main().catch(error => {
	console.error(error?.stack || error);
	process.exit(1);
});
