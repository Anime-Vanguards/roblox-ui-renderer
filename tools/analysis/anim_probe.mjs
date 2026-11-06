// Runs an animation/timing probe in Roblox Studio (edit mode) and offline in the rur engine.
//
//   node tools/analysis/anim_probe.mjs tools/studio/anim-probe-frames.luau [--studio-only]
//        [--offline-only] [--runs N] [--time 4] [--studio <name>] [--project] [--clock <file.json>]
//
// A probe is a Luau chunk returning `function(Target) ... return Results end`. It may yield (it
// usually records RunService events for a second or two) and returns a JSON-able table. In Studio it
// runs under a temporary ScreenGui in CoreGui (destroyed afterwards) while holding the Studio lock;
// offline it runs as a fixture (`--time` virtual seconds). Results go to
// out/probe/<name>/studio[-k].json and ours.json; the per-probe analysis scripts
// (tools/analysis/anim_*.py) compare them. --project mounts the project offline (RUR_PROJECT's default
// place, cli/places.mjs) so a probe can require its modules (springs); --clock replays a recorded Studio
// frame clock offline (RUR_CLOCK, see tools/analysis/anim_clock.py).

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { runLuau, DEFAULT_STUDIO } from "../../cli/studio.mjs";
import { withStudioLock } from "../../cli/lock.mjs";
import { engineBaseArgs, findEngine } from "../../cli/engine.mjs";
import { placeRoots } from "../../cli/places.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const MARKER = "__RURANIM__";

const STUDIO_WRAPPER = source => `
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")
local Old = CoreGui:FindFirstChild("__RUR_AnimProbe")
if Old then
	Old:Destroy()
end
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_AnimProbe"
Gui.IgnoreGuiInset = true
Gui.DisplayOrder = 2000000000
Gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
local Target = Instance.new("Frame")
Target.Name = "StoryTarget"
Target.Size = UDim2.fromOffset(400, 300)
Target.BackgroundTransparency = 1
Target.BorderSizePixel = 0
Target.Parent = Gui
Gui.Parent = CoreGui
local Probe = (function()
${source}
end)()
local Ok, Result = pcall(Probe, Target)
Gui:Destroy()
if not Ok then
	return HttpService:JSONEncode({ __error = tostring(Result) })
end
return HttpService:JSONEncode(Result)
`;

const OFFLINE_WRAPPER = source => `
return function(Target)
	local Probe = (function()
${source}
	end)()
	task.spawn(function()
		local Ok, Result = pcall(Probe, Target)
		local HttpService = game:GetService("HttpService")
		print("${MARKER}" .. HttpService:JSONEncode(if Ok then Result else { __error = tostring(Result) }))
	end)
end
`;

function parseArgs(argv) {
	const args = { _: [] };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg.startsWith("--")) {
			const next = argv[i + 1];
			if (next === undefined || next.startsWith("--")) args[arg.slice(2)] = true;
			else {
				args[arg.slice(2)] = next;
				i++;
			}
		} else args._.push(arg);
	}
	return args;
}

export async function runStudio(source, { studio = DEFAULT_STUDIO, owner = "anim-probe" } = {}) {
	const { text } = await withStudioLock(owner, () => runLuau(STUDIO_WRAPPER(source), { studio }));
	return JSON.parse(text);
}

const PROJECT = path.resolve(process.env.RUR_PROJECT || path.join(ROOT, "examples", "project"));

export function runOffline(source, outDir, time, { project = false, clock = null } = {}) {
	fs.mkdirSync(outDir, { recursive: true });
	const fixture = path.join(outDir, "offline-probe.luau");
	fs.writeFileSync(fixture, OFFLINE_WRAPPER(source));
	const scenePath = path.join(outDir, "scene.json");
	const roots = project ? placeRoots(PROJECT).join(";") : path.join(ROOT, "tools", "tests", "oracle-root");
	const args = [...engineBaseArgs(), "--no-snapshots", "--roots", roots, "--out", scenePath, "--fixture", fixture, "--viewport", "400x300", "--scale", "1.5", "--time", String(time)];
	const env = { ...process.env };
	if (clock) env.RUR_CLOCK = path.resolve(clock);
	else delete env.RUR_CLOCK;
	const result = spawnSync(findEngine() || (() => { throw new Error("no engine build (node tools/native-app/build.mjs)"); })(), args, { cwd: ROOT, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, env });
	if (result.status !== 0 || !fs.existsSync(scenePath)) {
		throw new Error(`offline probe failed: ${(result.stdout || "").slice(-3000)} ${(result.stderr || "").slice(-3000)}`);
	}
	const scene = JSON.parse(fs.readFileSync(scenePath, "utf8"));
	const output = (scene.diagnostics && scene.diagnostics.output) || [];
	for (const entry of output) {
		const message = entry.message || "";
		if (message.startsWith(MARKER)) return JSON.parse(message.slice(MARKER.length));
	}
	const errors = output.filter(entry => entry.kind === "error").map(entry => entry.message);
	throw new Error(`offline probe printed no result; errors: ${JSON.stringify(errors.slice(0, 5))}`);
}

async function main() {
	const args = parseArgs(process.argv.slice(2));
	const probe = args._[0];
	if (!probe) {
		process.stderr.write("usage: node tools/analysis/anim_probe.mjs <probe.luau> [--studio-only] [--offline-only] [--runs N] [--time 4] [--project] [--clock <clock.json>]\n");
		process.exit(2);
	}
	const name = path.basename(probe).replace(/\.luau$/, "");
	const outDir = path.join(ROOT, "out", "probe", name);
	fs.mkdirSync(outDir, { recursive: true });
	const source = fs.readFileSync(probe, "utf8");

	if (!args["offline-only"]) {
		const runs = Number(args.runs || 1);
		for (let run = 1; run <= runs; run++) {
			const result = await runStudio(source, { studio: args.studio || DEFAULT_STUDIO, owner: `anim-probe ${name}` });
			const file = path.join(outDir, runs > 1 ? `studio-${run}.json` : "studio.json");
			fs.writeFileSync(file, JSON.stringify(result, null, 1));
			process.stdout.write(`studio${runs > 1 ? ` run ${run}` : ""}: ${result.__error ? `error ${result.__error}` : "ok"} -> ${path.relative(ROOT, file)}\n`);
		}
	}
	if (!args["studio-only"]) {
		const result = runOffline(source, outDir, Number(args.time || 4), { project: Boolean(args.project), clock: args.clock && args.clock !== true ? String(args.clock) : null });
		const file = path.join(outDir, args.clock && args.clock !== true ? "ours-clock.json" : "ours.json");
		fs.writeFileSync(file, JSON.stringify(result, null, 1));
		process.stdout.write(`offline: ${result.__error ? `error ${result.__error}` : "ok"} -> ${path.relative(ROOT, file)}\n`);
	}
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	await main();
}
