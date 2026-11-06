// Compares the numbered PNG frames of two `--save-frames` captures pixel by pixel.
//   node tools/analysis/frame_diff.mjs <dirA> <dirB>
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";

const [a, b] = process.argv.slice(2).map(dir => path.join(dir, "frames"));
const names = fs.readdirSync(a).filter(name => name.endsWith(".png")).sort();
let identical = 0, maxDiff = 0, differing = [];
for (const name of names) {
	const left = PNG.sync.read(fs.readFileSync(path.join(a, name)));
	const right = PNG.sync.read(fs.readFileSync(path.join(b, name)));
	let count = 0, worst = 0;
	for (let i = 0; i < left.data.length; i++) {
		const d = Math.abs(left.data[i] - right.data[i]);
		if (d) { count++; worst = Math.max(worst, d); }
	}
	if (count === 0) identical++;
	else differing.push(`${name}: ${count} channels, max ${worst}`);
	maxDiff = Math.max(maxDiff, worst);
}
console.log(`${identical}/${names.length} frames identical; max channel difference ${maxDiff}`);
for (const line of differing.slice(0, 20)) console.log(line);
