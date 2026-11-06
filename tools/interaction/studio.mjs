// Studio input experiments: mounts a fixture + the recorder (tools/interaction/recorder.luau) under
// CoreGui, delivers input by posting window messages to the Studio viewport HWND (no cursor movement,
// tools/interaction/post.ps1) and/or through UserInputService:CreateVirtualInput() (keys / text), reads
// the recorded log and always destroys the GUI. Holds the Studio lock for the whole experiment.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { runLuau, DEFAULT_STUDIO } from "../../cli/studio.mjs";
import { withStudioLock } from "../../cli/lock.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const RECORDER = fs.readFileSync(path.join(HERE, "recorder.luau"), "utf8").replace(/\nreturn Recorder\s*$/, "\n");
export const DPI = 1.5;

// Runs a chunk with the recorder in scope as `R`.
const withRecorder = code => `local R = _G.__RUR_INPUT
local HttpService = game:GetService("HttpService")
local UIS = game:GetService("UserInputService")
${code}`;

export function postMessages(studioName, viewport, actions) {
	const title = studioName.replace(/\s*\(placeId:.*$/, "").replace(/\s*-\s*Roblox Studio$/, "");
	const script = path.join(HERE, "post.ps1");
	// Long sequences exceed the command-line limit: pass them in a file
	const actionsFile = path.join(os.tmpdir(), `rur-post-${process.pid}-${Date.now()}.json`);
	fs.writeFileSync(actionsFile, JSON.stringify(actions));
	try {
		const result = spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, "-Title", title, "-Width", String(viewport[0]), "-Height", String(viewport[1]), "-ActionsFile", actionsFile], { encoding: "utf8" });
		if (result.status !== 0) throw new Error(`post failed: ${result.error || result.stderr || result.stdout}`);
		return result.stdout.trim();
	} finally {
		fs.rmSync(actionsFile, { force: true });
	}
}

// experiment(api): api.luau(code) runs Luau with the recorder as `R` and returns its string result;
// api.post(actions) posts window messages (physical client px); api.physical(x, y) converts logical px.
export async function withStudioInput(name, setupCode, experiment, { studio = DEFAULT_STUDIO } = {}) {
	return withStudioLock(`interaction ${name}`, async () => {
		const setup = await runLuau(`${RECORDER}
Recorder.Before = Recorder.Safety()
local R = Recorder
local UIS = game:GetService("UserInputService")
${setupCode}
R.Gui.Parent = game:GetService("CoreGui")
local Camera = workspace.CurrentCamera
return game:GetService("HttpService"):JSONEncode({ viewport = { Camera.ViewportSize.X, Camera.ViewportSize.Y }, safety = R.Before })`, { studio });
		const info = JSON.parse(setup.text);
		const viewport = [Math.round(info.viewport[0] * DPI), Math.round(info.viewport[1] * DPI)];
		const api = {
			studio: setup.studio,
			info,
			luau: async code => (await runLuau(withRecorder(code), { studio })).text,
			post: actions => postMessages(setup.studio.name, viewport, actions),
			physical: (x, y) => [Math.round(x * DPI), Math.round(y * DPI)],
		};
		let result;
		let final = null;
		try {
			result = await experiment(api);
		} finally {
			const text = (await runLuau(withRecorder(`
if not R then
	return "{}"
end
local After = R.Safety()
local Log = R.Log
local Before = R.Before
R.Destroy()
-- A stray click selected something while nothing was selected before: restore the empty selection
if Before.selection == 0 and After.selection > 0 then
	local Selection = game:GetService("Selection")
	local Names = {}
	for _, Item in Selection:Get() do
		table.insert(Names, Item:GetFullName())
	end
	After.selectionRestored = table.concat(Names, "; ")
	Selection:Set({})
end
return HttpService:JSONEncode({ log = Log, before = Before, after = After })`), { studio })).text;
			final = JSON.parse(text);
		}
		const before = final.before || {};
		const after = final.after || {};
		const changed = Object.keys(before).filter(key => before[key] !== after[key]);
		if (changed.length) process.stderr.write(`SAFETY: Studio state changed during ${name}: ${changed.map(key => `${key}: ${before[key]} -> ${after[key]}`).join("; ")}\n`);
		return { result, log: final.log || [], safety: { before, after, changed } };
	});
}

// Compact one-line-per-entry rendering of a recorder log.
export function formatLog(log) {
	return log.map(entry => {
		const head = `f${String(entry.f).padStart(4)} ${entry.t.toFixed(4)}`;
		if (entry.note) return `${head} -- ${entry.note}`;
		if (entry.p) return `${head} ${entry.o ? entry.o + "." : ""}${entry.p} = ${JSON.stringify(entry.v)}`;
		const args = (entry.a || []).map(arg => (arg && typeof arg === "object" && arg.input ? `#${arg.id}{${arg.input} ${arg.state} key=${arg.key} pos=${arg.pos.map(v => +v.toFixed(2)).join(",")} d=${arg.delta.map(v => +v.toFixed(2)).join(",")}}` : JSON.stringify(arg)));
		return `${head} ${entry.o ? entry.o + "." : ""}${entry.e}(${args.join(", ")})`;
	}).join("\n");
}

// Action builders in logical px (converted to the viewport's physical client px).
export const act = {
	// Moves carry MK_SHIFT when no button is held: Qt 5 drops button-less moves while it has no window
	// under the real cursor (always, as the real cursor is elsewhere). Qt reads modifiers from the
	// keyboard state, so the flag does not reach Studio as Shift.
	move: (x, y, held = 0) => ({ op: "move", x: Math.round(x * DPI), y: Math.round(y * DPI), keys: held ? held : 4 }),
	down: (x, y, button = 1) => ({ op: "down", button, x: Math.round(x * DPI), y: Math.round(y * DPI), keys: button === 1 ? 1 : button === 2 ? 2 : 16 }),
	up: (x, y, button = 1) => ({ op: "up", button, x: Math.round(x * DPI), y: Math.round(y * DPI), keys: 0 }),
	wheel: (x, y, delta, horizontal = false) => ({ op: "wheel", delta, x: Math.round(x * DPI), y: Math.round(y * DPI), keys: 0, horizontal }),
	sleep: ms => ({ op: "sleep", ms }),
	// Messages posted back to back in one call (see WinInput.PostBatch): no Qt leave between them
	batch: (...items) => ({ op: "batch", items: items.map(item => ({ button: 0, delta: 0, keys: 0, ...item })) }),
};

// Luau that builds a full-screen transparent Active frame under everything (sinks stray clicks).
export const COVER = `
local Cover = Instance.new("Frame")
Cover.Name = "Cover"
Cover.Size = UDim2.fromScale(1, 1)
Cover.BackgroundTransparency = 1
Cover.Active = true
Cover.ZIndex = -100
Cover.Parent = R.Gui
`;

// Posts `items` (act.* message builders, sent back to back) and captures `count` crops of the viewport
// region crop = [x, y, w, h] (physical px) every `intervalMs`; returns [{ file, t }] with t in ms since
// the messages were posted.
export function postAndCaptureSeries(studioName, viewport, items, { count, intervalMs, crop, outPrefix }) {
	const title = studioName.replace(/\s*\(placeId:.*$/, "").replace(/\s*-\s*Roblox Studio$/, "");
	const actionsFile = path.join(os.tmpdir(), `rur-series-${process.pid}-${Date.now()}.json`);
	fs.writeFileSync(actionsFile, JSON.stringify(items));
	try {
		const script = path.join(HERE, "series.ps1");
		const result = spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, "-Title", title, "-Width", String(viewport[0]), "-Height", String(viewport[1]), "-ActionsFile", actionsFile, "-Count", String(count), "-IntervalMs", String(intervalMs), "-Crop", crop.join(","), "-OutPrefix", outPrefix], { encoding: "utf8", maxBuffer: 1 << 24 });
		if (result.status !== 0) throw new Error(`series failed: ${result.error || result.stderr || result.stdout}`);
		const times = JSON.parse(fs.readFileSync(`${outPrefix}times.json`, "utf8"));
		return times.map((t, i) => ({ file: `${outPrefix}${String(i).padStart(3, "0")}.png`, t }));
	} finally {
		fs.rmSync(actionsFile, { force: true });
	}
}
