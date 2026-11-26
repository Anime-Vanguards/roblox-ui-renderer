// Where a text shadow's glow lies, glyph by glyph: reads a `rur compare` directory of a probe whose clear labels hold
// rows of stems under a red Mode = Text UIShadow on black (fixtures/shadow-probe-glow-pens.luau), and prints, for each
// label, the x of every crossing of the glow's 50 % level along the stems' middle row and the y of the first stem's top
// and bottom, in the capture and in the renderer's image, with their differences (capture - renderer).
//
//   node tools/analysis/glow_edges.mjs <compare dir> [--level 127.5]

import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";

const args = process.argv.slice(2);
const dir = args.find(arg => !arg.startsWith("--"));
if (!dir) {
	console.error("usage: node tools/analysis/glow_edges.mjs <compare dir> [--level 127.5]");
	process.exit(64);
}
const levelAt = args.indexOf("--level");
const level = levelAt >= 0 ? Number(args[levelAt + 1]) : 127.5;
const read = file => PNG.sync.read(fs.readFileSync(file));
const capture = read(path.join(dir, "studio.png"));
const ours = read(path.join(dir, "rur", "render.png"));
const dpi = JSON.parse(fs.readFileSync(path.join(dir, "report.json"), "utf8")).capture.dpi;
const labels = JSON.parse(fs.readFileSync(path.join(dir, "studio-nodes.json"), "utf8")).nodes.filter(node => node.class === "TextLabel");
const red = (image, x, y) => image.data[(y * image.width + x) * 4];

// Every crossing of the level between t and t + 1, linearly interpolated
function crossings(get, from, to) {
	const out = [];
	for (let t = from; t < to; t++) {
		const a = get(t);
		const b = get(t + 1);
		if ((a - level) * (b - level) < 0) out.push(t + 0.5 + (level - a) / (b - a));
	}
	return out;
}

function edges(image, x0, y0, x1, y1) {
	let top = Infinity;
	let bottom = -Infinity;
	for (let y = y0; y < y1; y++) {
		for (let x = x0; x < x1; x++) {
			if (red(image, x, y) > level) {
				top = Math.min(top, y);
				bottom = Math.max(bottom, y);
			}
		}
	}
	if (top > bottom) return { xs: [], ys: [] };
	const middle = Math.round((top + bottom) / 2);
	const xs = crossings(t => red(image, t, middle), x0, x1);
	const column = xs.length >= 2 ? Math.round((xs[0] + xs[1]) / 2 - 0.5) : x0;
	return { xs, ys: crossings(t => red(image, column, t), y0, y1) };
}

const fixed = values => values.map(value => value.toFixed(2)).join(" ");
const differences = (a, b) => (a.length === b.length ? fixed(a.map((value, i) => value - b[i])) : `${a.length} crossings vs ${b.length}`);
const pad = 6;
for (const label of labels) {
	const box = [Math.floor(label.x * dpi) - pad, Math.floor(label.y * dpi) - pad, Math.ceil((label.x + label.w) * dpi) + pad, Math.ceil((label.y + label.h) * dpi) + pad];
	const theirs = edges(capture, ...box);
	const mine = edges(ours, ...box);
	console.log(`${label.x},${label.y} ${label.w}x${label.h} ${JSON.stringify(label.text)}`);
	console.log(`  x capture  ${fixed(theirs.xs)}`);
	console.log(`  x renderer ${fixed(mine.xs)}`);
	console.log(`  dx ${differences(theirs.xs, mine.xs)}`);
	console.log(`  y capture ${fixed(theirs.ys)} renderer ${fixed(mine.ys)} dy ${differences(theirs.ys, mine.ys)}`);
}
