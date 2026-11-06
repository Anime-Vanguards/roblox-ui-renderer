// Zoomed Studio | rur | difference crops of a `rur compare` output, for checking an attribution by eye.
//
//   node tools/sweep/crop.mjs <out/compare/name> <node> [--zoom 4] [--pad 6] [--out file.png]
//
// <node>: a node key from attribution.json ("/Stack/Flex/Button/Body/Content/Label#7"), a path below the
// story target ("/Stack/Text", first occurrence) or a physical-pixel box "x,y,w,h". The difference panel
// shows the largest channel difference (red > 8, yellow 3-8).

import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";

const argv = process.argv.slice(2);
const option = (name, fallback) => {
	const index = argv.indexOf(`--${name}`);
	return index >= 0 ? argv[index + 1] : fallback;
};
const [dir, which] = argv.filter((arg, index) => !arg.startsWith("--") && !(index > 0 && argv[index - 1].startsWith("--")));
if (!dir || !which) {
	process.stderr.write("usage: node tools/sweep/crop.mjs <compare dir> <node key | path | x,y,w,h> [--zoom 4] [--pad 6] [--out file.png]\n");
	process.exit(2);
}
const zoom = Number(option("zoom", 4));
const pad = Number(option("pad", 6));

const studio = PNG.sync.read(fs.readFileSync(path.join(dir, "studio.png")));
const ours = PNG.sync.read(fs.readFileSync(path.join(dir, "rur", "render.png")));
const scene = JSON.parse(fs.readFileSync(path.join(dir, "rur", "scene.json"), "utf8"));
const scale = scene.viewport.scale;

let box;
if (/^\d+,\d+,\d+,\d+$/.test(which)) {
	box = which.split(",").map(Number);
} else {
	const [relative, occurrence] = which.split("#");
	const target = scene.nodes.find(node => node.name === "StoryTarget");
	const prefix = target ? target.path : "";
	const wanted = prefix + relative.split("/").join(".");
	const matches = scene.nodes.filter(node => node.path === wanted);
	const node = matches[Number(occurrence || 0)];
	if (!node) {
		process.stderr.write(`no node ${which} (${matches.length} with that path)\n`);
		process.exit(1);
	}
	const [x, y, w, h] = node.rect;
	box = [Math.floor(x * scale), Math.floor(y * scale), Math.ceil(w * scale), Math.ceil(h * scale)];
}
let [x0, y0, w, h] = box;
x0 = Math.max(0, x0 - pad);
y0 = Math.max(0, y0 - pad);
w = Math.min(studio.width - x0, w + 2 * pad);
h = Math.min(studio.height - y0, h + 2 * pad);

const gap = 4;
const out = new PNG({ width: (w * zoom) * 3 + gap * 2, height: h * zoom });
out.data.fill(255);
const put = (panel, px, py, rgb) => {
	for (let zy = 0; zy < zoom; zy++) {
		for (let zx = 0; zx < zoom; zx++) {
			const k = ((py * zoom + zy) * out.width + panel * (w * zoom + gap) + px * zoom + zx) * 4;
			out.data[k] = rgb[0];
			out.data[k + 1] = rgb[1];
			out.data[k + 2] = rgb[2];
			out.data[k + 3] = 255;
		}
	}
};
let differing = 0;
for (let y = 0; y < h; y++) {
	for (let x = 0; x < w; x++) {
		const ia = ((y0 + y) * studio.width + x0 + x) * 4, ib = ((y0 + y) * ours.width + x0 + x) * 4;
		const a = [studio.data[ia], studio.data[ia + 1], studio.data[ia + 2]];
		const b = [ours.data[ib], ours.data[ib + 1], ours.data[ib + 2]];
		const d = Math.max(Math.abs(a[0] - b[0]), Math.abs(a[1] - b[1]), Math.abs(a[2] - b[2]));
		if (d > 2) {
			differing++;
		}
		put(0, x, y, a);
		put(1, x, y, b);
		put(2, x, y, d > 8 ? [255, 40, 40] : d > 2 ? [255, 210, 0] : a.map(v => Math.round(v * 0.3)));
	}
}
const file = option("out", path.join(dir, `crop-${which.replace(/[^\w.-]+/g, "_").replace(/^_+/, "")}.png`));
fs.writeFileSync(file, PNG.sync.write(out));
console.log(`${file}: box ${x0},${y0} ${w}x${h}, ${differing} px differ`);
