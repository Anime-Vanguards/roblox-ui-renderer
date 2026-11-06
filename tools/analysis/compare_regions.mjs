// Attributes a compare's mismatching pixels (> tolerance) to the deepest matching node rect.
//   node tools/analysis/compare_regions.mjs out/compare/<name> [--tolerance 2] [--top 30]
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";

const dir = process.argv[2];
const option = (name, fallback) => {
	const i = process.argv.indexOf(name);
	return i >= 0 ? Number(process.argv[i + 1]) : fallback;
};
const tolerance = option("--tolerance", 2);
const top = option("--top", 30);
const studio = PNG.sync.read(fs.readFileSync(path.join(dir, "studio.png")));
const ours = PNG.sync.read(fs.readFileSync(path.join(dir, "rur", "render.png")));
const scene = JSON.parse(fs.readFileSync(path.join(dir, "rur", "scene.json"), "utf8"));
const scale = scene.viewport?.scale || 1.5;
const nodes = scene.nodes.filter(n => n.rect && n.visible !== false)
	.map(n => ({ path: n.path, r: n.rect.map(v => v * scale), depth: n.path.split(".").length }))
	.sort((a, b) => b.depth - a.depth);
const width = Math.min(studio.width, ours.width), height = Math.min(studio.height, ours.height);
const counts = new Map();
let total = 0;
for (let y = 0; y < height; y++) {
	for (let x = 0; x < width; x++) {
		const i = (y * studio.width + x) * 4, j = (y * ours.width + x) * 4;
		let worst = 0;
		for (let c = 0; c < 3; c++) worst = Math.max(worst, Math.abs(studio.data[i + c] - ours.data[j + c]));
		if (worst <= tolerance) continue;
		total++;
		// A node's paint can overflow its rect (strokes, text): allow a small margin
		let owner = "(background)";
		for (const margin of [0, 6]) {
			const hit = nodes.find(n => x + 0.5 >= n.r[0] - margin && x + 0.5 < n.r[0] + n.r[2] + margin && y + 0.5 >= n.r[1] - margin && y + 0.5 < n.r[1] + n.r[3] + margin);
			if (hit) { owner = hit.path.split(".").slice(-2).join("."); break; }
		}
		counts.set(owner, (counts.get(owner) || 0) + 1);
	}
}
console.log(`${total} pixels above ${tolerance} of ${width * height}`);
for (const [owner, count] of [...counts].sort((a, b) => b[1] - a[1]).slice(0, top)) console.log(String(count).padStart(8), owner);
