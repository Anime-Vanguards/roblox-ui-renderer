#!/usr/bin/env node
// rur — offline Roblox UI renderer.
//
//   rur render --story <file.story.luau> [--out dir] [--viewport 1920x1080] [--scale 1.5] [--time 1]
//   rur render --model <file.rbxm|.rbxmx> ...          render a saved instance tree
//   rur render --module <file.luau> ...                 require a module (function(target) or Instance)
//   rur sequence --story <file> [--from 0] [--to 4 | --frames 240 | --duration 4] [--fps 60] [--crop <node>|x,y,w,h]
//             [--pad px] [--no-png] [--no-video] [--scenes]  the story mounted once and captured frame by frame at
//                                                       exact moments: frames/, trace.json, video.mp4, video-quarter.mp4
//   rur stories [--filter a|b] [--place <name>|all]     list storybook stories in the project
//   rur sweep [--filter a|b] [--jobs N] [--timeout s] [--no-png] [--out out/sweep]
//                                                       render every story (every place), report failures
//             [--failed]  re-run the non-clean stories of the last report, [--limit N], [--retries 1],
//             [--summarize]  rebuild report totals + summary.md without rendering
//   rur compare --story <file> [--studio <name>]       render in Studio + offline, diff layout & pixels
//   rur doctor [--fix]                                  check the toolchain (--fix: remove build leftovers)
//   rur clean [--older-than <days>] [--dry-run]         shrink the output root: copied images to hard links,
//                                                       retired files out; --older-than: stale frames/ and
//                                                       traces too (cli/clean.mjs)
//   rur assets warm [--refresh]                         cache every image an open Studio's place uses: downloads,
//                                                       and private ones read from Studio (docs/assets.md)
//   rur selftest                                        run offline assertion checks
//   rur native [--story <file>]                         the desktop storybook (native/, wgpu)
//   rur native --headless --scene <scene.json>           a saved scene to PNG (--batch: many)
//   rur diff --git-ref <ref> --story <file> [--out dir]  render at <ref> and in the working tree, diff them
//   rur record --harness <file.luau> [--place <name>] [--seconds 3] [--fps 60] [--out dir]
//             [--context <json|file>] [--assets-at <Service.Path>] [--assets <prefix>,...] [--mode play|edit]
//                                                       run a harness on the project frame by frame and record
//                                                       the 3D world: scene.rbxm + playback.json (ror), events.json
//   rur profile diff <before> <after> | report <dir>    compare two --profile directories, rewrite a profile's page
//   rur profile view <dir> [<dir2>]                    explore a profile (or a before/after pair) in the terminal
//   rur profile render <dir> --view <name|all> --out <file.png|.svg|.txt|dir|->   the viewer's screens as files
//
// Global: --state-dir <dir> (or RUR_STATE_DIR): caches, builds, logs go there instead of .cache/;
// RUR_OUT_DIR replaces out/ as the default output root (cli/paths.mjs). Renders go through the render
// service (cli/client.mjs): the same files and pixels, without loading the engine and project each time,
// and a render of a later moment of the last render's story goes on from its run. Two switches run
// without it, each for one reason only; each render then costs a process, an engine and a project load
// (seconds): --isolated (RUR_ISOLATED=1), a frozen engine while native/ is being edited (another agent's
// work, a bisect): no service is started or contacted and the engine is the state dir's build as it is;
// --no-service (RUR_NO_SERVICE=1), debugging the service itself, or a story that must be killable on its
// own (the sweep's children, tests that need a state dir of their own).
// --actions '<JSON list>' | <file>: timed input (move, down, up, wheel, leave, key, text, paste, and
// click / drag by coordinates or node path), checked first: a bad list exits 64 (cli/actions.mjs).
// --profile <kinds> [--profile-out <dir>] (render, sequence, record): the engine's profiling suite (work, caches,
// alloc, heap, timeline, cpu, phases, time, boundary or all: roblox-engine-emulator's docs/cli.md "Profiles and
// caches"), with the layout's and text's caches among the caches, written to <out>/profile: summary.txt (printed on
// stderr), profile.json, report.html, folded stacks. A profiled render or sequence runs in a process of its own.
// `rur doctor --fix` removes build leftovers from the state dir (superseded engine binaries, retired
// Python / FreeType installs).
// Common options: --project <dir> (RUR_PROJECT; default examples/project; its places come from its rur.json,
// cli/places.mjs), --roots "<dir>;<dir>", --place <name> (default: the place whose roots hold the story, else the
// first), --offline (no network), --placeholders (render: draw a labelled checkerboard for each missing image;
// it is still listed in missingImages), --no-png, --background r,g,b (0-255) | transparent, --mode edit|play,
// --device desktop|touch|gamepad (what UserInputService reports: TouchEnabled, MouseEnabled, GetLastInputType; default desktop),
// --player <name>, --place-id <n> (game.PlaceId; default: the rur.json placeIds entry of the roots), --snapshot-dir <dir>
// (Studio-only templates exported from Studio, loaded only when given; see snapshotDir) / --no-snapshots.
// Rojo projects: --rojo <file.project.json> (or --project <file.project.json>) [--rojo-at Service/Name],
// --mount Service/Name=<dir|file> (repeatable). --git-ref <ref>: the sources as they are at a commit.
// --load Service/Name=<file.rbxm> (repeatable): a saved model's top-level instances (a SerializationService
// blob) in a Folder at that path for the story to clone, keeping the default project. --all-layers: draw every enabled
// ScreenGui the run leaves under the PlayerGui and CoreGui (DisplayOrder, then the order they were
// parented or enabled), not only the story gui. --topbar-inset <px>: GuiService's top inset (default 0, as
// in Studio's Edit mode), which ScreenGuis with IgnoreGuiInset false start below; scripts read AbsolutePosition
// from below it, as in a Roblox client.
// --viewport WxH (logical px), WxH@physical (divided by --scale) or --viewport-from <png> (its size,
// physical px). --plugin [settings JSON]: a Studio `plugin` global with dock widgets. --json: stdout
// holds only the JSON result (one line). --tag <name> (<out>/<name>/), --keep (numbered <out>-2, ...
// instead of replacing an earlier render).

import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawnSync, spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

// First: applies the global --state-dir flag before anything resolves a path (cli/paths.mjs)
import { OUT_DIR, sharedStatePath, stateDir } from "./paths.mjs";
import { hasCookie, studioContentRoots, CACHE_DIR } from "./assets.mjs";
import { engineBaseArgs, ensureEngine } from "./engine.mjs";
import { mountArgs, rojoFileOf, runMounts } from "./rojo.mjs";
import { readActions, UsageError } from "./actions.mjs";
import { diagnosticLines, sceneDiagnostics } from "./diagnostics.mjs";
import { emulatorLink, pinnedCommit } from "../tools/emulator.mjs";
import { placeOf, placeRoots, readPlaces } from "./places.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// The project a run mounts beside its story (docs/projects.md): --project, else RUR_PROJECT, else the example
// project in this checkout. Its places (named sets of source roots) come from its rur.json (cli/places.mjs).
const SNAPSHOT_PROJECT = null;
const DEFAULT_PROJECT = path.resolve(process.env.RUR_PROJECT || path.join(ROOT, "examples", "project"));
// Studio snapshots load only when --snapshot-dir names one: no place has an implicit one
export const IMPLICIT_SNAPSHOTS = {};

// Options that may be given more than once (collected into a list)
const REPEATABLE = new Set(["mount", "load"]);

export function parseArgs(argv) {
	const args = { _: [] };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg.startsWith("--")) {
			const key = arg.slice(2);
			const next = argv[i + 1];
			let value = true;
			if (next !== undefined && !next.startsWith("--")) {
				value = next;
				i++;
			}
			if (REPEATABLE.has(key) && value !== true) args[key] = [...(Array.isArray(args[key]) ? args[key] : []), value];
			else args[key] = value;
		} else args._.push(arg);
	}
	return args;
}

// Whether the run's project comes from --rojo / --mount instead of roots
const hasMounts = args => Boolean(rojoFileOf(args)) || (args.mount !== undefined && args.mount !== true);

// --viewport as logical "WxH": WxH@physical (or --viewport-from <png>, its pixel size) is divided by
// --scale (default 1.5) and must come out whole.
export function resolveViewport(args) {
	const scale = Number(args.scale) || 1.5;
	let physical = null;
	let what = "";
	if (args["viewport-from"] && args["viewport-from"] !== true) {
		const file = path.resolve(String(args["viewport-from"]));
		const header = Buffer.alloc(24);
		const fd = fs.openSync(file, "r");
		try {
			fs.readSync(fd, header, 0, 24, 0);
		} finally {
			fs.closeSync(fd);
		}
		if (header.toString("latin1", 1, 4) !== "PNG" || header.toString("latin1", 12, 16) !== "IHDR") throw new Error(`--viewport-from ${file}: not a PNG`);
		physical = [header.readUInt32BE(16), header.readUInt32BE(20)];
		what = `--viewport-from ${path.basename(file)} (${physical[0]}x${physical[1]} physical px)`;
	} else if (args.viewport && args.viewport !== true) {
		const match = /^\s*(\d+(?:\.\d+)?)\s*[xX]\s*(\d+(?:\.\d+)?)\s*@\s*physical\s*$/i.exec(String(args.viewport));
		if (!match) return String(args.viewport);
		physical = [Number(match[1]), Number(match[2])];
		what = `--viewport ${args.viewport}`;
	} else {
		return args.viewport;
	}
	const logical = physical.map(value => value / scale);
	if (logical.some(value => Math.abs(value - Math.round(value)) > 1e-6)) {
		throw new Error(`${what} is ${logical.map(value => +value.toFixed(3)).join("x")} logical px at --scale ${scale}: not whole pixels (use a size divisible by the scale, or another --scale)`);
	}
	return logical.map(Math.round).join("x");
}

export function projectRoots(args) {
	if (args.roots) return String(args.roots).split(";").filter(Boolean);
	// A Rojo project / explicit mounts replace the default project's roots
	if (hasMounts(args)) return [];
	const project = path.resolve(args.project || DEFAULT_PROJECT);
	// --place, else the place whose roots hold the story (or module, model, fixture), else the default place
	const source = ["story", "module", "model", "fixture"].map(key => args[key]).find(value => value && value !== true);
	const place = args.place && args.place !== true ? String(args.place) : source ? placeOf(project, path.resolve(String(source))) : undefined;
	const roots = placeRoots(project, place);
	return roots.filter(dir => fs.existsSync(dir));
}

// Studio-only instances the stories reach (GUI templates kept in ServerStorage and the like), exported
// from the open Studio by tools/sweep/studio-assets.mjs into .cache/snapshots/<studio>/. Runs load their
// place's snapshot (IMPLICIT_SNAPSHOTS) when present, as `rur compare` does; --snapshot-dir <dir> picks
// another directory and --no-snapshots none.
export function snapshotDir(args) {
	if (args["snapshot-dir"] && args["snapshot-dir"] !== true) {
		return path.resolve(String(args["snapshot-dir"]));
	}
	const place = args.place && args.place !== true ? String(args.place) : undefined;
	if (args["no-snapshots"] || args.roots || hasMounts(args) || !IMPLICIT_SNAPSHOTS[place]) {
		return undefined;
	}
	// Implicit snapshots belong to SNAPSHOT_PROJECT alone; other projects opt in with --snapshot-dir.
	const project = path.resolve(String(args.project || DEFAULT_PROJECT));
	const normalize = value => process.platform === "win32" ? value.toLowerCase() : value;
	if (normalize(project) !== normalize(SNAPSHOT_PROJECT)) {
		return undefined;
	}
	const dir = sharedStatePath(path.join("snapshots", args.studio && args.studio !== true ? String(args.studio) : IMPLICIT_SNAPSHOTS[place]));
	return fs.existsSync(dir) ? dir : undefined;
}

// --load Service/Name=<file.rbxm|.rbxmx> (repeatable, or ";"-separated) as engine arguments: a saved
// model, such as a SerializationService blob, whose top-level instances the engine puts in a Folder at
// that path before the story runs, for a fixture to clone (story.rs load_models). Unlike --mount it
// keeps the default project and its Studio snapshots.
export function loadArgs(args) {
	const loads = args.load === undefined || args.load === true ? [] : (Array.isArray(args.load) ? args.load : [args.load]);
	return loads.flatMap(value => String(value).split(";")).filter(Boolean).flatMap(item => {
		const index = item.indexOf("=");
		const at = index > 0 ? item.slice(0, index).split(/[/\\]/).filter(Boolean) : [];
		const file = index > 0 ? path.resolve(item.slice(index + 1)) : "";
		if (at.length < 2) throw new UsageError(`--load ${item}: expected Service/Name=<file.rbxm>`);
		if (!/\.rbxmx?$/i.test(file)) throw new UsageError(`--load ${item}: not an .rbxm / .rbxmx file`);
		if (!fs.existsSync(file)) throw new UsageError(`--load ${item}: ${file} does not exist`);
		return ["--load", `${at.join("/")}=${file}`];
	});
}

// The engine arguments of a run (`rur-native engine` / `render`, cli/engine.mjs): the renderer
// checkout, project roots, the source and the run's options. `scenePath`: also --out <scene.json> and
// --dependencies (a bare engine run); `rur-native render` writes both into its --dir itself.
export function runtimeArgs(args, scenePath = null) {
	const result = [...engineBaseArgs(), "--roots", projectRoots(args).join(";"), ...mountArgs(runMounts(args)), ...loadArgs(args)];
	if (scenePath) result.push("--out", scenePath);
	args = { ...args, viewport: resolveViewport(args) };
	if (args.plugin !== undefined && args.plugin !== false) {
		// --plugin: empty settings, or a JSON object / file seeding plugin:GetSetting
		const value = args.plugin === true ? "1" : String(args.plugin);
		result.push("--plugin", value.trim().startsWith("{") || value === "1" || value === "0" ? value : path.resolve(value));
	}
	for (const key of ["story", "module", "model", "fixture", "viewport", "scale", "time", "mode", "player", "background", "place-name", "snapshot-dir", "actions", "controls", "camera", "place-id", "rotation-clipping", "topbar-inset", "device"]) {
		if (args[key] !== undefined && args[key] !== true) {
			// --actions / --controls: a JSON file or inline JSON
			const inline = key === "actions" ? "[" : "{";
			const isPath = ["story", "module", "model", "fixture", "snapshot-dir"].includes(key) || ((key === "actions" || key === "controls") && !String(args[key]).trim().startsWith(inline));
			const value = isPath ? path.resolve(String(args[key])) : String(args[key]);
			result.push(`--${key === "place-name" ? "place" : key}`, value);
		}
	}
	// --all-layers: every enabled ScreenGui under the PlayerGui and CoreGui is drawn, not only the story gui
	if (args["all-layers"] !== undefined && args["all-layers"] !== false && String(args["all-layers"]) !== "0") result.push("--all-layers", "1");
	if (args.verbose) result.push("--verbose");
	// The source files the run required (rur compare keys its Studio captures on them)
	if (scenePath) result.push("--dependencies", path.join(path.dirname(scenePath), "dependencies.json"));
	return result;
}

// --profile <kinds> [--profile-out <dir>] [--profile-top <n>] [--profile-phase <a,b|all>] [--profile-alloc-step <bytes>]
// as `rur-native render`, `sequence` and `record` take them: the engine's profiling suite (roblox-engine-emulator's
// docs/cli.md "Profiles and caches"). A profiled render or sequence runs in a process of its own, never through the
// render service; the profile goes to <out>/profile unless --profile-out names a directory.
export function profileArgs(args) {
	if (args.profile === undefined || args.profile === false) return [];
	const result = ["--profile", ...(args.profile === true ? [] : [String(args.profile)])];
	for (const key of ["profile-out", "profile-top", "profile-phase", "profile-alloc-step"]) {
		if (args[key] !== undefined && args[key] !== true) result.push(`--${key}`, key === "profile-out" ? path.resolve(String(args[key])) : String(args[key]));
	}
	return result;
}

// One `rur render` output directory in a process of its own (`rur-native render`, native/src/story_render.rs):
// the run, scene.json, dependencies.json, the glyph atlas, the images, manifest.json and (unless
// `png` is false) render.png. Resolves with its result ({ status, nodes, phases, renderErrors }).
export async function renderNative(argv, outDir, { png = true, offline = false, placeholders = false, executable } = {}) {
	executable ||= await ensureEngine();
	const renderArgs = ["render", "--dir", outDir, ...(png ? [] : ["--no-png"]), ...(offline ? ["--offline"] : []), ...(placeholders ? ["--placeholders"] : []), ...argv];
	return new Promise((resolve, reject) => {
		const child = spawn(executable, renderArgs, { cwd: ROOT, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
		let stdout = "";
		let stderr = "";
		child.stdout.on("data", chunk => {
			stdout += chunk;
		});
		child.stderr.on("data", chunk => {
			stderr += chunk;
			process.stderr.write(chunk);
		});
		child.on("error", reject);
		child.on("close", code => {
			const line = stdout.trim().split(/\r?\n/).pop();
			try {
				if (code === 0) return resolve(JSON.parse(line));
			} catch {
				// reported below
			}
			reject(new Error(`runtime failed (exit ${code})${stderr ? `\n${stderr.slice(-4000)}` : ""}`));
		});
	});
}

// `rur record`: a harness script (returning function(Context)) run against the project in play mode,
// the 3D world recorded frame by frame by the engine (the emulator's recorder: `rur-native record`)
// into `<out>/scene.rbxm`, `playback.json` (ror's playback), `events.json`, `camera.json` and
// `summary.json`. The game place by default (as game stories); the summary is the result.
export async function record(args) {
	const harness = args.harness && args.harness !== true ? path.resolve(String(args.harness)) : null;
	if (!harness) throw new UsageError("record needs --harness <file.luau>");
	if (!fs.existsSync(harness)) throw new UsageError(`record: ${harness} does not exist`);
	const name = path.basename(harness).replace(/\.luau?$/, "");
	const outDir = renderOutDir(args, path.join("record", name));
	const place = args.place && args.place !== true ? String(args.place) : readPlaces(path.resolve(args.project && args.project !== true ? String(args.project) : DEFAULT_PROJECT))[0].name;
	const runArgs = { ...args, place };
	const argv = ["record", "--repo", ROOT, "--roots", projectRoots(runArgs).join(";"), ...mountArgs(runMounts(runArgs)), "--harness", harness, "--out", outDir];
	for (const key of ["seconds", "fps", "assets-at", "assets", "mode", "player", "place-id"]) {
		if (args[key] !== undefined && args[key] !== true) argv.push(`--${key}`, String(args[key]));
	}
	if (args.context !== undefined && args.context !== true) {
		const context = String(args.context);
		argv.push("--context", context.trim().startsWith("{") ? context : path.resolve(context));
	}
	const snapshots = snapshotDir(runArgs);
	if (snapshots) argv.push("--snapshot-dir", snapshots);
	if (args.offline) argv.push("--offline");
	if (args.verbose) argv.push("--verbose");
	argv.push(...profileArgs(args));
	const executable = await ensureEngine();
	return new Promise((resolve, reject) => {
		const child = spawn(executable, argv, { cwd: ROOT, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
		let stdout = "";
		let stderr = "";
		child.stdout.on("data", chunk => {
			stdout += chunk;
		});
		child.stderr.on("data", chunk => {
			stderr += chunk;
			process.stderr.write(chunk);
		});
		child.on("error", reject);
		child.on("close", code => {
			const line = stdout.trim().split(/\r?\n/).pop();
			try {
				if (code === 0) return resolve(JSON.parse(line));
			} catch {
				// reported below
			}
			reject(new Error(`record failed (exit ${code})${stderr ? `\n${stderr.slice(-4000)}` : ""}`));
		});
	});
}

function asArray(value) {
	return Array.isArray(value) ? value : [];
}

// The output directory of a render: --out, else out/<name>; --tag <tag> puts it in a <tag>
// subdirectory of that, and --keep picks a numbered sibling (<dir>-2, -3, ...) instead of
// replacing an earlier render there. The summary's `out` is the directory used.
export function renderOutDir(args, name) {
	let dir = path.resolve(args.out && args.out !== true ? String(args.out) : path.join(OUT_DIR, name));
	if (args.tag && args.tag !== true) dir = path.join(dir, String(args.tag).replace(/[<>:"/\\|?*\x00-\x1f]/g, "_"));
	if (args.keep) {
		const used = candidate => fs.existsSync(path.join(candidate, "summary.json")) || fs.existsSync(path.join(candidate, "scene.json"));
		let candidate = dir;
		for (let index = 2; used(candidate); index++) candidate = `${dir}-${index}`;
		dir = candidate;
	}
	return dir;
}

// A render: the run and the output directory (renderNative), through the render service when `service`
// (cli/client.mjs; the same files and pixels, without the start-up costs), then summary.json.
// `--scene <scene.json>` draws a saved (possibly hand-edited) scene without running any Luau.
export async function render(args, { service = false, env = process.env } = {}) {
	// --viewport WxH@physical / --viewport-from <png>: logical size (errors before anything runs)
	if (args.viewport !== undefined || args["viewport-from"] !== undefined) args = { ...args, viewport: resolveViewport(args) };
	const source = args.story || args.module || args.model || args.fixture || args.scene;
	if (!source) throw new Error("render needs --story, --module, --model, --fixture or --scene");
	// A bad --actions list is a usage error before anything runs or is written (cli/actions.mjs)
	if (args.actions !== undefined && !args.scene) readActions(args.actions, { time: args.time !== undefined && args.time !== true ? Number(args.time) : 1 });
	const name = path.basename(String(source)).replace(/\.(story\.)?luau$|\.rbxmx?$|\.json$/, "");
	const outDir = renderOutDir(args, name);
	fs.mkdirSync(outDir, { recursive: true });

	const started = Date.now();
	let argv;
	let gitRef = null;
	if (args.scene) {
		// Re-render an existing (possibly hand-edited) scene.json without running the runtime
		const scenePath = path.join(outDir, "scene.json");
		if (path.resolve(String(args.scene)) !== scenePath) fs.copyFileSync(path.resolve(String(args.scene)), scenePath);
		argv = [...engineBaseArgs(), "--scene", scenePath];
	} else {
		let runArgs = { ...args, "snapshot-dir": snapshotDir(args) };
		if (args["git-ref"] && args["git-ref"] !== true) {
			// The project's sources at a commit, extracted into the state dir (cli/gitref.mjs)
			const { applyGitRef } = await import("./gitref.mjs");
			const applied = applyGitRef(runArgs, { roots: projectRoots(runArgs), rojoFile: rojoFileOf(runArgs) });
			runArgs = applied.args;
			runMounts(runArgs, { onPath: source => applied.ref.map(source) });
			gitRef = runArgs.gitRef;
		}
		argv = runtimeArgs(runArgs);
	}
	const options = { png: !args["no-png"], offline: Boolean(args.offline), placeholders: Boolean(args.placeholders) };
	let result = null;
	// (--verbose mirrors the story's output on stderr, and a profile is a run's own: a process of its own)
	const profile = profileArgs(args);
	if (service && !args.scene && !args.verbose && !profile.length) {
		const { renderInService } = await import("./client.mjs");
		result = await renderInService(argv, outDir, options, { env });
	}
	result ||= await renderNative([...profile, ...argv], outDir, options);

	const scene = JSON.parse(fs.readFileSync(path.join(outDir, "scene.json"), "utf8"));
	let manifest = { images: {} };
	try {
		manifest = JSON.parse(fs.readFileSync(path.join(outDir, "manifest.json"), "utf8"));
	} catch {
		// no manifest: nothing to report as missing
	}
	const missingImages = Object.entries(manifest.images || {}).filter(([, entry]) => entry.error);
	const output = asArray(scene.diagnostics && scene.diagnostics.output);
	const summary = {
		status: scene.status,
		failure: scene.failure || null,
		out: outDir,
		png: args["no-png"] ? null : path.join(outDir, "render.png"),
		nodes: asArray(scene.nodes).length,
		viewport: scene.viewport,
		scriptErrors: output.filter(entry => entry.kind === "error").length,
		warnings: output.filter(entry => entry.kind === "warn").length,
		unemulated: asArray(scene.diagnostics && scene.diagnostics.unemulated).map(entry => entry.member),
		missingImages: missingImages.map(([uri, entry]) => `${uri}: ${entry.error}`),
		missingFonts: asArray(scene.diagnostics && scene.diagnostics.missingFontAssets),
		// What the render left out or could not draw (cli/diagnostics.mjs): culled nodes, wrapped text
		// lines the box dropped, glyphs no font has, and the script's print / warn output
		...sceneDiagnostics(scene).summary,
		renderErrors: asArray(result.renderErrors),
		seconds: (Date.now() - started) / 1000,
		phases: result.phases || {},
	};
	if (gitRef) summary.gitRef = gitRef;
	// (--profile: the profile's directory; its summary.txt was printed on stderr)
	if (result.profile) summary.profile = result.profile;
	fs.writeFileSync(path.join(outDir, "summary.json"), JSON.stringify(summary, null, 2));
	return summary;
}

// A sequence's own options (native/src/sequence.rs parse_options), passed as they are
export const SEQUENCE_OPTIONS = ["from", "to", "frames", "duration", "fps", "crop", "pad"];

// The frames a sequence's options ask for (sequence.rs parse_options): --frames, else (--to - --from) * --fps,
// else --duration * --fps, else one second
export function sequenceFrames(options) {
	const fps = options.fps === undefined ? 60 : Number(options.fps);
	const from = options.from === undefined ? 0 : Number(options.from);
	if (options.frames !== undefined) return Math.round(Number(options.frames));
	if (options.to !== undefined) return Math.round((Number(options.to) - from) * fps);
	if (options.duration !== undefined) return Math.round(Number(options.duration) * fps);
	return Math.round(fps);
}

// `rur-native sequence` in a process of its own; its result
export async function sequenceNative(argv, outDir, options, { offline = false, executable } = {}) {
	executable ||= await ensureEngine();
	const flags = [];
	for (const key of SEQUENCE_OPTIONS) if (options[key] !== undefined) flags.push(`--${key}`, String(options[key]));
	if (options.png === false) flags.push("--no-png");
	if (options.video === false) flags.push("--no-video");
	if (options.scenes) flags.push("--scenes");
	if (offline) flags.push("--offline");
	return new Promise((resolve, reject) => {
		const child = spawn(executable, ["sequence", "--dir", outDir, ...flags, ...argv], { cwd: ROOT, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
		let stdout = "";
		let stderr = "";
		child.stdout.on("data", chunk => {
			stdout += chunk;
		});
		child.stderr.on("data", chunk => {
			stderr += chunk;
			process.stderr.write(chunk);
		});
		child.on("error", reject);
		child.on("close", code => {
			const line = stdout.trim().split(/\r?\n/).pop();
			try {
				if (code === 0) return resolve(JSON.parse(line));
			} catch {
				// reported below
			}
			reject(new Error(`sequence failed (exit ${code})${stderr ? `\n${stderr.slice(-4000)}` : ""}`));
		});
	});
}

// `rur sequence`: the story run once and captured frame by frame, every frame at an exact moment
// (`--from` + i / `--fps` s, on the engine's 60 Hz grid when the rate divides 60), as render --time
// would show that moment (native/src/sequence.rs): frames/NNNNN.png, trace.json (every node's
// AbsolutePosition / AbsoluteSize at each change), video.mp4 and video-quarter.mp4, with --scenes each
// frame's scene.json too. Through the render service when `service` (its warm GPU device and a standby
// VM), else `rur-native sequence`. Then summary.json.
export async function sequence(args, { service = false, env = process.env } = {}) {
	if (args.viewport !== undefined || args["viewport-from"] !== undefined) args = { ...args, viewport: resolveViewport(args) };
	const source = args.story || args.module || args.model || args.fixture;
	if (!source) throw new UsageError("sequence needs --story, --module, --model or --fixture");
	if (args.time !== undefined) throw new UsageError("sequence takes --from and --to (or --frames / --duration) with --fps, not --time");
	const options = {};
	for (const key of SEQUENCE_OPTIONS) if (args[key] !== undefined && args[key] !== true) options[key] = String(args[key]);
	if (args["no-png"]) options.png = false;
	if (args["no-video"]) options.video = false;
	if (args.scenes) options.scenes = true;
	const frames = sequenceFrames(options);
	if (!(frames >= 1 && frames <= 99999)) throw new UsageError(`a sequence takes 1 to 99999 frames (got ${frames}: check --to / --frames / --duration)`);
	const fps = options.fps === undefined ? 60 : Number(options.fps);
	const from = options.from === undefined ? 0 : Number(options.from);
	// Actions happen by the last frame's moment at the latest (cli/actions.mjs checks them against it)
	if (args.actions !== undefined) readActions(args.actions, { time: from + (frames - 1) / fps });
	const name = path.basename(String(source)).replace(/\.(story\.)?luau$|\.rbxmx?$/, "");
	const outDir = renderOutDir(args, `${name}-sequence`);
	fs.mkdirSync(outDir, { recursive: true });
	const started = Date.now();
	const argv = runtimeArgs({ ...args, "snapshot-dir": snapshotDir(args) });
	const offline = Boolean(args.offline);
	let result = null;
	// (a profile is a run's own: a process of its own)
	const profile = profileArgs(args);
	if (service && !args.verbose && !profile.length) {
		const { sequenceInService } = await import("./client.mjs");
		result = await sequenceInService(argv, outDir, options, { offline }, { env });
	}
	result ||= await sequenceNative([...profile, ...argv], outDir, options, { offline });
	const diagnostics = result.diagnostics || {};
	const output = asArray(diagnostics.output);
	const summary = {
		status: result.status,
		failure: result.failure || null,
		out: outDir,
		frames: result.frames,
		fps: result.fps,
		times: result.times,
		framesDir: result.framesDir || null,
		trace: result.trace,
		scenesDir: result.scenesDir || null,
		video: result.video || null,
		crop: result.crop,
		size: result.size,
		nodes: result.nodes,
		repeats: asArray(result.repeats),
		unchanged: result.unchanged,
		scriptErrors: output.filter(entry => entry.kind === "error").length,
		warnings: output.filter(entry => entry.kind === "warn").length,
		unemulated: asArray(diagnostics.unemulated).map(entry => entry.member),
		missingImages: [...new Set([...asArray(result.missingImages), ...asArray(result.assetErrors)])],
		missingFonts: asArray(diagnostics.missingFontAssets),
		output: sceneDiagnostics({ diagnostics }).summary.output,
		seconds: (Date.now() - started) / 1000,
		phases: result.phases || {},
		perFrame: result.perFrame || {},
	};
	if (result.profile) summary.profile = result.profile;
	fs.writeFileSync(path.join(outDir, "summary.json"), JSON.stringify(summary, null, 2));
	return summary;
}

function listStories(args) {
	const results = [];
	const walk = dir => {
		for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
			const full = path.join(dir, entry.name);
			if (entry.isDirectory()) walk(full);
			else if (entry.name.endsWith(".story.luau") || entry.name.endsWith(".story.lua")) results.push(full);
		}
	};
	for (const root of projectRoots(args)) walk(root);
	// Directories a Rojo project / --mount places, and the Rojo project's own directory (stories kept
	// next to the project, outside what it mounts; dot-directories and node_modules skipped)
	for (const mount of hasMounts(args) ? runMounts(args) : []) {
		if (mount.kind === "mount" && fs.existsSync(mount.value) && fs.statSync(mount.value).isDirectory()) walk(mount.value);
	}
	if (rojoFileOf(args)) {
		const skipWalk = dir => {
			for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
				if (entry.name.startsWith(".") || entry.name === "node_modules") continue;
				const full = path.join(dir, entry.name);
				if (entry.isDirectory()) skipWalk(full);
				else if (entry.name.endsWith(".story.luau") || entry.name.endsWith(".story.lua")) results.push(full);
			}
		};
		skipWalk(path.dirname(path.resolve(rojoFileOf(args))));
	}
	results.splice(0, results.length, ...new Set(results.map(file => path.resolve(file))));
	// --filter: case-insensitive substring; "a|b" matches either
	const filters = args.filter && args.filter !== true ? String(args.filter).toLowerCase().split("|").filter(Boolean) : [];
	return results.filter(file => !filters.length || filters.some(filter => file.toLowerCase().includes(filter))).sort();
}

// Every story of the project with the place whose roots mount it (a story under a root two places share
// is listed by both and runs with the first).
export function storyEntries(args) {
	const project = path.resolve(args.project && args.project !== true ? String(args.project) : DEFAULT_PROJECT);
	const requested = args.place && args.place !== true ? String(args.place) : "all";
	const names = readPlaces(project).map(place => place.name);
	const places = args.roots ? [requested === "all" ? names[0] : requested] : requested === "all" ? names : [requested];
	const seen = new Map();
	for (const place of places) {
		for (const file of listStories({ ...args, place })) {
			if (!seen.has(file)) seen.set(file, place);
		}
	}
	return [...seen].map(([file, place]) => {
		const relative = path.relative(project, file).replace(/\\/g, "/");
		const key = relative.startsWith("..") || path.isAbsolute(relative) ? file.replace(/\\/g, "/") : relative;
		// Short, readable output directory names (Windows paths stay well under MAX_PATH)
		const slug = key
			.replace(/^[A-Za-z]:/, "")
			.replace(/\.story\.luau?$/, "")
			.split("/")
			.filter(Boolean)
			.join(".");
		return { file, place, key, slug };
	}).sort((a, b) => a.key.localeCompare(b.key));
}

// Story sweep

const CLI_FILE = fileURLToPath(import.meta.url);

function killTree(child) {
	if (!child.pid) return;
	if (process.platform === "win32") {
		spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
		return;
	}
	try {
		process.kill(-child.pid, "SIGKILL");
	} catch {
		child.kill("SIGKILL");
	}
}

// Renders one story in a child `rur render` process of its own (no render service: a story that runs
// away is killed with its whole process tree when it exceeds the timeout).
function renderStoryProcess(entry, dir, args, timeoutSeconds, engine, withPng) {
	const childArgs = [CLI_FILE, "render", "--story", entry.file, "--out", dir, "--place", entry.place, ...(withPng ? [] : ["--no-png"])];
	for (const key of ["viewport", "time", "scale", "mode", "player", "background", "project", "roots", "place-id", "rotation-clipping", "device", "topbar-inset", "rojo", "rojo-at", "git-ref", "plugin"]) {
		if (args[key] !== undefined && args[key] !== true) childArgs.push(`--${key}`, String(args[key]));
	}
	for (const mount of Array.isArray(args.mount) ? args.mount : args.mount && args.mount !== true ? [args.mount] : []) childArgs.push("--mount", String(mount));
	if (args.plugin === true) childArgs.push("--plugin");
	if (args.offline) childArgs.push("--offline");
	return new Promise(resolve => {
		const child = spawn(process.execPath, childArgs, { cwd: ROOT, windowsHide: true, detached: process.platform !== "win32", stdio: ["ignore", "ignore", "pipe"],
			env: { ...process.env, RUR_NATIVE_ENGINE: engine, RUR_NO_SERVICE: "1" } });
		let stderr = "";
		let timedOut = false;
		child.stderr.on("data", chunk => {
			stderr = (stderr + chunk.toString("utf8")).slice(-8000);
		});
		const timer = setTimeout(() => {
			timedOut = true;
			killTree(child);
		}, timeoutSeconds * 1000);
		child.on("error", error => {
			clearTimeout(timer);
			resolve({ code: -1, timedOut, stderr: `${stderr}\n${error.message}` });
		});
		child.on("close", code => {
			clearTimeout(timer);
			resolve({ code, timedOut, stderr });
		});
	});
}

// The line of a (possibly nested, traceback-laden) Luau error that states what went wrong.
function primaryMessage(text) {
	const lines = String(text || "").split(/\r?\n/).map(line => line.trim()).filter(Boolean);
	const meaningful = lines.filter(line => !/^error while running/i.test(line) && !/^\[string ".*"\]:\d+( function .*)?$/.test(line) && !/^Stack Begin|^Stack End/.test(line));
	return meaningful.find(line => /:\d+: /.test(line)) || meaningful[0] || lines[0] || "";
}

// Splits "[string "A.B"]:12: message" / "A.B:12: message" into a site and a message.
function splitSite(message) {
	const match = /^(?:\[string "([^"]+)"\]|([\w.\- ]+?)):(\d+): ([\s\S]*)$/.exec(message);
	if (!match) return { site: null, message };
	return { site: `${match[1] || match[2]}:${match[3]}`, message: match[4] };
}

// Groups messages that differ only in instance paths / numbers.
function causeKey(message) {
	return message
		.replace(/(valid member of \w+) "[^"]*"/g, "$1 \"…\"")
		.replace(/(Infinite yield possible on ')[^']*(')/g, "$1…$2")
		.replace(/\b\d+(\.\d+)?\b/g, "N")
		.slice(0, 240);
}

function readJson(file) {
	try {
		return JSON.parse(fs.readFileSync(file, "utf8"));
	} catch {
		return null;
	}
}

async function sweepOne(entry, args, outRoot, timeoutSeconds, withPng, engine) {
	const dir = path.join(outRoot, "stories", entry.slug);
	fs.rmSync(dir, { recursive: true, force: true });
	fs.mkdirSync(dir, { recursive: true });
	const started = Date.now();
	const result = await renderStoryProcess(entry, dir, args, timeoutSeconds, engine, withPng);
	const record = {
		story: entry.key,
		place: entry.place,
		out: path.relative(outRoot, dir).replace(/\\/g, "/"),
		status: "crash",
		clean: false,
		failure: null,
		errors: [],
		scriptErrors: 0,
		warnings: 0,
		warningMessages: [],
		unemulated: [],
		missingImages: [],
		missingFonts: [],
		renderErrors: [],
		nodes: 0,
		placeholders: 0,
		placeholderRoots: [],
		timings: null,
		seconds: 0,
	};
	const summary = readJson(path.join(dir, "summary.json"));
	if (result.timedOut) {
		record.status = "timeout";
		record.failure = `timed out after ${timeoutSeconds}s`;
	} else if (!summary) {
		// Engine crash: the engine prints the error ("rur-native engine: ..."), then the CLI its own
		const lines = result.stderr.split(/\r?\n/).map(line => line.trim()).filter(line => line && !/^at /.test(line) && !/^rur: /.test(line) && !/^\[Stack (Begin|End)\]$/.test(line) && !/^Script '/.test(line));
		record.failure = lines.find(line => !/^Error: runtime failed/.test(line)) || lines[0] || `exit code ${result.code}`;
		record.detail = result.stderr.slice(-2000);
	} else {
		const scene = readJson(path.join(dir, "scene.json")) || {};
		const diagnostics = scene.diagnostics || {};
		const output = asArray(diagnostics.output);
		record.status = summary.status || "unknown";
		record.failure = summary.failure ? primaryMessage(summary.failure) : null;
		record.errors = output.filter(item => item.kind === "error").map(item => primaryMessage(item.message)).slice(0, 20);
		record.scriptErrors = summary.scriptErrors;
		record.warnings = summary.warnings;
		record.warningMessages = output.filter(item => item.kind === "warn").map(item => primaryMessage(item.message)).slice(0, 20);
		record.unemulated = asArray(diagnostics.unemulated);
		record.missingImages = summary.missingImages;
		record.missingFonts = summary.missingFonts;
		record.nodes = summary.nodes;
		record.timings = diagnostics.timings || null;
		// Studio-only instances the story touched (created from the place hierarchy, default properties)
		const studioOnly = diagnostics.studioOnly || {};
		record.placeholders = Number(studioOnly.count) || 0;
		record.placeholderRoots = asArray(studioOnly.used).slice(0, 10);
		record.renderErrors = asArray(summary.renderErrors);
		record.clean = record.status === "ok" && record.scriptErrors === 0 && record.renderErrors.length === 0;
	}
	record.seconds = (Date.now() - started) / 1000;
	return record;
}

function countBy(records, keyOf) {
	const counts = new Map();
	for (const record of records) {
		for (const key of new Set(keyOf(record))) counts.set(key, (counts.get(key) || 0) + 1);
	}
	return counts;
}

// Instance paths (below the DataModel, dot-joined) known to exist in each place's Studio hierarchy, so
// failures on them count as Studio-only content, not emulation (none are known here: an empty map).
function studioHierarchies(args) {
	return new Map();
}

// "X is not a valid member of Class "A.B"" where A.B.X exists in the place's Studio hierarchy.
function studioOnlyMember(message, place, hierarchies) {
	const match = /([^\s"]+) is not a valid member of \w+ "([^"]+)"/.exec(message);
	const set = hierarchies.get(place);
	if (!match || !set) return null;
	const full = `${match[2]}.${match[1]}`;
	return set.has(full) ? full : null;
}

function aggregateSweep(records, hierarchies = new Map()) {
	const status = {};
	for (const record of records) status[record.status] = (status[record.status] || 0) + 1;

	// Failure causes: mount failures, script errors and timeouts/crashes, counted once per story
	const causes = new Map();
	const addCause = (record, message, kind) => {
		const { site, message: text } = splitSite(message);
		const studioOnly = studioOnlyMember(text, record.place, hierarchies);
		if (studioOnly) {
			record.studioOnly = record.studioOnly || studioOnly;
			kind = `${kind} (Studio-only instance)`;
		}
		const key = `${kind}: ${causeKey(text)}`;
		let cause = causes.get(key);
		if (!cause) {
			cause = { cause: key, stories: new Set(), sites: new Map(), examples: [] };
			causes.set(key, cause);
		}
		if (!cause.stories.has(record.story) && cause.examples.length < 5) cause.examples.push(record.story);
		cause.stories.add(record.story);
		if (site) cause.sites.set(site, (cause.sites.get(site) || 0) + 1);
	};
	for (const record of records) {
		if (record.status === "timeout" || record.status === "crash") addCause(record, record.failure || record.status, record.status);
		else if (record.status === "error" && record.failure) addCause(record, record.failure, "mount");
		for (const message of record.errors) {
			if (record.status === "error" && message === record.failure) continue;
			addCause(record, message, "script");
		}
		if (record.status === "yielded") addCause(record, "story mount never finished (yielded)", "yielded");
	}
	const causeList = [...causes.values()].map(cause => ({
		cause: cause.cause,
		stories: cause.stories.size,
		sites: [...cause.sites].sort((a, b) => b[1] - a[1]).slice(0, 4).map(([site, count]) => `${site} (${count})`),
		examples: cause.examples,
	})).sort((a, b) => b.stories - a.stories);

	const unemulated = new Map();
	for (const record of records) {
		for (const entry of record.unemulated) {
			const item = unemulated.get(entry.member) || { member: entry.member, stories: 0, calls: 0 };
			item.stories++;
			item.calls += entry.count;
			unemulated.set(entry.member, item);
		}
	}
	const warnings = countBy(records, record => record.warningMessages.map(message => causeKey(splitSite(message).message)));
	const images = countBy(records, record => record.missingImages);
	const placeholders = countBy(records, record => record.placeholderRoots || []);
	const fonts = countBy(records, record => record.missingFonts);
	const seconds = records.map(record => record.seconds).sort((a, b) => a - b);
	const percentile = p => (seconds.length ? seconds[Math.min(seconds.length - 1, Math.floor(p * seconds.length))] : 0);
	return {
		stories: records.length,
		clean: records.filter(record => record.clean).length,
		studioOnly: records.filter(record => !record.clean && record.studioOnly).length,
		status,
		causes: causeList,
		unemulated: [...unemulated.values()].sort((a, b) => b.stories - a.stories || b.calls - a.calls),
		warnings: [...warnings].sort((a, b) => b[1] - a[1]).map(([message, stories]) => ({ message, stories })),
		missingImages: [...images].sort((a, b) => b[1] - a[1]).map(([image, stories]) => ({ image, stories })),
		placeholders: [...placeholders].sort((a, b) => b[1] - a[1]).map(([instance, stories]) => ({ instance, stories })),
		missingFonts: [...fonts].sort((a, b) => b[1] - a[1]).map(([font, stories]) => ({ font, stories })),
		seconds: {
			total: seconds.reduce((sum, value) => sum + value, 0),
			median: percentile(0.5),
			p90: percentile(0.9),
			max: seconds.length ? seconds[seconds.length - 1] : 0,
		},
		slowest: [...records].sort((a, b) => b.seconds - a.seconds).slice(0, 15).map(record => ({ story: record.story, seconds: record.seconds, status: record.status })),
	};
}

// A table cell's text, cut to 200 characters before escaping (a cut cannot leave half an escape), with backslashes
// escaped before pipes (text ending in a backslash cannot unescape the cell's own `|`)
export function markdownCell(value) {
	return String(value ?? "").replace(/\r?\n/g, " ").slice(0, 200).replace(/\\/g, "\\\\").replace(/\|/g, "\\|");
}

function sweepMarkdown(report) {
	const { totals, options, changes } = report;
	const lines = [];
	lines.push("# Story sweep", "");
	lines.push(`${report.finished} · ${totals.stories} stories · ${totals.clean} clean · jobs ${options.jobs} · timeout ${options.timeout}s · viewport ${options.viewport} @${options.scale}x · time ${options.time}s · wall ${report.wallSeconds.toFixed(0)}s`, "");
	lines.push("Clean = mounted without error, no script errors, no render errors.", "");
	lines.push("## Status", "", "| status | stories |", "|---|---|");
	for (const [status, count] of Object.entries(totals.status).sort((a, b) => b[1] - a[1])) lines.push(`| ${status} | ${count} |`);
	lines.push(`| ok with script errors | ${report.records.filter(record => record.status === "ok" && record.scriptErrors > 0).length} |`);
	lines.push(`| not clean, needs Studio-only content (instances the place has but the repo does not) | ${totals.studioOnly} |`);
	lines.push(`| rendered with Studio-only placeholder instances (default properties) | ${report.records.filter(record => record.placeholders > 0).length} |`, "");
	if (changes) {
		lines.push("## Changes vs previous report", "");
		lines.push(`Fixed (now clean): ${changes.fixed.length} · regressed (no longer clean): ${changes.regressed.length}`, "");
		for (const story of changes.regressed.slice(0, 50)) lines.push(`- regressed: ${story}`);
		if (changes.regressed.length) lines.push("");
	}
	lines.push("## Failure causes (stories affected)", "", "| stories | cause | top sites | example |", "|---|---|---|---|");
	for (const cause of totals.causes.slice(0, 40)) lines.push(`| ${cause.stories} | ${markdownCell(cause.cause)} | ${markdownCell(cause.sites.join(", "))} | ${markdownCell(cause.examples[0])} |`);
	lines.push("", "## Unemulated members (stories / calls)", "", "| member | stories | calls |", "|---|---|---|");
	for (const item of totals.unemulated.slice(0, 40)) lines.push(`| ${markdownCell(item.member)} | ${item.stories} | ${item.calls} |`);
	lines.push("", "## Studio-only instances used (placeholders with default properties)", "", "| stories | instance |", "|---|---|");
	for (const item of (totals.placeholders || []).slice(0, 25)) lines.push(`| ${item.stories} | ${markdownCell(item.instance)} |`);
	lines.push("", "## Warnings (stories)", "", "| stories | warning |", "|---|---|");
	for (const item of totals.warnings.slice(0, 20)) lines.push(`| ${item.stories} | ${markdownCell(item.message)} |`);
	lines.push("", `## Missing images (${totals.missingImages.length} unique)`, "", "| stories | image |", "|---|---|");
	for (const item of totals.missingImages.slice(0, 15)) lines.push(`| ${item.stories} | ${markdownCell(item.image)} |`);
	lines.push("", `## Missing fonts (${totals.missingFonts.length} unique)`, "");
	for (const item of totals.missingFonts.slice(0, 15)) lines.push(`- ${item.stories} stories: ${item.font}`);
	lines.push("", "## Time", "", `Total ${totals.seconds.total.toFixed(0)}s · median ${totals.seconds.median.toFixed(1)}s · p90 ${totals.seconds.p90.toFixed(1)}s · max ${totals.seconds.max.toFixed(1)}s`, "");
	for (const item of totals.slowest) lines.push(`- ${item.seconds.toFixed(1)}s ${item.story} (${item.status})`);
	lines.push("", "## Stories", "", "| story | place | status | errors | first problem | unemulated | missing images | nodes | s |", "|---|---|---|---|---|---|---|---|---|");
	for (const record of report.records) {
		const problem = record.failure || record.errors[0] || record.renderErrors[0] || "";
		lines.push(`| ${markdownCell(record.story)} | ${record.place} | ${record.status} | ${record.scriptErrors} | ${markdownCell(problem)} | ${markdownCell(record.unemulated.slice(0, 3).map(entry => entry.member).join(", "))} | ${record.missingImages.length} | ${record.nodes} | ${record.seconds.toFixed(1)} |`);
	}
	return lines.join("\n") + "\n";
}

async function sweep(args) {
	const outRoot = path.resolve(args.out && args.out !== true ? String(args.out) : path.join(OUT_DIR, "sweep"));
	const jobs = Math.max(1, Number(args.jobs) || Math.min(8, Math.max(1, Math.floor(os.cpus().length / 3))));
	const timeoutSeconds = Math.max(5, Number(args.timeout) || 90);
	const withPng = !args["no-png"];
	// Studio runs at 1.5x DPI here; sweep renders match it unless --scale says otherwise
	const options = { jobs, timeout: timeoutSeconds, viewport: String(args.viewport && args.viewport !== true ? args.viewport : "1200x900"), scale: Number(args.scale) || 1.5, time: Number(args.time) || 1, png: withPng, filter: args.filter || null };
	const childArgs = { ...args, viewport: options.viewport, scale: String(options.scale), time: String(options.time) };
	fs.mkdirSync(outRoot, { recursive: true });

	const reportFile = path.join(outRoot, "report.json");
	const previous = readJson(reportFile);

	// --summarize: rebuild totals + summary.md from the existing report without rendering
	if (args.summarize) {
		if (!previous) throw new Error(`no report at ${reportFile}`);
		previous.totals = aggregateSweep(previous.records, studioHierarchies(args));
		fs.writeFileSync(reportFile, JSON.stringify(previous, null, 1));
		fs.writeFileSync(path.join(outRoot, "summary.md"), sweepMarkdown(previous));
		return previous;
	}

	let entries = storyEntries(args);
	if (args.failed && previous) {
		const notClean = new Set(previous.records.filter(record => !record.clean).map(record => record.story));
		entries = entries.filter(entry => notClean.has(entry.key));
	}
	if (args.limit) entries = entries.slice(0, Number(args.limit));
	// One engine build for every child render (cli/engine.mjs)
	const engine = await ensureEngine();
	process.stderr.write(`rur sweep: ${entries.length} stories, ${jobs} jobs, timeout ${timeoutSeconds}s${withPng ? "" : ", no png"} -> ${outRoot}\n`);

	const started = Date.now();
	const records = [];
	let next = 0;
	let done = 0;
	const worker = async () => {
		while (next < entries.length) {
			const entry = entries[next++];
			const record = await sweepOne(entry, childArgs, outRoot, timeoutSeconds, withPng, engine);
			records.push(record);
			done++;
			const problem = record.clean ? "" : ` ${(record.failure || record.errors[0] || record.renderErrors[0] || "").slice(0, 140)}`;
			process.stderr.write(`[${done}/${entries.length}] ${record.status}${record.scriptErrors ? ` (${record.scriptErrors} errors)` : ""} ${record.seconds.toFixed(1)}s ${entry.key}${problem}\n`);
		}
	};
	await Promise.all(Array.from({ length: Math.min(jobs, entries.length) }, worker));

	// Runtime crashes are re-run once more at the end: other agents edit the runtime while sweeps run,
	// and a half-written file crashes every story that starts during the edit
	const retries = args.retries !== undefined ? Number(args.retries) : 1;
	for (let attempt = 0; attempt < retries; attempt++) {
		const crashed = records.filter(record => record.status === "crash");
		if (!crashed.length) break;
		process.stderr.write(`rur sweep: re-running ${crashed.length} crashed stories
`);
		const byKey = new Map(entries.map(entry => [entry.key, entry]));
		const retry = crashed.map(record => byKey.get(record.story)).filter(Boolean);
		let retryNext = 0;
		const retryWorker = async () => {
			while (retryNext < retry.length) {
				const entry = retry[retryNext++];
				const record = await sweepOne(entry, childArgs, outRoot, timeoutSeconds, withPng, engine);
				records[records.findIndex(item => item.story === entry.key)] = record;
			}
		};
		await Promise.all(Array.from({ length: Math.min(jobs, retry.length) }, retryWorker));
	}
	// --failed re-runs merge into the previous report so it stays complete
	let allRecords = records;
	if (args.failed && previous) {
		const rerun = new Map(records.map(record => [record.story, record]));
		allRecords = previous.records.map(record => rerun.get(record.story) || record);
	}
	allRecords.sort((a, b) => a.story.localeCompare(b.story));

	let changes = null;
	if (previous && Array.isArray(previous.records)) {
		const before = new Map(previous.records.map(record => [record.story, record]));
		changes = { fixed: [], regressed: [] };
		for (const record of allRecords) {
			const old = before.get(record.story);
			if (!old) continue;
			if (record.clean && !old.clean) changes.fixed.push(record.story);
			if (!record.clean && old.clean) changes.regressed.push(record.story);
		}
	}

	const report = {
		finished: new Date().toISOString(),
		wallSeconds: (Date.now() - started) / 1000,
		options,
		totals: aggregateSweep(allRecords, studioHierarchies(args)),
		changes,
		records: allRecords,
	};
	fs.writeFileSync(reportFile, JSON.stringify(report, null, 1));
	fs.writeFileSync(path.join(outRoot, "summary.md"), sweepMarkdown(report));
	return report;
}

// `fix`: remove the state dir's build leftovers (cli/cleanup.mjs)
async function doctor({ fix = false } = {}) {
	const { nativeAppStatus, describeNativeApp } = await import("../tools/native-app/config.mjs");
	const { servicePorts, status: serviceStatus } = await import("./client.mjs");
	const { LOCK_FILE, studioLockHolder } = await import("./lock.mjs");
	const { isolated, serviceDisabled } = await import("./paths.mjs");
	const { megabytes, removeLeftovers, stateLeftovers } = await import("./cleanup.mjs");
	const leftovers = stateLeftovers();
	const fixed = fix ? removeLeftovers(leftovers) : null;
	const leftoverBytes = leftovers.reduce((sum, item) => sum + item.bytes, 0);
	const leftoverFiles = leftovers.reduce((sum, item) => sum + item.files, 0);
	const engineStatus = nativeAppStatus();
	const given = process.env.RUR_NATIVE_ENGINE;
	const service = await serviceStatus().catch(() => null);
	const holder = studioLockHolder();
	const rust = spawnSync("rustc", ["--version"], { encoding: "utf8", windowsHide: true });
	const content = studioContentRoots();
	return {
		root: ROOT,
		stateDir: stateDir(),
		outDir: OUT_DIR,
		engine: given ? `${given} (RUR_NATIVE_ENGINE${fs.existsSync(given) ? "" : ": missing"})`
			: engineStatus.state === "current" ? engineStatus.executable
			: engineStatus.state === "stale" && isolated() ? `${engineStatus.executable} (used as it is: --isolated; ${describeNativeApp(engineStatus).replace(/\. To use it anyway: .*$/, "")})`
			: `${describeNativeApp(engineStatus)} (built on first use, or: node tools/native-app/build.mjs)`,
		engineFingerprint: engineStatus.fingerprint || null,
		// The engine core: roblox-engine-emulator at the commit native/Cargo.toml pins, or a linked checkout
		emulator: emulatorLink() ? `linked: ${emulatorLink()} (node tools/emulator.mjs unlink: back to ${pinnedCommit().slice(0, 12)})` : `${pinnedCommit().slice(0, 12)} (pinned in native/Cargo.toml; node tools/emulator.mjs status)`,
		isolated: isolated() ? "yes (RUR_ISOLATED / --isolated): no render service; the engine is the state dir's build even when native/ changed" : "no",
		service: serviceDisabled() ? `not used (${isolated() ? "--isolated" : "--no-service / RUR_NO_SERVICE"}; not contacted)`
			: service ? `running on 127.0.0.1:${service.port} (pid ${service.pid}, ${service.executable || "an older build"}) for ${service.root}, state ${service.stateDir}`
			: `not running (starts on first use on 127.0.0.1:${servicePorts().join("/")}${process.env.RUR_SERVE_PORT ? ", RUR_SERVE_PORT" : ""})`,
		leftovers: fixed
			? `removed ${megabytes(fixed.bytes)} in ${fixed.files} files${fixed.failed.length ? `; kept ${fixed.failed.map(item => `${item.path} (${item.error})`).join(", ")}` : ""}`
			: leftovers.length ? `${megabytes(leftoverBytes)} in ${leftoverFiles} files of build leftovers (${[...new Set(leftovers.map(item => item.what))].join(", ")}): rur doctor --fix removes them`
			: "none",
		studioLock: holder ? `held by pid ${holder.pid} (${holder.owner}) since ${new Date(holder.since).toISOString()}: ${holder.file}` : `free (${LOCK_FILE})`,
		rust: rust.status === 0 ? rust.stdout.trim() : "missing: the engine is native (install Rust: https://rustup.rs)",
		freetype: "2.14.2, compiled into the engine (native/build.rs; the release Studio's glyphs match it)",
		studioInstall: content[0] || "missing (fonts/textures need a local Roblox Studio install)",
		assetCookie: hasCookie() ? "available (never printed)" : "not configured (public assets only)",
		cache: CACHE_DIR,
		defaultProject: fs.existsSync(DEFAULT_PROJECT) ? DEFAULT_PROJECT : "not found",
	};
}

// stdout carries only a command's JSON result: anything else written to it while `work` runs (a
// dependency's console.log, a build) goes to stderr. `--json` prints the result on one line.
async function jsonResult(args, work, shape = value => value) {
	const write = process.stdout.write;
	const log = console.log, info = console.info;
	process.stdout.write = (chunk, ...rest) => process.stderr.write(chunk, ...rest);
	console.log = console.info = (...values) => console.error(...values);
	let result;
	try {
		result = await work();
	} finally {
		process.stdout.write = write;
		console.log = log;
		console.info = info;
	}
	const shaped = shape(result);
	process.stdout.write(`${args.json ? JSON.stringify(shaped) : JSON.stringify(shaped, null, 2)}\n`);
	return result;
}

// rur diff --git-ref <ref>: the same source rendered at <ref> (before) and in the working tree
// (after), then cli/inspect.mjs's layout + pixel diff (diff.png)
async function beforeAfter(args) {
	if (!args["git-ref"] || args["git-ref"] === true) throw new Error("rur diff needs --git-ref <ref> (the 'before' commit)");
	const source = args.story || args.module || args.model || args.fixture;
	if (!source || source === true) throw new Error("rur diff needs --story, --module, --model or --fixture");
	const name = path.basename(String(source)).replace(/\.(story\.)?luau$|\.rbxmx?$/, "");
	const outDir = renderOutDir(args, `${name}-diff`);
	const own = { ...args, tag: undefined, keep: undefined };
	const before = await render({ ...own, out: path.join(outDir, "before") });
	const after = await render({ ...own, "git-ref": undefined, out: path.join(outDir, "after") });
	const { diffRenders } = await import("./inspect.mjs");
	const diff = diffRenders(before.out, after.out, { out: path.join(outDir, "diff.png") });
	return { before, after, changes: diff.changes, pixels: diff.pixels, out: outDir };
}

async function main() {
	const args = parseArgs(process.argv.slice(2));
	const command = args._[0];
	if (command === "render") {
		const summary = await jsonResult(args, async () => {
			// Through the render service unless --isolated / --no-service (cli/client.mjs)
			const { serviceDisabled } = await import("./paths.mjs");
			return render(args, { service: !serviceDisabled() });
		});
		// The diagnostics in words on stderr (stdout holds only the JSON)
		for (const line of diagnosticLines(summary)) process.stderr.write(`rur: ${line}
`);
		if (summary.status === "error") process.exitCode = 2;
	} else if (command === "sequence") {
		const summary = await jsonResult(args, async () => {
			const { serviceDisabled } = await import("./paths.mjs");
			return sequence(args, { service: !serviceDisabled() });
		});
		if (summary.status === "error") process.exitCode = 2;
	} else if (command === "diff") {
		const result = await jsonResult(args, () => beforeAfter(args), value => ({ ...value, changes: value.changes.slice(0, 200) }));
		if (result.before.status === "error" || result.after.status === "error") process.exitCode = 2;
	} else if (command === "compare") {
		const { compare, CaptureNeeded } = await import("./compare.mjs");
		try {
			await jsonResult(args, () => compare({ ...args, viewport: resolveViewport(args), rootsResolved: projectRoots(args).join(";") }), report => ({ ...report, layout: { ...report.layout, rows: report.layout.rows.slice(0, 15) } }));
		} catch (error) {
			// --reuse-only and no reusable Studio capture: exit code 3, nothing else touched
			if (!(error instanceof CaptureNeeded)) throw error;
			process.stderr.write(`rur compare: ${error.message}
`);
			process.exitCode = 3;
		}
	} else if (command === "record") {
		const summary = await jsonResult(args, () => record(args));
		if (summary.status === "error") process.exitCode = 2;
	} else if (command === "profile") {
		// The engine's profile comparison and page (`rur-native profile`): its arguments as they are
		const executable = await ensureEngine();
		const result = spawnSync(executable, ["profile", ...process.argv.slice(3)], { cwd: process.cwd(), stdio: "inherit", windowsHide: true });
		if (result.error) throw result.error;
		process.exitCode = result.status ?? 1;
	} else if (command === "stories") {
		if (args.place === "all") {
			for (const entry of storyEntries(args)) console.log(entry.file);
		} else {
			for (const file of listStories(args)) console.log(file);
		}
	} else if (command === "sweep") {
		const report = await sweep(args);
		const { totals } = report;
		console.log(JSON.stringify({ stories: totals.stories, clean: totals.clean, status: totals.status, topCauses: totals.causes.slice(0, 10).map(cause => `${cause.stories} ${cause.cause}`), topUnemulated: totals.unemulated.slice(0, 10).map(item => `${item.stories} ${item.member}`), seconds: totals.seconds, report: path.resolve(args.out && args.out !== true ? String(args.out) : path.join(OUT_DIR, "sweep"), "summary.md") }, null, 2));
	} else if (command === "doctor") {
		console.log(JSON.stringify(await doctor({ fix: Boolean(args.fix) }), null, 2));
	} else if (command === "clean") {
		const { clean } = await import("./clean.mjs");
		const { megabytes } = await import("./cleanup.mjs");
		const days = args["older-than"];
		if (days !== undefined && !(Number(days) >= 0)) throw new UsageError("--older-than needs a number of days");
		const result = clean({ olderThanDays: days === undefined ? null : Number(days), dryRun: Boolean(args["dry-run"]), log: line => process.stderr.write(`rur: ${line}\n`) });
		const line = bucket => `${megabytes(bucket.bytes)} in ${bucket.files} files`;
		console.log(JSON.stringify({
			root: OUT_DIR,
			dryRun: Boolean(args["dry-run"]),
			linkedImages: line(result.linked),
			linkedFrames: line(result.frames),
			retiredFiles: line(result.retired),
			expired: days === undefined ? "not asked (--older-than <days>)" : line(result.expired),
			frameStore: line(result.store),
			freed: megabytes(result.linked.bytes + result.frames.bytes + result.retired.bytes + result.expired.bytes + result.store.bytes),
			failed: result.failed.slice(0, 20),
		}, null, 2));
	} else if (command === "assets") {
		if (args._[1] !== "warm") throw new UsageError("rur assets: the only subcommand is warm (rur assets warm [--refresh])");
		const { warmAssets } = await import("./asset-warm.mjs");
		const result = await warmAssets({ refresh: Boolean(args.refresh) });
		console.log(JSON.stringify({ ...result, missing: result.missing.slice(0, 50) }, null, 2));
		if (result.missing.length) process.stderr.write(`rur: ${result.missing.length} of ${result.images} images could not be cached (missing)\n`);
	} else if (command === "selftest") {
		const { selftest } = await import("./selftest.mjs");
		process.exitCode = await selftest();
	} else if (command === "native") {
		const result = spawnSync(process.execPath, [path.join(ROOT, "cli/native.mjs"), ...process.argv.slice(3)], {
			stdio: "inherit", windowsHide: true,
		});
		if (result.error) throw result.error;
		process.exitCode = result.status ?? 1;
	} else {
		const header = fs.readFileSync(fileURLToPath(import.meta.url), "utf8").split("\n").slice(1);
		console.log(header.slice(0, header.findIndex(line => !line.startsWith("//"))).join("\n"));
		process.exitCode = command ? 1 : 0;
	}
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	main().catch(async error => {
		// A bad option (e.g. --actions): the message alone, exit 64, nothing was run or written
		if (error instanceof UsageError) {
			console.error(`rur: ${error.message}`);
			process.exitCode = 64;
			return;
		}
		console.error(error && error.stack ? error.stack : error);
		process.exitCode = 1;
	});
}
