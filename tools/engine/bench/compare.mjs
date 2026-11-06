#!/usr/bin/env node
// Runs tools/engine/bench/api.luau (or another bench fixture) through the engine several times and
// prints each row's best ns/op (the machine is often busy: the minimum over runs is the least
// disturbed measurement). With --before, another engine build runs alternately in its own column.
//
//   node tools/engine/bench/compare.mjs [--runs 5] [--fixture tools/engine/bench/api.luau]
//        [--before <another rur-native build>] [--engine <rur-native build>] [--json out/engine-bench.json]
// --engine defaults to the current build (cli/engine.mjs); --before is e.g. the last commit's,
// built in a worktree. (The Luau runtime column of the rewrite's log is gone with the runtime.)

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { engineBaseArgs, ensureEngine } from "../../../cli/engine.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
	const index = args.indexOf(`--${name}`);
	return index >= 0 ? args[index + 1] : fallback;
};
const runs = Number(option("runs", 5));
const fixture = option("fixture", "tools/engine/bench/api.luau");
const before = option("before");
const current = option("engine") ? path.resolve(option("engine")) : await ensureEngine();
const out = path.join(ROOT, "out", "engine-bench");
fs.mkdirSync(out, { recursive: true });
const env = { ...process.env, RUR_REAL_CLOCK: "1" };

function rows(lines) {
	const result = new Map();
	for (const line of lines) {
		const match = /^(.*?)\s+(-?[\d.]+) ns\/op$/.exec(line);
		if (match) result.set(match[1].trim(), Number(match[2]));
	}
	return result;
}

function run(exe, name) {
	const json = path.join(out, `${name}.json`);
	spawnSync(path.resolve(exe), [...engineBaseArgs(), "--roots", path.join(ROOT, "tools", "tests", "oracle-root"), "--no-snapshots", "--fixture", fixture, "--out", json], { cwd: ROOT, env, stdio: "ignore" });
	const scene = JSON.parse(fs.readFileSync(json, "utf8"));
	return rows(scene.diagnostics.output.map(entry => entry.message));
}

const best = { before: new Map(), engine: new Map() };
const order = [];
const keep = (target, measured) => {
	for (const [label, value] of measured) {
		if (!order.includes(label)) order.push(label);
		if (!target.has(label) || value < target.get(label)) target.set(label, value);
	}
};
for (let index = 0; index < runs; index++) {
	if (before) keep(best.before, run(before, "before"));
	keep(best.engine, run(current, "engine"));
	process.stderr.write(`run ${index + 1}/${runs}\n`);
}
const pad = (text, width) => String(text).padEnd(width);
console.log(`${pad("row", 36)} ${before ? `${pad("before", 10)} ` : ""}${pad("engine", 10)}${before ? " before/engine" : ""}`);
for (const label of order) {
	const now = best.engine.get(label);
	const then = best.before.get(label);
	const ratio = before && then !== undefined && now ? ` ${(then / Math.max(now, 0.1)).toFixed(2)}x` : "";
	console.log(`${pad(label, 36)} ${before ? `${pad(then?.toFixed(1) ?? "-", 10)} ` : ""}${pad(now?.toFixed(1) ?? "-", 10)}${ratio}`);
}
const json = option("json");
if (json) {
	fs.writeFileSync(path.resolve(ROOT, json), JSON.stringify({ before: Object.fromEntries(best.before), engine: Object.fromEntries(best.engine) }, null, 2));
}
