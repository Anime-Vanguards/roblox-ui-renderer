// Lists the nodes whose export differs between two offline scenes of one story (e.g. rur/ = Studio's
// replayed clock and rur-ideal/ = the ideal clock of an `anim_timing.mjs` / `rur compare --ideal` run),
// i.e. what the frame clock changes, with the Studio dump's values for the same paths when a compare
// directory is given.
//
//   node tools/analysis/anim_scenediff.mjs <out/compare/timing/Story> [--limit 40]
//   node tools/analysis/anim_scenediff.mjs <sceneA.json> <sceneB.json>

import fs from "node:fs";
import path from "node:path";

function parseArgs(argv) {
	const args = { _: [] };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg.startsWith("--")) {
			args[arg.slice(2)] = argv[i + 1];
			i++;
		} else {
			args._.push(arg);
		}
	}
	return args;
}

// Relative path below StoryTarget plus the occurrence index of that path
function keyed(scene) {
	const result = new Map();
	const target = scene.nodes.find(node => node.name === "StoryTarget");
	const prefix = target ? target.path + "." : "";
	const seen = new Map();
	for (const node of scene.nodes) {
		if (!node.path.startsWith(prefix)) {
			continue;
		}
		const relative = "/" + node.path.slice(prefix.length).split(".").join("/");
		const count = seen.get(relative) || 0;
		seen.set(relative, count + 1);
		result.set(`${relative}#${count}`, node);
	}
	return result;
}

// The fields of two node exports that differ (identity fields ignored)
function changedFields(a, b) {
	const skip = new Set(["id", "parent", "order"]);
	const fields = [];
	for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
		if (skip.has(key)) {
			continue;
		}
		if (JSON.stringify(a[key]) !== JSON.stringify(b[key])) {
			fields.push(key);
		}
	}
	return fields;
}

const brief = value => {
	const text = JSON.stringify(value, (key, item) => (typeof item === "number" ? +item.toFixed(4) : item));
	return text && text.length > 160 ? text.slice(0, 157) + "..." : text;
};

const args = parseArgs(process.argv.slice(2));
let fileA, fileB, studioRows = null;
if (args._.length >= 2) {
	[fileA, fileB] = args._;
} else {
	const dir = path.resolve(args._[0]);
	fileA = path.join(dir, "rur", "scene.json");
	fileB = path.join(dir, "rur-ideal", "scene.json");
	const layoutFile = path.join(dir, "layout-full.json");
	if (fs.existsSync(layoutFile)) {
		studioRows = new Map();
		for (const row of JSON.parse(fs.readFileSync(layoutFile, "utf8")).rows) {
			if (!studioRows.has(row.path)) {
				studioRows.set(row.path, row);
			}
		}
	}
}
const a = keyed(JSON.parse(fs.readFileSync(fileA, "utf8")));
const b = keyed(JSON.parse(fs.readFileSync(fileB, "utf8")));
const limit = Number(args.limit || 40);
let count = 0;
for (const [key, nodeA] of a) {
	const nodeB = b.get(key);
	if (!nodeB) {
		console.log(`${key}: only in ${path.basename(path.dirname(fileA))}`);
		count++;
		continue;
	}
	const fields = changedFields(nodeA, nodeB);
	if (!fields.length) {
		continue;
	}
	count++;
	if (count > limit) {
		continue;
	}
	console.log(`${key} (${nodeA.class})`);
	for (const field of fields) {
		console.log(`    ${field}: ${brief(nodeA[field])}  |  ${brief(nodeB[field])}`);
	}
	const row = studioRows && studioRows.get(key.replace(/#\d+$/, ""));
	if (row && row.studio) {
		console.log(`    studio rect ${brief(row.studio)}${row.props ? ` props ${brief(row.props)}` : ""}`);
	}
}
for (const key of b.keys()) {
	if (!a.has(key)) {
		console.log(`${key}: only in ${path.basename(path.dirname(fileB))}`);
		count++;
	}
}
console.log(`${count} nodes differ`);
