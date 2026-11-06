// How much of each story's Studio difference is timing: `rur compare --ideal` per story, i.e. one Studio
// capture scored against two offline runs, one replaying Studio's recorded frame clock (the compare
// default) and one on the ideal clock. The difference between the two is what the animation / timing
// state at the capture accounts for.
//
//   node tools/analysis/anim_timing.mjs [--filter a|b] --set <parity set.json> [--time 1]
//        [--viewport 1200x900] [--out out/compare/timing] [story files...]
//
// Writes <out>/<name>/ (the compare outputs, rur/ = replayed clock, rur-ideal/ = ideal clock) and
// <out>/timing.md + timing.json.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { compare } from "../../cli/compare.mjs";
import { placeOf, placeRoots } from "../../cli/places.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const PROJECT = path.resolve(process.env.RUR_PROJECT || path.join(ROOT, "examples", "project"));

function parseArgs(argv) {
	const args = { _: [] };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg.startsWith("--")) {
			const next = argv[i + 1];
			if (next === undefined || next.startsWith("--")) {
				args[arg.slice(2)] = true;
			} else {
				args[arg.slice(2)] = next;
				i++;
			}
		} else {
			args._.push(arg);
		}
	}
	return args;
}

// The roots a story's place mounts
function rootsFor(file) {
	return placeRoots(PROJECT, placeOf(PROJECT, file)).join(";");
}

function storyList(args) {
	if (args._.length) {
		return args._.map(file => path.resolve(file));
	}
	if (!args.set || args.set === true) throw new Error("name the stories, or a parity set with --set <file.json> ({ stories: [{ story: <path in the project> }] })");
	const set = JSON.parse(fs.readFileSync(path.resolve(String(args.set)), "utf8"));
	const filters = args.filter && args.filter !== true ? String(args.filter).toLowerCase().split("|").filter(Boolean) : [];
	return set.stories
		.map(entry => path.join(PROJECT, entry.story))
		.filter(file => !filters.length || filters.some(filter => file.toLowerCase().includes(filter)));
}

const pct = value => (typeof value === "number" ? value.toFixed(3) : "-");

async function main() {
	const args = parseArgs(process.argv.slice(2));
	const outRoot = path.resolve(String(args.out || path.join(ROOT, "out", "compare", "timing")));
	fs.mkdirSync(outRoot, { recursive: true });
	const time = Number(args.time || 1);
	const viewport = String(args.viewport || "1200x900");

	const results = [];
	for (const file of storyList(args)) {
		const name = path.basename(file).replace(/\.story\.luau?$/, "");
		const out = path.join(outRoot, name);
		let entry = { name, story: file };
		try {
			const report = await compare({ story: file, rootsResolved: rootsFor(file), viewport, time: String(time), ideal: true, out });
			const ideal = report.clock.ideal || {};
			entry = {
				...entry,
				studio: report.studio.status,
				rur: report.rur.status,
				frozen: report.clock.frozen,
				mountMs: report.clock.mountMs,
				waited: report.clock.waited,
				nodes: report.layout.studioNodes,
				replay: { exact: report.pixels.exactPercent, layout: report.layout.exact, matched: report.layout.matched, props: report.layout.propMismatches },
				ideal: ideal.pixels ? { exact: ideal.pixels.exactPercent, layout: ideal.layout.exact, matched: ideal.layout.matched, props: ideal.layout.propMismatches } : null,
			};
		} catch (error) {
			entry.error = String(error && error.message ? error.message : error).slice(0, 300);
		}
		results.push(entry);
		const line = entry.error
			? `${name}: error ${entry.error}`
			: `${name}: exact ${pct(entry.ideal && entry.ideal.exact)} -> ${pct(entry.replay.exact)}, layout ${entry.ideal ? entry.ideal.layout : "-"} -> ${entry.replay.layout} / ${entry.nodes}, props ${entry.ideal ? entry.ideal.props : "-"} -> ${entry.replay.props}`;
		process.stdout.write(line + "\n");
	}

	fs.writeFileSync(path.join(outRoot, "timing.json"), JSON.stringify(results, null, 1));
	const rows = results.map(entry => entry.error
		? `| ${entry.name} | error: ${entry.error.replace(/\|/g, "/")} | | | | | | |`
		: `| ${entry.name} | ${pct(entry.ideal && entry.ideal.exact)} | ${pct(entry.replay.exact)} | ${entry.ideal ? entry.ideal.layout : "-"} | ${entry.replay.layout} / ${entry.nodes} | ${entry.ideal ? entry.ideal.props : "-"} | ${entry.replay.props} | ${entry.frozen ? "yes" : "no"} |`);
	const table = [
		`# Timing share of the Studio difference (${new Date().toISOString().slice(0, 16).replace("T", " ")})`,
		"",
		`\`node tools/analysis/anim_timing.mjs\`: one Studio capture per story (viewport ${viewport}, time ${time} s) against the offline run on the ideal clock and on Studio's replayed clock.`,
		"",
		"| story | exact % ideal | exact % replay | layout exact ideal | layout exact replay | props ideal | props replay | frozen |",
		"|---|---|---|---|---|---|---|---|",
		...rows,
	].join("\n");
	fs.writeFileSync(path.join(outRoot, "timing.md"), table + "\n");
	process.stdout.write(`-> ${path.relative(ROOT, path.join(outRoot, "timing.md"))}\n`);
}

await main();
