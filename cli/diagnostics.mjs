// Scene diagnostics derived from a rendered scene.json (no engine or renderer changes): what the
// render left out or could not draw, for the render summary (cli/rur.mjs) and cli/inspect.mjs.
//
//   culled nodes     `culled: true` (native export.rs): Studio's AutomaticSize culling (an AutomaticSize
//                    object whose centre is not strictly inside its clip is not drawn, nor its subtree),
//                    or a rotated node wholly outside its clip
//   dropped lines    `text.droppedLines`: wrapped lines the box height left out (Studio drops them;
//                    TextBounds counts only the lines shown)
//   missing glyphs   glyph id 0 (.notdef, drawn as an empty box): the character is in neither the
//                    label's font nor any fallback font
//   script output    diagnostics.output (print / warn / error)
//   unambiguous paths  instance names holding "." (or "[", "]", '"') are written Holder["Item.Name"].Label

import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const asArray = value => (Array.isArray(value) ? value : []);

// Paths

const needsQuote = name => /[.[\]"]/.test(name) || name === "";

function segment(name, first) {
	if (needsQuote(name)) return `[${JSON.stringify(name)}]`;
	return first ? name : `.${name}`;
}

// Unambiguous paths for every node (Map id -> path): names holding dots are quoted, the rest as in
// node.path. The engine's path prefix above the exported roots (e.g. "CoreGui.") is kept.
export function nodePaths(nodes, byId = new Map(nodes.map(node => [node.id, node]))) {
	const paths = new Map();
	const of = node => {
		if (paths.has(node.id)) return paths.get(node.id);
		const parent = byId.get(node.parent);
		let result;
		if (parent) {
			result = of(parent) + segment(node.name, false);
		} else {
			const raw = String(node.path || node.name);
			const name = String(node.name);
			const prefix = raw.endsWith(name) ? raw.slice(0, raw.length - name.length) : "";
			result = needsQuote(name) ? `${prefix.replace(/\.$/, "")}${segment(name, !prefix)}` : raw;
		}
		paths.set(node.id, result);
		return result;
	};
	for (const node of nodes) of(node);
	return paths;
}

// Whether a node's path (as exported, or unambiguous) contains the query (case-insensitive). The
// query may use either form: "Item.Name.Label" and '["Item.Name"].Label' both match.
export function pathMatches(node, unambiguous, query) {
	const needle = String(query).toLowerCase();
	return String(node.path).toLowerCase().includes(needle) || String(unambiguous || "").toLowerCase().includes(needle);
}

// Culling

// Why a culled node is not drawn: the culled root's own rule, or its culled ancestor
export function cullReason(node, byId) {
	if (!node.culled) return null;
	const parent = byId.get(node.parent);
	if (parent && parent.culled) return { root: false, text: "inside a culled ancestor" };
	if (node.rotation) return { root: true, text: "rotated, wholly outside its clip" };
	const [x, y, w, h] = node.rect || [];
	const clip = node.clip;
	if (clip && x !== undefined) {
		const cx = x + w / 2, cy = y + h / 2;
		const inside = cx > clip[0] && cx < clip[0] + clip[2] && cy > clip[1] && cy < clip[1] + clip[3];
		if (!inside) return { root: true, text: "AutomaticSize, centre outside clip" };
	}
	// A text object's own AutomaticSize culling hides its descendants but still draws its text
	return { root: true, text: "AutomaticSize culling of its parent" };
}

// Missing glyphs

const FALLBACK_FILES = ["sourcesanspro-regular", "arimo-regular", "robloxemoji", "twemojimozilla", "notosansthaiui-regular", "notonaskharabicui-regular", "notosansdevanagariui-regular", "123924484074402"];
const fontCache = new Map();

function loadFont(file) {
	if (fontCache.has(file)) return fontCache.get(file);
	let font = null;
	try {
		const require = createRequire(import.meta.url);
		const opentype = require("opentype.js");
		const data = fs.readFileSync(file);
		font = opentype.parse(data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength));
	} catch {
		font = null;
	}
	fontCache.set(file, font);
	return font;
}

const hasChar = (font, char) => {
	try {
		return font.charToGlyphIndex(char) > 0;
	} catch {
		return false;
	}
};

const fontLabel = entry => (entry && entry.file ? path.basename(String(entry.file)).replace(/\.(ttf|otf|ttc)$/i, "") : "unknown font");

// Characters a text node draws as .notdef: [{ char, code, font }]. Null when it has none.
function nodeMissingGlyphs(node, fonts) {
	const notdef = [];
	for (const line of asArray(node.text && node.text.lines)) {
		for (const glyph of asArray(line.glyphs)) {
			if (glyph[1] === 0) notdef.push(glyph[0]);
		}
	}
	if (!notdef.length) return null;
	const primary = fonts[notdef[0]];
	const props = node.props || {};
	let text = String(props.text ?? "");
	if (node.text.rich) text = text.replace(/<[^>]*>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, "\"").replace(/&apos;/g, "'").replace(/&amp;/g, "&");
	const font = primary && primary.file ? loadFont(String(primary.file)) : null;
	const fallbacks = fonts.filter(entry => entry && entry.file && FALLBACK_FILES.some(name => path.basename(String(entry.file)).toLowerCase().includes(name))).map(entry => loadFont(String(entry.file))).filter(Boolean);
	const missing = [];
	const seen = new Set();
	if (font) {
		for (const char of text) {
			if (seen.has(char) || /\s/.test(char)) continue;
			seen.add(char);
			if (hasChar(font, char) || fallbacks.some(fallback => hasChar(fallback, char))) continue;
			missing.push({ char, code: `U+${char.codePointAt(0).toString(16).toUpperCase().padStart(4, "0")}`, font: fontLabel(primary) });
		}
	}
	// The font could not be read: report the count without the characters
	if (!missing.length) missing.push({ char: null, code: null, font: fontLabel(primary), count: notdef.length });
	return missing;
}

// Per-node annotations for inspect (id -> [text]) and the render summary's diagnostics
export function sceneDiagnostics(scene, { examples = 5, outputLines = 20 } = {}) {
	const nodes = asArray(scene && scene.nodes);
	const byId = new Map(nodes.map(node => [node.id, node]));
	const paths = nodePaths(nodes, byId);
	const fonts = asArray(scene && scene.fonts);
	const notes = new Map();
	const note = (node, text) => {
		if (!notes.has(node.id)) notes.set(node.id, []);
		notes.get(node.id).push(text);
	};

	const culledRoots = [];
	let culledNodes = 0;
	const dropped = [];
	const glyphs = new Map();
	for (const node of nodes) {
		const reason = cullReason(node, byId);
		if (reason) {
			culledNodes++;
			note(node, `culled, not drawn: ${reason.text}`);
			if (reason.root) culledRoots.push(`${paths.get(node.id)} (${reason.text})`);
		}
		if (node.text && node.text.droppedLines > 0) {
			const shown = asArray(node.text.lines).length;
			const text = `${node.text.droppedLines} wrapped line${node.text.droppedLines === 1 ? "" : "s"} dropped (${shown} shown; box too short)`;
			note(node, text);
			dropped.push(`${paths.get(node.id)} (${node.text.droppedLines} of ${shown + node.text.droppedLines} lines dropped)`);
		}
		if (node.text) {
			const missing = nodeMissingGlyphs(node, fonts);
			for (const entry of missing || []) {
				note(node, entry.char ? `missing glyph '${entry.char}' ${entry.code} in ${entry.font}` : `${entry.count} missing glyph(s) in ${entry.font}`);
				const key = `${entry.code}|${entry.font}`;
				if (!glyphs.has(key)) glyphs.set(key, { char: entry.char, code: entry.code, font: entry.font, labels: 0, paths: [] });
				const group = glyphs.get(key);
				group.labels++;
				if (group.paths.length < examples) group.paths.push(paths.get(node.id));
			}
		}
	}

	const output = asArray(scene && scene.diagnostics && scene.diagnostics.output);
	return {
		notes,
		paths,
		summary: {
			culled: { count: culledRoots.length, nodes: culledNodes, examples: culledRoots.slice(0, examples) },
			textLinesDropped: { count: dropped.length, examples: dropped.slice(0, examples) },
			missingGlyphs: [...glyphs.values()],
			output: {
				total: output.length,
				lines: output.slice(0, outputLines).map(entry => `${entry.kind}: ${String(entry.message ?? "").split(/\r?\n/)[0].slice(0, 300)}`),
			},
		},
	};
}

// One-line texts of the summary's diagnostics (stderr after a render, sweep reports)
export function diagnosticLines(summary) {
	const lines = [];
	const more = (shown, total) => (total > shown ? `, +${total - shown} more` : "");
	if (summary.hint) lines.push(`hint: ${summary.hint}`);
	if (summary.culled && summary.culled.count) {
		lines.push(`culled: ${summary.culled.count} (${summary.culled.nodes} nodes; ${summary.culled.examples.join("; ")}${more(summary.culled.examples.length, summary.culled.count)})`);
	}
	if (summary.textLinesDropped && summary.textLinesDropped.count) {
		const { count, examples } = summary.textLinesDropped;
		lines.push(`text lines dropped: ${count} (${examples.join("; ")}${more(examples.length, count)})`);
	}
	for (const glyph of asArray(summary.missingGlyphs)) {
		const what = glyph.char ? `'${glyph.char}' ${glyph.code}` : "glyphs";
		lines.push(`missing glyphs: ${what} in ${glyph.font} (${glyph.labels} label${glyph.labels === 1 ? "" : "s"}: ${glyph.paths.join("; ")}${more(glyph.paths.length, glyph.labels)})`);
	}
	if (summary.output && summary.output.total) {
		lines.push(`script output (${summary.output.total} line${summary.output.total === 1 ? "" : "s"}${summary.output.total > summary.output.lines.length ? `, first ${summary.output.lines.length}` : ""}):`);
		for (const line of summary.output.lines) lines.push(`  ${line}`);
	}
	return lines;
}
