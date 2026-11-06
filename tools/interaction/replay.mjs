// Event-sequence parity: runs one scenario (recorder setup + timed posted mouse input + Luau steps) in
// Studio and offline in rur (the same recorder, the input as an --actions timeline), then compares the
// event logs step by step.
//
//   node tools/interaction/replay.mjs tools/interaction/scenarios/<name>.mjs [--offline-only]
//
// A scenario module exports { name, viewport?: [w, h], setup: "<Luau using R>", duration, steps: [
//   { at, note }, { at, post: [act.move(x, y) | act.down(x, y) | act.up(x, y) | ...] }, { at, luau: "<Luau>" } ] }.
// Posted items of one step go out back to back (one Qt burst in Studio, consecutive events offline).
// Studio-only artifacts are dropped before comparing: GuiState polls, MouseMovement events with the
// real cursor's position (outside the viewport), and events of the recorder's own cover.
// Output: out/interaction/replay/<name>/{studio.json, ours.json, report.txt}

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { engineBaseArgs, findEngine } from "../../cli/engine.mjs";
import { withStudioInput, act, DPI } from "./studio.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..", "..");
const MARKER = "__RURREPLAY__";

const file = process.argv[2];
if (!file) {
	process.stderr.write("usage: node tools/interaction/replay.mjs <scenario.mjs> [--offline-only]\n");
	process.exit(2);
}
const offlineOnly = process.argv.includes("--offline-only");
const scenario = (await import(pathToFileURL(path.resolve(file)).href)).default;
const outDir = path.join(ROOT, "out", "interaction", "replay", scenario.name);
fs.mkdirSync(outDir, { recursive: true });
const [width, height] = scenario.viewport || [1200, 800];
const recorder = fs.readFileSync(path.join(HERE, "recorder.luau"), "utf8").replace(/\nreturn Recorder\s*$/, "\n");

// Studio run: steps in order, sleeping to each step's time
let origin = [5, 252];

// A step's post: a list of items (one burst) or of bursts (arrays) and sleeps (numbers, ms) posted in
// one call, so short gaps (double clicks) keep their timing in Studio.
function bursts(post) {
	return Array.isArray(post[0]) || typeof post[0] === "number" ? post : [post];
}

async function runStudio() {
	const result = await withStudioInput(`replay ${scenario.name}`, scenario.setup, async api => {
		// The viewport's screen origin (physical px): Qt maps posted client positions through it
		const match = api.post([]).match(/origin=(-?\d+),(-?\d+)/);
		if (match) origin = [Number(match[1]), Number(match[2])];
		const started = Date.now();
		for (const step of scenario.steps) {
			const wait = step.at * 1000 - (Date.now() - started);
			if (wait > 0) await new Promise(resolve => setTimeout(resolve, wait));
			if (step.note) await api.luau(`R.Note(${JSON.stringify(step.note)}) return ""`);
			if (step.post) api.post([...bursts(step.post).flatMap(part => (typeof part === "number" ? [act.sleep(part)] : [act.batch(...part)])), act.sleep(30)]);
			if (step.luau) await api.luau(`${step.luau}\nreturn ""`);
		}
		const remaining = scenario.duration * 1000 - (Date.now() - started);
		if (remaining > 0) await new Promise(resolve => setTimeout(resolve, remaining));
		return "";
	});
	fs.writeFileSync(path.join(outDir, "studio.json"), JSON.stringify(result.log));
	fs.writeFileSync(path.join(outDir, "studio-origin.json"), JSON.stringify(origin));
	if (result.safety.changed.length) process.stderr.write(`studio state changed: ${result.safety.changed.join(", ")}\n`);
	return result.log;
}

// Offline run: the recorder + setup as a fixture, posted items as an --actions timeline (logical px)
function runOffline() {
	const actions = [];
	const delayed = [];
	let last = null;
	for (const step of scenario.steps) {
		if (step.note) delayed.push(`task.delay(${step.at}, function() R.Note(${JSON.stringify(step.note)}) end)`);
		if (step.luau) delayed.push(`task.delay(${step.at + 0.0005}, function()\n${step.luau}\nend)`);
		if (step.post) {
			let offset = 0;
			const items = [];
			for (const part of bursts(step.post)) {
				if (typeof part === "number") {
					offset += part / 1000;
					continue;
				}
				part.forEach((item, index) => items.push({ item, t: step.at + offset + 0.001 + index * 0.0001 }));
				offset += 0.001 + part.length * 0.0001;
			}
			items.forEach(({ item, t }) => {
				// Studio's logical position of a posted client position (Qt: global native / DPI, rounded,
				// minus the widget's rounded logical origin)
				const x = Math.round((origin[0] + item.x) / DPI) - Math.round(origin[0] / DPI);
				const y = Math.round((origin[1] + item.y) / DPI) - Math.round(origin[1] / DPI);
				// Qt drops a posted move to the position the mouse already has
				if (item.op === "move" && last && last[0] === x && last[1] === y) return;
				last = [x, y];
				if (item.op === "move") actions.push({ t, type: "move", x, y });
				else if (item.op === "down") actions.push({ t, type: "down", button: item.button || 1, x, y });
				else if (item.op === "up") actions.push({ t, type: "up", button: item.button || 1, x, y });
			});
			// Studio: after a burst with a button press, Qt's leave (the real cursor is elsewhere) makes the
			// engine end the mouse movement (MouseLeave on hovered objects); the next move begins again
			if (items.some(({ item }) => item.op === "down" || item.op === "up")) {
				actions.push({ t: items[items.length - 1].t + 0.0001, type: "leave" });
				last = null;
			}
		}
	}
	const fixture = path.join(outDir, "offline-fixture.luau");
	fs.writeFileSync(fixture, `return function(Target)
${recorder}
local R = Recorder
local UIS = game:GetService("UserInputService")
local HttpService = game:GetService("HttpService")
${scenario.setup}
R.Gui.Parent = game:GetService("CoreGui")
${delayed.join("\n")}
task.delay(${scenario.duration}, function()
	print("${MARKER}" .. HttpService:JSONEncode(R.Log))
end)
end
`);
	const actionsFile = path.join(outDir, "actions.json");
	fs.writeFileSync(actionsFile, JSON.stringify(actions));
	const scene = path.join(outDir, "scene.json");
	const run = spawnSync(findEngine() || (() => { throw new Error("no engine build (node tools/native-app/build.mjs)"); })(), [...engineBaseArgs(), "--roots", path.join(ROOT, "tools", "tests", "oracle-root"), "--no-snapshots", "--out", scene, "--fixture", fixture, "--viewport", `${width}x${height}`, "--scale", String(DPI), "--time", String(scenario.duration + 0.1), "--actions", actionsFile], { cwd: ROOT, encoding: "utf8", maxBuffer: 1 << 26 });
	if (!fs.existsSync(scene)) throw new Error(`offline run failed: ${run.stderr}`);
	const output = JSON.parse(fs.readFileSync(scene, "utf8")).diagnostics.output || [];
	const line = output.find(entry => (entry.message || "").startsWith(MARKER));
	if (!line) throw new Error(`no log from the offline run: ${output.filter(e => e.kind === "error").map(e => e.message).join("\n")}`);
	const log = JSON.parse(line.message.slice(MARKER.length));
	fs.writeFileSync(path.join(outDir, "ours.json"), JSON.stringify(log));
	return log;
}

// Normalized event lines per note section. The window-leave (Studio: Qt's leave after a burst with a
// button, the real cursor being elsewhere; offline: the simulated leave) is dropped: MouseMovement End
// events and the MouseLeave right before each of them; the MouseMovement state (Begin / Change) and
// UserInputService's InputBegan vs InputChanged for movement are not compared.
function normalize(log) {
	const sections = [];
	let current = { note: "(start)", lines: [] };
	sections.push(current);
	for (const entry of log) {
		if (entry.note) {
			current = { note: entry.note, lines: [] };
			sections.push(current);
			continue;
		}
		// Polled properties listed in scenario.props: the value each object ends the section with
		if (entry.p && entry.o && (scenario.props || []).includes(entry.p)) {
			current.values = current.values || {};
			const value = Array.isArray(entry.v) ? entry.v.map(v => (typeof v === "number" ? Math.round(v * 100) / 100 : v)) : entry.v;
			current.values[`${entry.o}.${entry.p}`] = JSON.stringify(value);
			continue;
		}
		if (!entry.e || entry.o === "Cover") continue;
		const raw = Array.isArray(entry.a) ? entry.a : [];
		const ending = raw.find(arg => arg && typeof arg === "object" && arg.input === "MouseMovement" && arg.state === "End");
		if (ending) {
			const leave = `${entry.o}.MouseLeave(${Math.round(ending.pos[0])}, ${Math.round(ending.pos[1])})`;
			if (entry.o && current.lines[current.lines.length - 1] === leave) current.lines.pop();
			continue;
		}
		const args = raw.map(arg => {
			if (arg && typeof arg === "object" && arg.input) {
				const state = arg.input === "MouseMovement" ? "Move" : arg.state;
				return `${arg.input}:${state}${arg.key !== "None" ? ":" + arg.key : ""}@${Math.round(arg.pos[0])},${Math.round(arg.pos[1])}${arg.pos[2] ? "," + arg.pos[2] : ""}`;
			}
			if (typeof arg === "number") return String(Math.round(arg));
			return JSON.stringify(arg);
		});
		const name = /^UIS\.Input(Began|Changed)$/.test(entry.e) && String(args[0]).startsWith("MouseMovement") ? "UIS.InputMoved" : entry.e;
		current.lines.push(`${entry.o ? entry.o + "." : ""}${name}(${args.join(", ")})`);
	}
	return sections;
}

if (offlineOnly && fs.existsSync(path.join(outDir, "studio-origin.json"))) origin = JSON.parse(fs.readFileSync(path.join(outDir, "studio-origin.json"), "utf8"));
const studioLog = offlineOnly ? JSON.parse(fs.readFileSync(path.join(outDir, "studio.json"), "utf8")) : await runStudio();
const oursLog = runOffline();
const studio = normalize(studioLog);
const ours = normalize(oursLog);
const report = [];
let equal = 0;
// Property values carry over between sections (a value logged earlier still holds)
for (const list of [studio, ours]) {
	let carried = {};
	for (const section of list) {
		carried = { ...carried, ...(section.values || {}) };
		section.state = Object.keys(carried).sort().map(key => `${key} = ${carried[key]}`);
	}
}
for (const section of studio) {
	const other = ours.find(candidate => candidate.note === section.note);
	const a = [...section.lines, ...section.state].join("\n");
	const b = other ? [...other.lines, ...other.state].join("\n") : "(missing)";
	if (a === b) {
		equal++;
		report.push(`== ${section.note}: same (${section.lines.length} events${section.state.length ? `, ${section.state.length} values` : ""})`);
	} else {
		report.push(`!! ${section.note}\n  studio:\n    ${[...section.lines, ...section.state].join("\n    ") || "(none)"}\n  ours:\n    ${other ? [...other.lines, ...other.state].join("\n    ") || "(none)" : "(missing)"}`);
	}
}
report.unshift(`${scenario.name}: ${equal}/${studio.length} steps identical`);
fs.writeFileSync(path.join(outDir, "report.txt"), report.join("\n"));
console.log(report.join("\n"));
