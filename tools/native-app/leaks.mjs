#!/usr/bin/env node
// What a story keeps alive of the UI it removed: runs the story in an engine session, optionally
// flips a boolean control back and forth (each flip makes a story that switches between two UIs
// rebuild one), and prints the engine's leak report (`{"cmd":"leaks"}`, the emulator's src/lua/leaks.rs):
// the trees outside the DataModel that are still held, and per connected handler in them (script
// and line, signal, class of the instance) how many trees it holds and what it captures.
//
// Roblox does not free an instance whose connected handler references it (measured in Studio: such
// an instance, parented to nil with every other reference dropped, survives garbage collection;
// Destroy or Disconnect releases it), and the engine does the same. UI code that removes instances
// without destroying them (Vide's `show` / `switch` only unparent what they replace) therefore leaks
// every tree one of whose still-connected handlers reaches it, in the game as here. The fix is in
// the UI code: Destroy what a cleanup removes, or disconnect the listed handlers.
//
//   node tools/native-app/leaks.mjs --story <file.story.luau> [--control <Group.Name>] [--flips 10]
//        [--frames 9] [--place <name>] [--roots "a;b"] [--viewport 1200x900] [--json] [--top 40]
//
// --control names a boolean control (dot-separated path) to flip `--flips` times, `--frames` frames
// (1/60 s) apart; without it the story only runs one second. The first lines show the engine's
// instances, Luau heap and process memory at the start and the end, and their growth per flip.
import { spawn } from "node:child_process";
import path from "node:path";
import readline from "node:readline";
import { ROOT } from "../../cli/paths.mjs";
import { engineBaseArgs, ensureEngine } from "../../cli/engine.mjs";
import { placeOf, placeRoots } from "../../cli/places.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
	const index = args.indexOf(name);
	return index >= 0 ? args[index + 1] : fallback;
};
const project = process.env.RUR_PROJECT ? path.resolve(process.env.RUR_PROJECT) : path.join(ROOT, "examples", "project");
if (!option("--story")) {
	console.error("usage: node tools/native-app/leaks.mjs --story <file.story.luau> [--control <Group.Name>] [--flips 10] [--frames 9] [--json]");
	process.exit(64);
}
const story = path.resolve(option("--story"));
// --roots, else the roots of --place, else of the project's place that holds the story (cli/places.mjs)
const roots = option("--roots", placeRoots(project, option("--place") || placeOf(project, story)).join(";"));
const control = option("--control", null)?.split(".");
const flips = control ? Number(option("--flips", 10)) : 0;
const frames = Number(option("--frames", 9));
const top = Number(option("--top", 40));

const engine = await ensureEngine();
const child = spawn(engine, [...engineBaseArgs(), "--story", story, "--roots", roots, "--viewport", option("--viewport", "1200x900"), "--scale", "1.5", "--time", "1", "--session", "1"], { stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
let stderr = "";
child.stderr.on("data", chunk => (stderr += chunk));
const waiters = [];
readline.createInterface({ input: child.stdout }).on("line", line => {
	if (line.startsWith("@@RUR ")) waiters.shift()?.(JSON.parse(line.slice(6)));
});
child.on("exit", code => {
	for (const waiter of waiters.splice(0)) waiter({ type: "error", message: `the engine exited (${code}): ${stderr.slice(-2000)}` });
});
const reply = () => new Promise(resolve => waiters.push(resolve));
const send = command => {
	const answer = reply();
	child.stdin.write(JSON.stringify(command) + "\n");
	return answer;
};
const check = message => {
	if (message.type === "error") {
		console.error(message.message);
		process.exit(1);
	}
	return message;
};

check(await reply());
const frame = () => send({ cmd: "frame", dt: 1 / 60, events: [] });
let last;
for (let i = 0; i < 30; i++) last = check(await frame());
const gauge = message => ({
	instances: message.timings?.instances,
	luauHeapMB: (message.timings?.luauHeapBytes ?? NaN) / 1048576,
	processMB: (message.timings?.processBytes ?? NaN) / 1048576,
});
const start = gauge(last);
for (let flip = 1; flip <= flips; flip++) {
	check(await send({ cmd: "control", path: control, value: flip % 2 === 1 }));
	for (let i = 0; i < frames; i++) last = check(await frame());
}
const end = gauge(last);
const leaks = check(await send({ cmd: "leaks" }));
child.stdin.end(JSON.stringify({ cmd: "quit" }) + "\n");

if (args.includes("--json")) {
	console.log(JSON.stringify({ story, flips, start, end, leaks }, null, 1));
} else {
	const number = value => (Number.isFinite(value) ? (Number.isInteger(value) ? String(value) : value.toFixed(1)) : "-");
	for (const [key, unit] of [["instances", ""], ["luauHeapMB", " MB"], ["processMB", " MB"]]) {
		const perFlip = flips ? `  (${number((end[key] - start[key]) / flips)}${unit} per flip)` : "";
		console.log(`${key.padEnd(12)} ${number(start[key]).padStart(9)} -> ${number(end[key]).padStart(9)}${unit}${perFlip}`);
	}
	const outside = leaks.outsideDataModel;
	console.log(`\noutside the DataModel: ${outside.trees} trees, ${outside.instances} instances; ${outside.heldByConnectedHandlers} of them in trees a connected handler captures`);
	for (const tree of leaks.largestTrees.filter(tree => tree.instances > 1).slice(0, 8)) {
		console.log(`  ${String(tree.instances).padStart(6)} instances  ${tree.root}${tree.heldByHandler ? "  (held by its handlers)" : ""}`);
	}
	if (leaks.handlers.length) {
		console.log(`\nconnected handlers that capture a removed tree (${leaks.handlers.length}; trees held / connections):`);
		for (const handler of leaks.handlers.slice(0, top)) {
			console.log(`  ${String(handler.trees).padStart(4)} / ${String(handler.connections).padStart(5)}  ${handler.handler}  ${handler.class}.${handler.signal}`);
			console.log(`${" ".repeat(16)}captures ${handler.captures}`);
			console.log(`${" ".repeat(16)}e.g. ${handler.example}`);
		}
		if (leaks.handlers.length > top) console.log(`  ... ${leaks.handlers.length - top} more (--top)`);
	} else {
		console.log("\nno connected handler captures a removed tree");
	}
}
