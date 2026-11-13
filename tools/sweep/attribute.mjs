// Attributes the pixels that differ between Studio and rur in `rur compare` outputs to causes.
//
//   node tools/sweep/attribute.mjs <out/compare/name> ... [--threshold 2] [--no-runs] [--quiet] [--map]
//
// Writes <dir>/attribution.json per compare directory and prints a summary line per story. Library use:
// `attribute(dirs, options)` (tools/sweep/spotcheck.mjs). `--map` also writes <dir>/attrib/causes.png: every
// differing pixel in its cause's colour (CAUSE_COLORS) over the dimmed render.
//
// How a differing pixel (largest channel difference > threshold) is attributed:
//
// 1. Paint masks: the offline scene is drawn again by the native renderer (`rur-native --batch`) with one
//    kind of visual removed at a time (text incl. text strokes, border strokes, images, shadows,
//    gradients, fills, scroll bars); a pixel belongs to a kind when removing it changes the pixel. This
//    gives exact ink masks.
// 2. Owner: the topmost node (draw order) whose own visuals include a kind whose mask covers the pixel,
//    within the node's device bounds (deviceBounds below); fills and images of rotated nodes only
//    inside their rotated rect, border strokes only in their band (not deep inside the frame).
// 3. Node status, first match wins (the owner, or a CanvasGroup / ancestor that composites it):
//    - content: the node's Text or Image differs from Studio's, it has no Studio counterpart, or a
//      second offline run at the same moment (replaying the compare's recorded clock) shows other text,
//      another image or another visibility (real-time clocks, randomness, pointer-keyed order);
//    - animation: its visual properties (transparency / colour / gradient offset) differ from Studio's
//      dump, or it animates (its export differs between ideal-clock runs at `time` and `time + 1 s`)
//      and either the compare did not replay Studio's clock or its rect still differs;
//    - layout: its rect differs from Studio's AbsolutePosition / AbsoluteSize, or its TextBounds do.
//    Pixels owned by a clean node that lie where a shifted, animated or Studio-only node paints in
//    Studio get that node's cause.
// 4. Text halo: a pixel within 2 px of the text ink of a node drawn above its owner (or anywhere, when the
//    owner only shades it with a shadow) counts as that text node's (Studio's glyphs reach further out).
//    Otherwise the kind of paint under the pixel: text, strokes, images, scroll bars, shadows,
//    gradients, fills; pixels in a CanvasGroup that Studio resamples or blends (fractional device
//    rect, GroupTransparency > 0, GroupColor3 tint) count as canvasgroups. A text node whose
//    differences mostly vanish when our glyphs move by up to 2 px counts as layout ("text placement").
// 5. Pixels no node paints: unowned (Studio draws something we do not, or the capture).

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";

import { ROOT } from "../native-app/config.mjs";
import { renderScenes } from "../native-app/batch.mjs";
import { engineBaseArgs, ensureEngine } from "../../cli/engine.mjs";
import { relativePaths } from "../../cli/compare.mjs";
import { isMain } from "../../cli/entry.mjs";
import { placeRoots } from "../../cli/places.mjs";

const PROJECT = path.resolve(process.env.RUR_PROJECT || path.join(ROOT, "examples", "project"));

// Paint kinds (bit per kind in the mask)
const KINDS = ["text", "strokes", "images", "shadows", "gradients", "fills", "scroll"];
const BIT = Object.fromEntries(KINDS.map((kind, index) => [kind, 1 << index]));

// Colours of attrib/causes.png (--map: every differing pixel in its cause's colour over the dimmed
// render; text includes text placement)
const CAUSE_COLORS = { text: [255, 255, 255], strokes: [255, 64, 64], images: [64, 160, 255], canvasgroups: [255, 0, 255], gradients: [255, 200, 0], shadows: [0, 255, 255], fills: [0, 255, 0], scroll: [128, 128, 255], layout: [255, 128, 0], animation: [160, 80, 255], content: [128, 255, 128], unowned: [255, 255, 128] };

export const CAUSES = ["text", "strokes", "images", "canvasgroups", "gradients", "shadows", "fills", "scroll", "layout", "animation", "content", "unowned"];

// The place id of compare reports that recorded none (0: an unpublished place, as in a new Studio file)
const DEFAULT_PLACE_ID = "0";

// Offline runs

function projectRoots(place) {
	const roots = placeRoots(PROJECT, place);
	return roots.filter(dir => fs.existsSync(dir));
}

// Runs the engine for a story with a compare report's options; resolves with the scene (or null).
// clock: a recorded Studio clock to replay (RUR_CLOCK), else the ideal clock.
function runStory(report, time, outDir, clock = null) {
	const options = report.options || {};
	const viewport = options.viewport || "1200x900";
	const camera = options.camera || (report.studio && report.studio.viewport ? `${report.studio.viewport[0]}x${report.studio.viewport[1]}` : viewport);
	const background = Array.isArray(options.background) ? options.background.join(",") : "46,46,46";
	const scenePath = path.join(outDir, "scene.json");
	const args = [...engineBaseArgs(), "--roots", projectRoots(options.place).join(";"), "--out", scenePath,
		"--story", path.resolve(report.source), "--viewport", viewport, "--scale", String(options.scale || 1.5), "--time", String(time),
		"--background", background, "--camera", camera, "--place-id", String(options.placeId || (report.studio && report.studio.placeId) || DEFAULT_PLACE_ID)];
	if (options.controls) {
		args.push("--controls", String(options.controls));
	}
	// Reports made before the rotation rollout option used legacy clipping.
	args.push("--rotation-clipping", options.rotationClipping || "disabled");
	// The Studio asset snapshot the compare's offline run loaded (cli/compare.mjs, tools/sweep/studio-assets.mjs)
	if (options.snapshotDir && fs.existsSync(options.snapshotDir)) {
		args.push("--snapshot-dir", path.resolve(options.snapshotDir));
	}
	fs.mkdirSync(outDir, { recursive: true });
	const env = { ...process.env };
	delete env.RUR_CLOCK;
	if (clock && fs.existsSync(clock)) {
		env.RUR_CLOCK = path.resolve(clock);
	}
	// Studio's LocalPlayer, as the compare's offline run had it (cli/compare.mjs withStudioPlayer)
	const player = options.player;
	if (player) {
		env.RUR_PLAYER = String(player.name);
		env.RUR_PLAYER_ID = String(player.id);
		env.RUR_PLAYER_DISPLAY = String(player.display);
	}
	// Studio's sibling order, replayed like the compare's offline run did (cli/compare.mjs withStudioOrder);
	// every run gets it, so node keys of same-named siblings stay comparable between the runs
	delete env.RUR_ORDER;
	if (options.order && fs.existsSync(options.order)) {
		env.RUR_ORDER = path.resolve(options.order);
	}
	// A run is a pure function of the engine build, its arguments, the replayed clock / player / order and
	// the story's source files (the compare's offline run listed them in dependencies.json): with none of
	// them changed since the last attribution, its saved scene is reused
	const fileHash = file => {
		try {
			return createHash("sha1").update(fs.readFileSync(file)).digest("hex");
		} catch {
			return null;
		}
	};
	const sources = (() => {
		try {
			const list = JSON.parse(fs.readFileSync(path.join(path.dirname(report.files.ours), "dependencies.json"), "utf8")).files || [];
			return list.length ? Object.fromEntries([path.resolve(report.source), ...list].map(file => [file, fileHash(file)])) : null;
		} catch {
			return null;
		}
	})();
	const keyFile = path.join(outDir, "run-key.json");
	return ensureEngine().then(engine => {
		const key = sources && createHash("sha1").update(JSON.stringify({
			engine: path.basename(engine), args, sources,
			clock: env.RUR_CLOCK ? fileHash(env.RUR_CLOCK) : null, order: env.RUR_ORDER ? fileHash(env.RUR_ORDER) : null,
			player: [env.RUR_PLAYER, env.RUR_PLAYER_ID, env.RUR_PLAYER_DISPLAY],
		})).digest("hex");
		if (key && fs.existsSync(keyFile) && fs.existsSync(scenePath)) {
			try {
				if (JSON.parse(fs.readFileSync(keyFile, "utf8")).key === key) return JSON.parse(fs.readFileSync(scenePath, "utf8"));
			} catch {
				// run it again
			}
		}
		fs.rmSync(keyFile, { force: true });
		fs.rmSync(scenePath, { force: true });
		return runEngine(engine, key);
	});
	function runEngine(engine, key) {
		return new Promise(resolve => {
		const child = spawn(engine, args, { cwd: ROOT, env, stdio: ["ignore", "pipe", "pipe"], windowsHide: true });
		let output = "";
		child.stdout.on("data", chunk => {
			output = (output + chunk).slice(-4000);
		});
		child.stderr.on("data", chunk => {
			output = (output + chunk).slice(-4000);
		});
		const timer = setTimeout(() => child.kill(), 120000);
		child.on("exit", code => {
			clearTimeout(timer);
			try {
				const scene = JSON.parse(fs.readFileSync(scenePath, "utf8"));
				if (key) fs.writeFileSync(keyFile, JSON.stringify({ key }));
				resolve(scene);
			} catch {
				process.stderr.write(`attribute: offline run failed (exit ${code}) for ${report.source} at t=${time}: ${output.trim().split(/\r?\n/).slice(-3).join(" | ")}\n`);
				resolve(null);
			}
		});
		});
	}
}

// Node key within a story: relative path below StoryTarget (as compareLayout builds it) plus the
// occurrence index of that path
function keyedNodes(scene) {
	const result = new Map();
	if (!scene || !Array.isArray(scene.nodes)) {
		return result;
	}
	const paths = relativePaths(scene);
	const seen = new Map();
	for (const node of scene.nodes) {
		const relative = paths.get(node.id);
		if (!relative) {
			continue;
		}
		const count = seen.get(relative) || 0;
		seen.set(relative, count + 1);
		result.set(`${relative}#${count}`, node);
	}
	return result;
}

// A node's export without its identity fields, for comparing two runs
function signature(node) {
	const { id, parent, order, ...rest } = node;
	void id;
	void parent;
	void order;
	return JSON.stringify(rest);
}

// What a node shows (text, image, visibility): two runs at the same moment that disagree on it produce
// content nondeterministically (real-time clocks, randomness, pointer-keyed table order)
function contentSignature(node) {
	return JSON.stringify([node.visible, node.props && node.props.text, node.image && (node.image.source || node.image.uri)]);
}

// Keys of nodes whose export (or `sign(node)`) differs between two scenes (or exists in one only)
function changedKeys(a, b, sign = signature) {
	const changed = new Set();
	const nodesA = keyedNodes(a), nodesB = keyedNodes(b);
	for (const [key, node] of nodesA) {
		const other = nodesB.get(key);
		if (!other || sign(node) !== sign(other)) {
			changed.add(key);
		}
	}
	for (const key of nodesB.keys()) {
		if (!nodesA.has(key)) {
			changed.add(key);
		}
	}
	return changed;
}

// Paint masks and node bounds

// Luau's empty tables arrive as {}: anything but an array is empty
const asArray = value => (Array.isArray(value) ? value : []);

// Studio's device edge of a logical coordinate: an edge up to 1/960 logical px below k + 0.5 still
// lands on k + 1 (native/src/render/geometry.rs snap_device)
const snapDevice = (v, S) => Math.floor((Math.floor(v * 256 + S * 4 / 15) + 128) / 256);

// Device edges [start, end) of a logical span: float32 position and size scaled separately, added in
// float32, then snapped (geometry.rs device_span)
function deviceSpan(position, size, S) {
	const f = Math.fround;
	const start = f(f(position) * f(S));
	return [snapDevice(start, S), snapDevice(f(start + f(f(size) * f(S))), S)];
}

// Conservative device-space box [x0, y0, x1, y1] (integers) of every pixel a node's own drawing can
// touch: its rect grown by borders, strokes and shadows, united with its text lines and TextBox editing
// marks, through its transform, cut by its clip
export function deviceBounds(node, S) {
	const [x, y, w, h] = node.rect;
	let grow = 2;
	if (node.border && node.border.size > 0) grow = Math.max(grow, node.border.size * S + 2);
	for (const stroke of asArray(node.strokes)) grow = Math.max(grow, ((stroke.thickness || 0) + Math.abs(stroke.borderOffset || 0)) * S + 3);
	let x0 = x * S - grow, y0 = y * S - grow, x1 = (x + w) * S + grow, y1 = (y + h) * S + grow;
	for (const shadow of asArray(node.shadows)) {
		const pad = 3 * Math.max(0, (shadow.blur || 0) * S / 2) + 2;
		const sx = Math.abs(shadow.spread ? shadow.spread[0] : 0) * S / 2 + pad, sy = Math.abs(shadow.spread ? shadow.spread[1] : 0) * S / 2 + pad;
		const ox = (shadow.offset ? shadow.offset[0] : 0) * S, oy = (shadow.offset ? shadow.offset[1] : 0) * S;
		x0 = Math.min(x0, x * S + ox - sx);
		y0 = Math.min(y0, y * S + oy - sy);
		x1 = Math.max(x1, (x + w) * S + ox + sx);
		y1 = Math.max(y1, (y + h) * S + oy + sy);
	}
	const text = node.text;
	if (text && asArray(text.lines).length) {
		// Glyph bitmaps overhang their line box (bearings, accents, italics) by well under a line height;
		// text strokes grow them by their radius
		let overhang = (text.size || 0) + (text.lineHeight || 0) + 4;
		for (const stroke of asArray(node.strokes)) overhang += (stroke.thickness || 0) * S;
		if (text.stroke) overhang += 1;
		for (const line of asArray(text.lines)) {
			x0 = Math.min(x0, line.x - overhang);
			y0 = Math.min(y0, line.top - overhang);
			x1 = Math.max(x1, line.x + (line.width || 0) + overhang);
			y1 = Math.max(y1, line.top + (text.lineHeight || 0) + overhang);
		}
	}
	// TextBox editing (physical px): selection highlights and the caret
	const editing = [...(node.selection ? asArray(node.selection.rects) : []), ...(node.caret ? [[node.caret.x, node.caret.y, node.caret.w, node.caret.h]] : [])];
	for (const [ex, ey, ew, eh] of editing) {
		x0 = Math.min(x0, ex - 2);
		y0 = Math.min(y0, ey - 2);
		x1 = Math.max(x1, ex + Math.max(1, ew) + 2);
		y1 = Math.max(y1, ey + eh + 2);
	}
	if (node.matrix) {
		const m = node.matrix;
		const xs = [], ys = [];
		for (const [px, py] of [[x0, y0], [x1, y0], [x0, y1], [x1, y1]]) {
			xs.push(m[0] * px + m[2] * py + m[4] * S);
			ys.push(m[1] * px + m[3] * py + m[5] * S);
		}
		x0 = Math.min(...xs) - 2;
		y0 = Math.min(...ys) - 2;
		x1 = Math.max(...xs) + 2;
		y1 = Math.max(...ys) + 2;
	}
	if (node.clip) {
		const [cx, cy, cw, ch] = node.clip;
		const [cx0, cx1] = deviceSpan(cx, cw, S), [cy0, cy1] = deviceSpan(cy, ch, S);
		x0 = Math.max(x0, cx0);
		y0 = Math.max(y0, cy0);
		x1 = Math.min(x1, cx1);
		y1 = Math.min(y1, cy1);
	}
	return [Math.floor(x0), Math.floor(y0), Math.max(Math.floor(x0), Math.ceil(x1)), Math.max(Math.floor(y0), Math.ceil(y1))];
}

// A scene without one kind of paint (the node fields that draw it removed)
const STRIP = {
	// Contextual strokes of a text node go with the text (without text they would become border strokes)
	text: node => ({ ...node, text: undefined, caret: undefined, selection: undefined, strokes: node.text ? (node.strokes || []).filter(stroke => stroke.mode !== "Contextual") : node.strokes }),
	strokes: node => ({ ...node, strokes: (node.strokes || []).filter(stroke => stroke.mode === "Contextual" && node.text) }),
	images: node => ({ ...node, image: undefined }),
	shadows: node => ({ ...node, shadows: undefined }),
	gradients: node => ({ ...node, gradient: undefined }),
	fills: node => ({ ...node, bg: node.bg ? { ...node.bg, transparency: 1 } : node.bg, border: undefined, interaction: undefined }),
	scroll: node => ({ ...node, scroll: undefined }),
};

// Renders the scene of `renderDir` (a render output directory) with each paint kind removed; returns
// { width, height, mask (one byte of kind bits per pixel), bounds: { id: [x0, y0, x1, y1] } }. The
// masks depend only on the scene, its manifest and the renderer build: cached beside the render
// (paint-masks.json) under a hash of those, so an unchanged story skips its renders
async function paintMasks(renderDir) {
	const { ensureEngine } = await import("../../cli/engine.mjs");
	const engine = await ensureEngine();
	const hashOf = files => createHash("sha1").update(files.map(file => {
		try {
			return fs.readFileSync(file);
		} catch {
			return "";
		}
	}).join("\0")).digest("hex");
	const key = hashOf([path.join(renderDir, "scene.json"), path.join(renderDir, "manifest.json"), fileURLToPath(import.meta.url)]) + path.basename(engine);
	const cacheFile = path.join(renderDir, "paint-masks.json");
	try {
		const cached = JSON.parse(fs.readFileSync(cacheFile, "utf8"));
		if (cached.key === key) return { ...cached.result, mask: new Uint8Array(Buffer.from(cached.result.mask, "base64")) };
	} catch {
		// render them
	}
	const result = await renderPaintMasks(renderDir, engine);
	fs.writeFileSync(cacheFile, JSON.stringify({ key, result: { ...result, mask: Buffer.from(result.mask).toString("base64") } }));
	return result;
}

async function renderPaintMasks(renderDir, executable) {
	const scene = JSON.parse(fs.readFileSync(path.join(renderDir, "scene.json"), "utf8"));
	const manifest = JSON.parse(fs.readFileSync(path.join(renderDir, "manifest.json"), "utf8"));
	// The variants sit in a directory of their own with the render's manifest, its files one level up
	const work = path.join(renderDir, "paint-masks");
	fs.rmSync(work, { recursive: true, force: true });
	fs.mkdirSync(work, { recursive: true });
	const up = src => (typeof src === "string" ? `../${src}` : src);
	const moved = { ...manifest, images: Object.fromEntries(Object.entries(manifest.images || {}).map(([uri, entry]) => [uri, entry && entry.src ? { ...entry, src: up(entry.src) } : entry])), atlas: manifest.atlas ? { ...manifest.atlas, src: up(manifest.atlas.src) } : null };
	fs.writeFileSync(path.join(work, "manifest.json"), JSON.stringify(moved));
	const variants = [["full", scene], ...KINDS.map(kind => [kind, { ...scene, nodes: asArray(scene.nodes).map(STRIP[kind]) }])];
	for (const [name, variant] of variants) fs.writeFileSync(path.join(work, `${name}.json`), JSON.stringify(variant));
	const captures = await renderScenes(variants.map(([name]) => ({ scene: path.join(work, `${name}.json`), out: path.join(work, name) })), { jobs: 1, executable });
	const failed = captures.find(capture => !capture.ok);
	if (failed) throw new Error(`paint masks: ${failed.error}`);
	const pixels = variants.map(([name]) => PNG.sync.read(fs.readFileSync(path.join(work, name, "render.png"))));
	fs.rmSync(work, { recursive: true, force: true });
	const [full, ...stripped] = pixels;
	const count = full.width * full.height;
	const mask = new Uint8Array(count);
	stripped.forEach((image, bit) => {
		const data = image.data, base = full.data;
		for (let index = 0, k = 0; index < count; index++, k += 4) {
			if (data[k] !== base[k] || data[k + 1] !== base[k + 1] || data[k + 2] !== base[k + 2]) mask[index] |= 1 << bit;
		}
	});
	const S = scene.viewport.scale;
	const bounds = {};
	for (const node of asArray(scene.nodes)) bounds[node.id] = deviceBounds(node, S);
	return { width: full.width, height: full.height, mask, bounds };
}

// Attribution of one compare directory

function readPng(file) {
	return PNG.sync.read(fs.readFileSync(file));
}

// Draw order: node ids, first drawn first (CanvasGroup entries nest)
function drawOrder(scene) {
	const order = [];
	const collect = entries => {
		for (const entry of entries || []) {
			if (entry && typeof entry === "object") {
				collect(entry.draw);
			} else if (typeof entry === "number" && entry > 0) {
				order.push(entry);
			}
		}
	};
	const layers = [...(scene.layers || [])].sort((a, b) => (a.playerGui ? 0 : 1) - (b.playerGui ? 0 : 1) || (a.displayOrder || 0) - (b.displayOrder || 0));
	for (const layer of layers) {
		collect(layer.draw);
	}
	return order;
}

// The paint kinds a node draws itself
function nodeKinds(node) {
	let bits = 0;
	if (node.text) {
		bits |= BIT.text;
	}
	if ((node.strokes || []).some(stroke => !(stroke.mode === "Contextual" && node.text))) {
		bits |= BIT.strokes;
	}
	if (node.image) {
		bits |= BIT.images;
	}
	if (node.shadows && node.shadows.length) {
		bits |= BIT.shadows;
	}
	if (node.gradient) {
		bits |= BIT.gradients;
	}
	if ((node.bg && node.bg.transparency < 0.999) || node.border || node.interaction) {
		bits |= BIT.fills;
	}
	if (node.scroll) {
		bits |= BIT.scroll;
	}
	return bits;
}

// A CanvasGroup Studio does not composite 1:1: fractional device rect (resampled), faded or tinted
function groupComposites(node, scale) {
	if (!node.group) {
		return false;
	}
	const fractional = node.rect.some(value => Math.abs(value * scale - Math.round(value * scale)) > 1e-3) || Boolean(node.matrix);
	const color = node.group.color || [1, 1, 1];
	return fractional || (node.group.transparency || 0) > 0 || color.some(value => value < 0.999);
}

// Maps a device pixel position into the node's unrotated device space (identity without a matrix)
function localPoint(node, scale) {
	if (!node.matrix) {
		return (x, y) => [x, y];
	}
	const [a, b, c, d, e, f] = node.matrix;
	const det = a * d - b * c || 1;
	return (x, y) => {
		const px = x - e * scale, py = y - f * scale;
		return [(d * px - c * py) / det, (a * py - b * px) / det];
	};
}

// The inner edge of a node's border strokes: inset = the deepest inward extent + 2 px, corner = the
// size of the corner squares where rounded strokes bend inwards
function strokeBand(node, scale) {
	let inset = 0, outer = 0;
	for (const stroke of node.strokes || []) {
		if (stroke.mode === "Contextual" && node.text) {
			continue;
		}
		const thickness = (stroke.thickness || 0) * scale;
		inset = Math.max(inset, stroke.position === "Inner" ? thickness : stroke.position === "Center" ? thickness / 2 : 0);
		outer = Math.max(outer, Math.abs(stroke.borderOffset || 0) * scale);
	}
	inset += outer + 2;
	const radius = (Array.isArray(node.corner) ? Math.max(0, ...node.corner) : node.corner || 0) * scale;
	return { inset, corner: radius + inset };
}

function fillBox(map, width, height, box, value, only = null) {
	const x0 = Math.max(0, Math.floor(box[0])), y0 = Math.max(0, Math.floor(box[1]));
	const x1 = Math.min(width, Math.ceil(box[2])), y1 = Math.min(height, Math.ceil(box[3]));
	for (let y = y0; y < y1; y++) {
		for (let x = x0; x < x1; x++) {
			const index = y * width + x;
			if (!only || only[index]) {
				map[index] = value;
			}
		}
	}
}

export async function attributeDir(dir, options = {}) {
	const threshold = options.threshold ?? 2;
	const report = JSON.parse(fs.readFileSync(path.join(dir, "report.json"), "utf8"));
	const layout = JSON.parse(fs.readFileSync(path.join(dir, "layout-full.json"), "utf8"));
	const scene = JSON.parse(fs.readFileSync(path.join(dir, "rur", "scene.json"), "utf8"));
	const studio = readPng(path.join(dir, "studio.png"));
	const ours = readPng(path.join(dir, "rur", "render.png"));
	const scale = scene.viewport.scale;
	const width = Math.min(studio.width, ours.width), height = Math.min(studio.height, ours.height);

	// Offline re-runs: same time (determinism) and one second later (animation)
	let unstable = new Set(), animated = new Set(), later = null;
	const time = Number((report.options && report.options.time) ?? 1);
	if (options.runs !== false && report.source && report.source.endsWith(".luau") && !/fixtures[\\/]/.test(report.source)) {
		// The compare's offline run replayed Studio's recorded clock when it had one (cli/clock.mjs): the
		// same-time run replays it too (unstable = the run itself differs); animated nodes come from two
		// ideal-clock runs at time and time + 1 s (a recorded clock ends at its capture)
		const clock = report.options && report.options.clock ? report.options.clock : null;
		const [again, now, next] = await Promise.all([
			runStory(report, time, path.join(dir, "attrib", "same"), clock),
			clock ? runStory(report, time, path.join(dir, "attrib", "ideal")) : Promise.resolve(scene),
			runStory(report, time + 1, path.join(dir, "attrib", "later")),
		]);
		// Rects are left out: another agent's layout edit between the compare and this run moves them
		if (again) {
			unstable = changedKeys(scene, again, contentSignature);
		}
		if (now && next) {
			animated = changedKeys(now, next);
			later = next;
		}
	}

	const masks = await paintMasks(path.join(dir, "rur"));

	// Layout rows by relative path + occurrence (the order compareLayout pairs same-path siblings in is by
	// geometry; rows keep one entry per Studio node, so pair rows to nodes by nearest rect)
	const nodesByKey = keyedNodes(scene);
	const keyOf = new Map();
	for (const [key, node] of nodesByKey) {
		keyOf.set(node.id, key);
	}
	const rowsByPath = new Map();
	for (const row of layout.rows) {
		if (!rowsByPath.has(row.path)) {
			rowsByPath.set(row.path, []);
		}
		rowsByPath.get(row.path).push(row);
	}
	const rowOf = new Map();
	for (const [key, node] of nodesByKey) {
		const relative = key.replace(/#\d+$/, "");
		const candidates = (rowsByPath.get(relative) || []).filter(row => row.ours || row.extra);
		let best = null, bestError = Infinity;
		for (const row of candidates) {
			const rect = row.ours;
			const [x, y] = node.absolute || node.rect;
			const error = Math.max(Math.abs(rect[0] - x), Math.abs(rect[1] - y), Math.abs(rect[2] - node.rect[2]), Math.abs(rect[3] - node.rect[3]));
			if (error < bestError && !row.taken) {
				best = row;
				bestError = error;
			}
		}
		if (best) {
			best.taken = true;
			rowOf.set(node.id, best);
		}
	}

	const nodeById = new Map(scene.nodes.map(node => [node.id, node]));
	const statusCache = new Map();
	// A compare that replayed Studio's recorded clock shows Studio's moment offline: a node that animates
	// is then in sync unless its dumped state (props, rect) differs
	const replayed = Boolean(report.clock && report.clock.replayed !== false && report.options && report.options.clock);
	// Own status of a node: content / animation / layout / null
	const ownStatus = node => {
		if (statusCache.has(node.id)) {
			return statusCache.get(node.id);
		}
		const key = keyOf.get(node.id);
		const row = rowOf.get(node.id);
		let status = null;
		if ((row && (row.content || row.extra)) || (key && unstable.has(key))) {
			status = "content";
		} else if ((key && animated.has(key) && (!replayed || (row && (row.error || 0) >= 0.01))) || (row && row.props)) {
			status = "animation";
		} else if (row && ((row.error || 0) >= 0.01 || (row.textError || 0) >= 0.01)) {
			status = "layout";
		}
		statusCache.set(node.id, status);
		return status;
	};
	// Status including the CanvasGroups that composite the node (a faded group mid-tween fades all of it)
	const effectiveStatus = node => {
		const own = ownStatus(node);
		if (own) {
			return { status: own, via: node };
		}
		let parent = nodeById.get(node.parent);
		while (parent) {
			if (parent.group) {
				const status = ownStatus(parent);
				if (status === "animation" || status === "content") {
					return { status, via: parent };
				}
			}
			parent = nodeById.get(parent.parent);
		}
		return { status: null, via: null };
	};
	const compositingGroup = node => {
		let parent = nodeById.get(node.parent);
		while (parent) {
			if (groupComposites(parent, scale)) {
				return parent;
			}
			parent = nodeById.get(parent.parent);
		}
		return null;
	};

	// Owner map: topmost node whose own paint covers the pixel. The masks are global (every node's paint of
	// a kind), so each kind is claimed only inside the node's own geometry: fills and images inside its
	// snapped rect (cut by its clip), text, strokes, shadows and scroll bars inside its device bounds.
	// Gradients only modify a paint (never claim).
	const order = drawOrder(scene);
	const owner = new Int32Array(width * height).fill(-1);
	const mask = masks.mask;
	const snap = value => Math.floor(value + 0.5);
	const snappedBox = node => {
		const [x, y, w, h] = node.rect;
		let box = [snap(x * scale), snap(y * scale), snap((x + w) * scale), snap((y + h) * scale)];
		if (node.clip) {
			const [cx, cy, cw, ch] = node.clip;
			box = [Math.max(box[0], snap(cx * scale)), Math.max(box[1], snap(cy * scale)), Math.min(box[2], snap((cx + cw) * scale)), Math.min(box[3], snap((cy + ch) * scale))];
		}
		return box;
	};
	// The kinds a node can have painted at a device pixel: fills and images inside its snapped rect (rotated:
	// its rotated rect, cut by its clip), border strokes in their band (not deep inside the frame, where other
	// nodes' strokes paint), text, shadows and scroll bars in its device bounds. Masks are global (every
	// node's paint of a kind), so a node claims a mask bit only where it can have painted it.
	const claimCache = new Map();
	const claimsOf = node => {
		if (claimCache.has(node.id)) {
			return claimCache.get(node.id);
		}
		const bounds = masks.bounds[node.id];
		const kinds = nodeKinds(node) & ~BIT.gradients;
		const inner = node.matrix ? bounds : snappedBox(node);
		const local = localPoint(node, scale);
		const [rx, ry, rw, rh] = node.rect.map(value => value * scale);
		const band = strokeBand(node, scale);
		const inBox = (box, x, y) => x >= box[0] && y >= box[1] && x < box[2] && y < box[3];
		const claims = (x, y) => {
			if (!bounds || !inBox(bounds, x, y)) {
				return 0;
			}
			let bits = kinds & (BIT.text | BIT.shadows | BIT.scroll);
			if (kinds & (BIT.fills | BIT.images | BIT.strokes)) {
				const [lx, ly] = local(x + 0.5, y + 0.5);
				if (kinds & (BIT.fills | BIT.images) && inBox(inner, x, y) && (!node.matrix || (lx >= rx - 1.5 && ly >= ry - 1.5 && lx <= rx + rw + 1.5 && ly <= ry + rh + 1.5))) {
					bits |= kinds & (BIT.fills | BIT.images);
				}
				if (kinds & BIT.strokes) {
					const dx = Math.min(lx - rx, rx + rw - lx), dy = Math.min(ly - ry, ry + rh - ly);
					// Inside the band's inner edge only where rounded corners bend the band inwards
					if (dx < band.inset || dy < band.inset || (dx < band.corner && dy < band.corner)) {
						bits |= BIT.strokes;
					}
				}
			}
			return bits;
		};
		const entry = { bounds, kinds, claims };
		claimCache.set(node.id, entry);
		return entry;
	};
	order.forEach((id, index) => {
		const node = nodeById.get(id);
		if (!node || !masks.bounds[id] || node.visible === false) {
			return;
		}
		const { bounds, kinds, claims } = claimsOf(node);
		if (!kinds) {
			return;
		}
		const x0 = Math.max(0, bounds[0]), y0 = Math.max(0, bounds[1]), x1 = Math.min(width, bounds[2]), y1 = Math.min(height, bounds[3]);
		for (let y = y0; y < y1; y++) {
			for (let x = x0; x < x1; x++) {
				const bits = mask[y * masks.width + x] & kinds;
				if (bits && bits & claims(x, y)) {
					owner[y * width + x] = index;
				}
			}
		}
	});
	// Text owners (topmost node whose own text ink covers the pixel, whatever is drawn over it): Studio's
	// glyphs sitting a pixel further out than ours differ where only the paint under the text is ours
	// (text halo), and text under a shadow shows through it
	const textOwner = new Int32Array(width * height).fill(-1);
	order.forEach((id, index) => {
		const node = nodeById.get(id);
		const bounds = masks.bounds[id];
		if (!node || !bounds || node.visible === false || !(nodeKinds(node) & BIT.text)) {
			return;
		}
		for (let y = Math.max(0, bounds[1]); y < Math.min(height, bounds[3]); y++) {
			for (let x = Math.max(0, bounds[0]); x < Math.min(width, bounds[2]); x++) {
				if (mask[y * masks.width + x] & BIT.text) {
					textOwner[y * width + x] = index;
				}
			}
		}
	});
	const HALO = 2;
	// below: the pixel's owner (text drawn over it counts), or -1 when the owner only shades the pixel
	const haloOwner = (x, y, below) => {
		let best = -1;
		for (let yy = Math.max(0, y - HALO); yy <= Math.min(height - 1, y + HALO); yy++) {
			for (let xx = Math.max(0, x - HALO); xx <= Math.min(width - 1, x + HALO); xx++) {
				const candidate = textOwner[yy * width + xx];
				if (candidate > below && candidate > best) {
					best = candidate;
				}
			}
		}
		return best;
	};

	// Where shifted, animated and Studio-only nodes paint in Studio (claims over clean owners)
	const claims = new Int32Array(width * height).fill(-1);
	const claimInfo = [];
	const claim = (box, cause, label) => {
		claimInfo.push({ cause, label });
		fillBox(claims, width, height, box, claimInfo.length - 1);
	};
	order.forEach(id => {
		const node = nodeById.get(id);
		const row = rowOf.get(id);
		const bounds = masks.bounds[id];
		if (!node || !bounds || !nodeKinds(node)) {
			return;
		}
		const status = ownStatus(node);
		const label = keyOf.get(id) || node.path;
		if (status === "layout" && row && row.studio && row.ours) {
			const shift = [row.studio[0] - row.ours[0], row.studio[1] - row.ours[1], row.studio[0] + row.studio[2] - row.ours[0] - row.ours[2], row.studio[1] + row.studio[3] - row.ours[1] - row.ours[3]].map(v => v * scale);
			claim([bounds[0] + shift[0], bounds[1] + shift[1], bounds[2] + shift[2], bounds[3] + shift[3]], "layout", label);
		} else if (status === "animation" && later) {
			const other = keyedNodes(later).get(keyOf.get(id));
			if (other) {
				const [x, y, w, h] = other.rect;
				const grow = [bounds[0] - node.rect[0] * scale, bounds[1] - node.rect[1] * scale, bounds[2] - (node.rect[0] + node.rect[2]) * scale, bounds[3] - (node.rect[1] + node.rect[3]) * scale];
				claim([x * scale + grow[0], y * scale + grow[1], (x + w) * scale + grow[2], (y + h) * scale + grow[3]], "animation", label);
			}
		}
	});
	for (const row of layout.rows) {
		if (row.missing && row.studio && row.visible !== false) {
			const [x, y, w, h] = row.studio;
			claim([x * scale - 2, y * scale - 2, (x + w) * scale + 2, (y + h) * scale + 2], "content", `${row.path} (Studio only)`);
		}
	}

	// Classify the differing pixels
	const counts = Object.fromEntries(CAUSES.map(cause => [cause, 0]));
	const severe = Object.fromEntries(CAUSES.map(cause => [cause, 0]));
	const byNode = new Map();
	// --map: the cause of every differing pixel (attrib/causes.png, colours in CAUSE_COLORS)
	const causeMap = options.map ? new Uint8Array(width * height).fill(255) : null;
	let current = -1;
	const note = (cause, label, big) => {
		counts[cause]++;
		if (causeMap && current >= 0) {
			causeMap[current] = CAUSES.indexOf(cause);
		}
		if (big) {
			severe[cause]++;
		}
		const key = `${cause}|${label}`;
		const entry = byNode.get(key) || { cause, node: label, pixels: 0, over8: 0 };
		entry.pixels++;
		if (big) {
			entry.over8++;
		}
		byNode.set(key, entry);
	};
	let differing = 0, over8 = 0;
	const textPixels = new Map();
	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			const ia = (y * studio.width + x) * 4, ib = (y * ours.width + x) * 4;
			let max = 0;
			for (let c = 0; c < 3; c++) {
				const d = Math.abs(studio.data[ia + c] - ours.data[ib + c]);
				if (d > max) {
					max = d;
				}
			}
			if (max <= threshold) {
				continue;
			}
			differing++;
			const big = max > 8;
			if (big) {
				over8++;
			}
			const index = y * width + x;
			current = index;
			const ownerIndex = owner[index];
			const node = ownerIndex >= 0 ? nodeById.get(order[ownerIndex]) : null;
			const label = node ? keyOf.get(node.id) || node.path : null;
			if (node) {
				const { status, via } = effectiveStatus(node);
				if (status) {
					note(status, via === node ? label : `${keyOf.get(via.id) || via.path} (group of ${label})`, big);
					continue;
				}
			}
			const claimIndex = claims[index];
			if (claimIndex >= 0) {
				note(claimInfo[claimIndex].cause, claimInfo[claimIndex].label, big);
				continue;
			}
			if (!node) {
				note("unowned", "(no node paints here)", big);
				continue;
			}
			// Gradients modify the node's own paint wherever it claims one
			const claimed = claimsOf(node).claims(x, y);
			const bits = mask[y * masks.width + x] & (claimed | (claimed & (BIT.fills | BIT.images | BIT.strokes | BIT.text) ? nodeKinds(node) & BIT.gradients : 0));
			if (!(bits & BIT.text)) {
				// Text halo: text of a node drawn above this pixel's owner lies within 2 px
				const halo = haloOwner(x, y, bits & ~BIT.shadows ? ownerIndex : -1);
				if (halo >= 0) {
					const textNode = nodeById.get(order[halo]);
					const { status, via } = effectiveStatus(textNode);
					if (status) {
						note(status, via === textNode ? keyOf.get(textNode.id) || textNode.path : `${keyOf.get(via.id) || via.path} (group of ${keyOf.get(textNode.id) || textNode.path})`, big);
						continue;
					}
					if (causeMap) {
						causeMap[index] = CAUSES.indexOf("text");
					}
					if (!textPixels.has(halo)) {
						textPixels.set(halo, []);
					}
					textPixels.get(halo).push(big);
					continue;
				}
			}
			const group = compositingGroup(node);
			if (group) {
				note("canvasgroups", `${keyOf.get(group.id) || group.path} (${label})`, big);
				continue;
			}
			// The paint on top in SceneRenderer.drawNode order (scroll bars in the post pass, then border strokes
			// over text over the image over the background over shadows); a gradient fill is a gradient
			let kind = ["scroll", "strokes", "text", "images", "fills", "shadows"].find(name => bits & BIT[name]) || "fills";
			if (kind === "fills" && bits & BIT.gradients) {
				kind = "gradients";
			}
			if (kind === "text") {
				if (causeMap) {
					causeMap[index] = CAUSES.indexOf("text");
				}
				// Decided per node below (glyph placement or rasterization)
				if (!textPixels.has(ownerIndex)) {
					textPixels.set(ownerIndex, []);
				}
				textPixels.get(ownerIndex).push(big);
				continue;
			}
			note(kind, label, big);
		}
	}

	// Text placement: when moving our glyphs by up to 2 px removes most of a node's text differences, the
	// glyphs sit elsewhere in Studio (text layout: pen position, baseline, size) rather than rasterizing
	// differently
	const pixelDiff = (x, y, dx, dy) => {
		const ox = x - dx, oy = y - dy;
		if (ox < 0 || oy < 0 || ox >= width || oy >= height) {
			return 255;
		}
		const ia = (y * studio.width + x) * 4, ib = (oy * ours.width + ox) * 4;
		return Math.max(Math.abs(studio.data[ia] - ours.data[ib]), Math.abs(studio.data[ia + 1] - ours.data[ib + 1]), Math.abs(studio.data[ia + 2] - ours.data[ib + 2]));
	};
	let placementNodes = 0;
	for (const [ownerIndex, list] of textPixels) {
		const node = nodeById.get(order[ownerIndex]);
		const label = keyOf.get(node.id) || node.path;
		current = -1;
		let placement = null;
		if (list.length >= 24) {
			const bounds = masks.bounds[node.id];
			const pixels = [];
			for (let y = Math.max(0, bounds[1]); y < Math.min(height, bounds[3]); y++) {
				for (let x = Math.max(0, bounds[0]); x < Math.min(width, bounds[2]); x++) {
					if (owner[y * width + x] === ownerIndex && mask[y * masks.width + x] & BIT.text) {
						pixels.push(x, y);
					}
				}
			}
			const countAt = (dx, dy) => {
				let count = 0;
				for (let k = 0; k < pixels.length; k += 2) {
					if (pixelDiff(pixels[k], pixels[k + 1], dx, dy) > threshold) {
						count++;
					}
				}
				return count;
			};
			const base = countAt(0, 0);
			let best = { dx: 0, dy: 0, count: base };
			for (let dy = -2; dy <= 2; dy++) {
				for (let dx = -2; dx <= 2; dx++) {
					if (dx || dy) {
						const count = countAt(dx, dy);
						if (count < best.count) {
							best = { dx, dy, count };
						}
					}
				}
			}
			if (best.count < 0.5 * base) {
				placement = best;
			}
		}
		if (placement) {
			placementNodes++;
		}
		for (const big of list) {
			if (placement) {
				note("layout", `${label} (text placement ${placement.dx},${placement.dy})`, big);
			} else {
				note("text", label, big);
			}
		}
	}

	if (causeMap) {
		const map = new PNG({ width, height });
		for (let index = 0; index < width * height; index++) {
			const cause = causeMap[index];
			const gray = Math.round(ours.data[(Math.floor(index / width) * ours.width + index % width) * 4] * 0.25);
			map.data.set([...(cause === 255 ? [gray, gray, gray] : CAUSE_COLORS[CAUSES[cause]]), 255], index * 4);
		}
		fs.mkdirSync(path.join(dir, "attrib"), { recursive: true });
		fs.writeFileSync(path.join(dir, "attrib", "causes.png"), PNG.sync.write(map));
	}

	const total = width * height;
	const result = {
		dir: path.relative(ROOT, dir).replace(/\\/g, "/"),
		source: report.source,
		threshold,
		total,
		differing,
		over8,
		exactPercent: 100 * (1 - differing / total),
		causes: counts,
		causesOver8: severe,
		// Percentage points of the image each cause costs (they sum to 100 - exactPercent)
		points: Object.fromEntries(CAUSES.map(cause => [cause, +(100 * counts[cause] / total).toFixed(4)])),
		unstableNodes: unstable.size,
		animatedNodes: animated.size,
		textPlacementNodes: placementNodes,
		nodes: [...byNode.values()].sort((a, b) => b.pixels - a.pixels).slice(0, 60),
	};
	fs.writeFileSync(path.join(dir, "attribution.json"), JSON.stringify(result, null, 1));
	return result;
}

// Directories are independent: they run concurrently (options.jobs, RUR_JOBS; default from the core
// count), results and log lines in the given order
export async function attribute(dirs, options = {}) {
	const jobs = Math.max(1, Number(options.jobs) || Number(process.env.RUR_JOBS) || Math.min(8, Math.max(2, Math.floor(os.availableParallelism() / 4))));
	const results = new Array(dirs.length);
	let next = 0, printed = 0;
	const report = () => {
		while (printed < dirs.length && results[printed]) {
			const last = results[printed];
			if (!options.quiet) {
				process.stderr.write(last.error ? `${dirs[printed]}: ${last.error.split("\n")[0]}\n` : `${last.dir}: ${last.differing} px (${(100 - last.exactPercent).toFixed(3)}%) ${CAUSES.filter(cause => last.causes[cause]).map(cause => `${cause} ${last.causes[cause]}`).join(", ")}\n`);
			}
			printed++;
		}
	};
	try {
		await Promise.all(Array.from({ length: Math.min(jobs, dirs.length) }, async () => {
			while (next < dirs.length) {
				const index = next++;
				const dir = dirs[index];
				try {
					results[index] = await attributeDir(path.resolve(dir), options);
				} catch (error) {
					results[index] = { dir, error: String(error && error.stack || error) };
				}
				report();
			}
		}));
	} finally {
		// (nothing held between directories)
	}
	return results;
}

// CLI
if (isMain(import.meta.url)) {
	const argv = process.argv.slice(2);
	const option = (name, fallback) => {
		const index = argv.indexOf(`--${name}`);
		return index >= 0 ? argv[index + 1] : fallback;
	};
	const dirs = argv.filter((arg, index) => !arg.startsWith("--") && !(index > 0 && argv[index - 1].startsWith("--") && !["--no-runs", "--quiet", "--map"].includes(argv[index - 1])));
	const results = await attribute(dirs, { threshold: Number(option("threshold", 2)), runs: !argv.includes("--no-runs"), quiet: argv.includes("--quiet"), map: argv.includes("--map") });
	for (const result of results) {
		if (result.error) {
			continue;
		}
		console.log(`\n${result.dir}: ${result.differing} px differ (${(100 - result.exactPercent).toFixed(3)}%), unstable ${result.unstableNodes}, animated ${result.animatedNodes}`);
		for (const entry of result.nodes.slice(0, 15)) {
			console.log(`  ${String(entry.pixels).padStart(7)} (${String(entry.over8).padStart(6)} >8)  ${entry.cause.padEnd(12)} ${entry.node}`);
		}
	}
}
