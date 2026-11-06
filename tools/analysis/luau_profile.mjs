// Summarizes a folded Luau profile written with RUR_LUAU_PROFILE=<file>.
//   node tools/analysis/luau_profile.mjs <file.folded> [--root frames|startup] [--top 40] [--lines] [--focus text]
import fs from "node:fs";

const args = process.argv.slice(2);
const file = args[0];
const option = (name, fallback) => {
	const index = args.indexOf(name);
	return index >= 0 ? args[index + 1] : fallback;
};
const root = option("--root", "frames");
const top = Number(option("--top", 40));
const focus = option("--focus", null);
const lines = args.includes("--lines");

const self = new Map();
const inclusive = new Map();
const leafLines = new Map();
let total = 0;
for (const row of fs.readFileSync(file, "utf8").split("\n")) {
	const split = row.lastIndexOf(" ");
	if (split < 0) continue;
	const count = Number(row.slice(split + 1));
	const frames = row.slice(0, split).replace(/[A-Z]:[^;]*?[\\/]roblox-ui-renderer[\\/]/g, "").replace(/\[string "([^"]*)"\]/g, "$1").split(";");
	if (frames[0] !== root) continue;
	if (focus && !frames.some(frame => frame.includes(focus))) continue;
	total += count;
	const leaf = frames[frames.length - 1];
	const at = leaf.lastIndexOf(" @");
	const leafFunction = at >= 0 ? leaf.slice(0, at) : leaf;
	self.set(leafFunction, (self.get(leafFunction) || 0) + count);
	if (at >= 0) leafLines.set(leaf, (leafLines.get(leaf) || 0) + count);
	const seen = new Set();
	for (let index = 1; index < frames.length; index++) {
		let frame = frames[index];
		if (index === frames.length - 1) frame = leafFunction;
		if (seen.has(frame)) continue;
		seen.add(frame);
		inclusive.set(frame, (inclusive.get(frame) || 0) + count);
	}
}
const print = (title, map) => {
	console.log(`\n== ${title} (${root}, ${total} ms sampled${focus ? `, focus ${focus}` : ""})`);
	for (const [name, count] of [...map].sort((a, b) => b[1] - a[1]).slice(0, top)) {
		console.log(`${(100 * count / total).toFixed(1).padStart(5)}% ${String(count).padStart(7)}  ${name}`);
	}
};
print("self", self);
print("inclusive", inclusive);
if (lines) print("leaf lines", leafLines);
