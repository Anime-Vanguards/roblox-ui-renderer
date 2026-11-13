#!/usr/bin/env node
// Text-first inspection of a rendered scene, for agents debugging UI without Studio.
//
//   node cli/inspect.mjs tree <render dir|scene.json> [--filter text] [--depth n] [--all]
//                  (--filter keeps each match's ancestry, matches marked *, hidden ones included)
//   node cli/inspect.mjs node <render dir|scene.json> <path substring>      full JSON of matching nodes
//   node cli/inspect.mjs at   <render dir|scene.json> <x,y>                 nodes under a point (logical px, topmost first)
//   node cli/inspect.mjs crop <render dir> <path substring> [--pad 8] [--zoom 2] [--out file.png]
//   node cli/inspect.mjs diff <render dir A> <render dir B> [--out diff.png]  layout + pixel differences between two renders
//
// A render dir is what `rur render` writes (scene.json + render.png); `rur compare` puts one at
// out/compare/<name>/rur. Paths match against instance paths such as StoryTarget.Stack.Button.Label;
// names holding dots are written quoted (Holder["Item.Name"].Label), and either form matches.
// Nodes the render left out or drew incompletely are marked [culled: ...], [N wrapped lines dropped ...],
// [missing glyph ...] (cli/diagnostics.mjs; `rur render` counts them in its summary).

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";
import { pathMatches, sceneDiagnostics } from "./diagnostics.mjs";
import { isMain } from "./entry.mjs";

function parseArgs(argv) {
	const args = { _: [] };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg.startsWith("--")) {
			const key = arg.slice(2);
			const next = argv[i + 1];
			if (next === undefined || next.startsWith("--")) {
				args[key] = true;
			} else {
				args[key] = next;
				i++;
			}
		} else {
			args._.push(arg);
		}
	}
	return args;
}

const asArray = value => (Array.isArray(value) ? value : []);

function loadScene(target) {
	const file = fs.statSync(target).isDirectory() ? path.join(target, "scene.json") : target;
	const scene = JSON.parse(fs.readFileSync(file, "utf8"));
	const dir = path.dirname(file);
	const nodes = asArray(scene.nodes);
	const byId = new Map(nodes.map(node => [node.id, node]));
	const children = new Map();
	for (const node of nodes) {
		if (!children.has(node.parent)) children.set(node.parent, []);
		children.get(node.parent).push(node);
	}
	SCALE = (scene.viewport && scene.viewport.scale) || 1;
	// Diagnostic notes and unambiguous paths, kept per node object (diff loads two scenes)
	const { notes, paths } = sceneDiagnostics(scene);
	for (const node of nodes) {
		if (notes.has(node.id)) NOTES.set(node, notes.get(node.id));
		PATHS.set(node, paths.get(node.id));
	}
	return { scene, dir, nodes, byId, children };
}

function readPng(file) {
	return PNG.sync.read(fs.readFileSync(file));
}

const round = value => Math.round(value * 100) / 100;
let SCALE = 1;
const NOTES = new WeakMap();
const PATHS = new WeakMap();
// A node's path with names holding dots quoted (Holder["Item.Name"].Label)
const pathOf = node => PATHS.get(node) || node.path;
const hex = color => (color ? "#" + color.map(v => Math.round(v * 255).toString(16).padStart(2, "0")).join("") : "");

// One-line summary of a node's visuals
function describe(node) {
	// Names that would make a path ambiguous are quoted
	const parts = [`${node.class} ${/[.[\]"]/.test(node.name) ? JSON.stringify(node.name) : node.name}`, `[${node.rect.map(round).join(", ")}]`];
	if (node.visible === false) parts.push("hidden");
	// LayoutOrder / ZIndex when not the default (0 / 1)
	if (node.layoutOrder) parts.push(`layoutOrder=${node.layoutOrder}`);
	if (node.zIndex !== undefined && node.zIndex !== 1 && !("displayOrder" in node)) parts.push(`z=${node.zIndex}`);
	if (node.rotation) parts.push(`rot=${round(node.rotation)}`);
	if (node.bg && node.bg.transparency < 1) parts.push(`bg=${hex(node.bg.color)}${node.bg.transparency ? `@${round(1 - node.bg.transparency)}` : ""}`);
	if (node.corner) parts.push(`corner=${Array.isArray(node.corner) ? node.corner.map(round).join("/") : round(node.corner)}`);
	for (const stroke of asArray(node.strokes)) parts.push(`stroke=${stroke.mode === "Contextual" && node.text ? "text:" : ""}${round(stroke.thickness)}${hex(stroke.color)}${stroke.transparency ? `@${round(1 - stroke.transparency)}` : ""}`);
	if (node.gradient) parts.push("gradient");
	for (const shadow of asArray(node.shadows)) parts.push(`shadow=blur${round(shadow.blur)}${hex(shadow.color)}`);
	if (node.image) parts.push(`image=${node.image.uri}${node.image.scaleType && node.image.scaleType !== "Stretch" ? ` ${node.image.scaleType}` : ""}`);
	if (node.group) parts.push(`group${node.group.transparency ? `@${round(1 - node.group.transparency)}` : ""}`);
	if (node.text) {
		const props = node.props || {};
		const text = String(props.text ?? "").replace(/\n/g, "\\n");
		parts.push(`text=${JSON.stringify(text.length > 60 ? text.slice(0, 57) + "..." : text)} size=${node.text.textSize} bounds=${round(node.text.width / SCALE)}x${round(node.text.height / SCALE)}`);
	}
	if (node.path2d) parts.push(`path=${round(node.path2d.thickness)}${hex(node.path2d.color)} segments=${node.path2d.segments.length}${node.path2d.closed ? " closed" : ""}`);
	if (node.scroll) parts.push(`scroll canvas=${node.scroll.canvas.slice(2).map(round).join("x")}`);
	for (const note of NOTES.get(node) || []) parts.push(`[${note}]`);
	return parts.join(" ");
}

export function treeLines(target, args = {}) {
	const { nodes, byId, children } = loadScene(target);
	const filter = args.filter && args.filter !== true ? String(args.filter).toLowerCase() : null;
	const maxDepth = args.depth ? Number(args.depth) : Infinity;
	const roots = nodes.filter(node => node.parent == null || !byId.has(node.parent));
	// --filter: the matches (hidden ones included) and their ancestors, so each match shows where it
	// sits; matches are marked "*"
	let shown = null;
	const matches = new Set();
	if (filter) {
		shown = new Set();
		for (const node of nodes) {
			if (!(pathMatches(node, pathOf(node), filter) || describe(node).toLowerCase().includes(filter))) continue;
			matches.add(node.id);
			for (let current = node; current && !shown.has(current.id); current = byId.get(current.parent)) shown.add(current.id);
		}
	}
	const lines = [];
	const visit = (node, depth) => {
		if (depth > maxDepth) return;
		if (shown ? !shown.has(node.id) : !args.all && node.visible === false) return;
		lines.push("  ".repeat(depth) + (matches.has(node.id) ? "* " : "") + describe(node));
		for (const child of children.get(node.id) || []) visit(child, depth + 1);
	};
	for (const root of roots) visit(root, 0);
	return lines;
}

function tree(target, args) {
	const lines = treeLines(target, args);
	if (!lines.length && args.filter) {
		console.error(`no node matches "${args.filter}"`);
		process.exitCode = 1;
		return;
	}
	console.log(lines.join("\n"));
}

function findNodes(nodes, query) {
	return nodes.filter(node => pathMatches(node, pathOf(node), query));
}

function showNode(target, query) {
	const { nodes } = loadScene(target);
	const matches = findNodes(nodes, query);
	if (!matches.length) {
		console.error(`no node path contains "${query}"`);
		process.exitCode = 1;
		return;
	}
	// The diagnostics (culled, dropped lines, missing glyphs) and the unambiguous path when it differs
	const shown = matches.slice(0, 20).map(node => {
		const extra = {};
		if (pathOf(node) !== node.path) extra.unambiguousPath = pathOf(node);
		if (NOTES.has(node)) extra.diagnostics = NOTES.get(node);
		return { ...node, ...extra };
	});
	console.log(JSON.stringify(shown, null, 1));
	if (matches.length > 20) console.error(`(${matches.length - 20} more matches not shown)`);
}

// Flattened draw order (topmost last), expanding CanvasGroup entries
function drawOrder(scene, byId) {
	const order = [];
	const collect = entries => {
		for (const entry of asArray(entries)) {
			if (entry && typeof entry === "object") collect(entry.draw);
			else if (entry > 0 && byId.has(entry)) order.push(byId.get(entry));
		}
	};
	// (the PlayerGui pass, `playerGui` layers of --all-layers, draws under CoreGui's)
	for (const layer of [...asArray(scene.layers)].sort((a, b) => (a.playerGui ? 0 : 1) - (b.playerGui ? 0 : 1) || (a.displayOrder || 0) - (b.displayOrder || 0))) collect(layer.draw);
	return order;
}

function contains(node, x, y) {
	const [nx, ny, nw, nh] = node.rect;
	let px = x, py = y;
	if (node.matrix) {
		// Undo the node's rotation about its transform (matrix maps unrotated → screen, logical units)
		const [a, b, c, d, e, f] = node.matrix;
		const det = a * d - b * c;
		const tx = x - e, ty = y - f;
		px = (d * tx - c * ty) / det;
		py = (-b * tx + a * ty) / det;
	}
	if (px < nx || py < ny || px >= nx + nw || py >= ny + nh) return false;
	if (node.clip) {
		const [cx, cy, cw, ch] = node.clip;
		if (x < cx || y < cy || x >= cx + cw || y >= cy + ch) return false;
	}
	return true;
}

function at(target, point) {
	const { scene, byId } = loadScene(target);
	const [x, y] = String(point).split(",").map(Number);
	const hits = drawOrder(scene, byId).filter(node => contains(node, x, y)).reverse();
	if (!hits.length) {
		console.log(`nothing drawn at ${x},${y}`);
		return;
	}
	for (const node of hits) console.log(`${pathOf(node)}\n    ${describe(node)}`);
}

// How a path holds the query (case-insensitive, either path form): 0 it ends with it at a name (the query
// names the node), 1 whole names inside it (the query names an ancestor), 2 anywhere else
function nameRank(node, query) {
	const needle = String(query).toLowerCase();
	let rank = 2;
	for (const full of [String(node.path), String(pathOf(node))]) {
		const text = full.toLowerCase();
		for (let at = text.indexOf(needle); at !== -1 && rank > 0; at = text.indexOf(needle, at + 1)) {
			if (at > 0 && text[at - 1] !== "." && needle[0] !== "[") continue;
			const end = at + needle.length;
			if (end === text.length) rank = 0;
			else if (text[end] === "." || text[end] === "[") rank = 1;
		}
	}
	return rank;
}

const overlapsImage = ([x, y, w, h], width, height) => x < width && y < height && x + w > 0 && y + h > 0;

// The match `crop` frames: the closest name match (nameRank), and among equals one whose rect reaches
// into the image, then the first
function cropTarget(matches, query, width, height) {
	const order = node => nameRank(node, query) * 2 + (overlapsImage(node.rect, width, height) ? 0 : 1);
	return matches.reduce((best, node) => (order(node) < order(best) ? node : best));
}

function crop(target, query, args) {
	const { nodes, dir, scene } = loadScene(target);
	const matches = findNodes(nodes, query);
	if (!matches.length) {
		console.error(`no node path contains "${query}"`);
		process.exitCode = 1;
		return;
	}
	const scale = (scene.viewport && scene.viewport.scale) || 1;
	const pad = Number(args.pad ?? 8);
	const zoom = Math.max(1, Math.round(Number(args.zoom ?? 2)));
	const png = readPng(path.join(dir, "render.png"));
	const node = cropTarget(matches, query, png.width / scale, png.height / scale);
	const [x, y, w, h] = node.rect;
	const x0 = Math.max(0, Math.floor((x - pad) * scale)), y0 = Math.max(0, Math.floor((y - pad) * scale));
	const x1 = Math.min(png.width, Math.ceil((x + w + pad) * scale)), y1 = Math.min(png.height, Math.ceil((y + h + pad) * scale));
	if (!overlapsImage(node.rect, png.width / scale, png.height / scale) || x1 <= x0 || y1 <= y0) {
		console.error(`${pathOf(node)} is outside the image: rect [${node.rect.map(round).join(", ")}], image ${round(png.width / scale)}x${round(png.height / scale)} (logical px; render.png ${png.width}x${png.height})`);
		process.exitCode = 1;
		return;
	}
	const out = new PNG({ width: (x1 - x0) * zoom, height: (y1 - y0) * zoom });
	for (let yy = 0; yy < out.height; yy++) {
		for (let xx = 0; xx < out.width; xx++) {
			const src = ((y0 + Math.floor(yy / zoom)) * png.width + x0 + Math.floor(xx / zoom)) * 4;
			const dst = (yy * out.width + xx) * 4;
			for (let k = 0; k < 4; k++) out.data[dst + k] = png.data[src + k];
		}
	}
	const file = path.resolve(args.out || path.join(dir, `crop-${node.name}.png`));
	fs.writeFileSync(file, PNG.sync.write(out));
	console.log(`${pathOf(node)}\n    ${describe(node)}\n    crop → ${file}`);
}

function diff(targetA, targetB, args) {
	const result = diffRenders(targetA, targetB, args);
	console.log(result.changes.length ? result.changes.join("\n") : "no layout/visual property differences");
	if (result.pixels) console.log(result.pixels.message);
}

// Layout/visual changes between two renders: { changes: [text], pixels: { changed, total, file,
// message } | null }, writing a diff PNG (changed pixels red/yellow over a gray copy) when both have
// render.png. `rur diff` uses it.
export function diffRenders(targetA, targetB, args = {}) {
	const A = loadScene(targetA), B = loadScene(targetB);
	const byPath = new Map();
	for (const node of B.nodes) {
		if (!byPath.has(pathOf(node))) byPath.set(pathOf(node), []);
		byPath.get(pathOf(node)).push(node);
	}
	const used = new Map();
	const changes = [];
	for (const node of A.nodes) {
		const list = byPath.get(pathOf(node)) || [];
		const k = used.get(pathOf(node)) || 0;
		used.set(pathOf(node), k + 1);
		const other = list[k];
		if (!other) {
			changes.push(`- removed ${pathOf(node)}`);
			continue;
		}
		const moved = node.rect.some((v, i) => Math.abs(v - other.rect[i]) > 0.01);
		const before = describe(node), after = describe(other);
		if (moved || before !== after) changes.push(`~ ${pathOf(node)}\n    A: ${before}\n    B: ${after}`);
	}
	for (const [nodePath, list] of byPath) {
		const k = used.get(nodePath) || 0;
		for (const node of list.slice(k)) changes.push(`+ added ${pathOf(node)}\n    ${describe(node)}`);
	}
	const fileA = path.join(A.dir, "render.png"), fileB = path.join(B.dir, "render.png");
	if (!fs.existsSync(fileA) || !fs.existsSync(fileB)) return { changes, pixels: null };
	const pa = readPng(fileA), pb = readPng(fileB);
	if (pa.width !== pb.width || pa.height !== pb.height) {
		return { changes, pixels: { changed: null, total: null, file: null, message: `pixels: sizes differ (${pa.width}x${pa.height} vs ${pb.width}x${pb.height})` } };
	}
	const out = new PNG({ width: pa.width, height: pa.height });
	let changed = 0;
	for (let i = 0; i < pa.data.length; i += 4) {
		const d = Math.max(Math.abs(pa.data[i] - pb.data[i]), Math.abs(pa.data[i + 1] - pb.data[i + 1]), Math.abs(pa.data[i + 2] - pb.data[i + 2]));
		if (d > 0) changed++;
		const gray = Math.round((pa.data[i] + pa.data[i + 1] + pa.data[i + 2]) / 12);
		out.data[i] = d > 0 ? 255 : gray;
		out.data[i + 1] = d > 0 ? Math.max(0, 255 - d * 4) : gray;
		out.data[i + 2] = d > 0 ? 0 : gray;
		out.data[i + 3] = 255;
	}
	const file = path.resolve(args.out || path.join(B.dir, "diff-vs-A.png"));
	fs.writeFileSync(file, PNG.sync.write(out));
	const total = pa.width * pa.height;
	return { changes, pixels: { changed, total, file, message: `pixels: ${changed} of ${total} differ (${(changed / total * 100).toFixed(3)}%) → ${file}` } };
}

// Also called in-process by tools that load this module (a preview tool embedding rur)
export function main(argv = process.argv.slice(2)) {
	const args = parseArgs(argv);
	const [command, target, extra, extra2] = args._;
	if (command === "tree" && target) tree(target, args);
	else if (command === "node" && target && extra) showNode(target, extra);
	else if (command === "at" && target && extra) at(target, extra);
	else if (command === "crop" && target && extra) crop(target, extra, args);
	else if (command === "diff" && target && extra) diff(target, extra, args);
	else {
		console.log(fs.readFileSync(new URL(import.meta.url), "utf8").split("\n").slice(1, 13).map(line => line.replace(/^\/\/ ?/, "")).join("\n"));
		process.exitCode = command ? 1 : 0;
	}
	void extra2;
}

if (isMain(import.meta.url)) {
	main();
}
