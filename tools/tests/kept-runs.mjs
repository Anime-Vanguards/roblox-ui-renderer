// The render service keeps the last render's run (native/src/serve.rs, engine/cli.rs Kept): a render of the same
// story and options at a later moment goes on from it instead of mounting the story again, and writes what a run
// from the start writes at that moment. A render loop through the service against renders in processes of their
// own: every scene equal but its timings, the later moments gone on from the earlier ones, and an earlier moment,
// other options or an edited source started over.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-kept-"));
const env = { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: "", RUR_ISOLATED: "" };
const fixture = path.join(dir, "kept.luau");
// A tween and a spring-like loop, so every moment differs
fs.writeFileSync(fixture, `return function(Target)
	local Bar = Instance.new("Frame")
	Bar.Name = "Bar"
	Bar.Size = UDim2.fromOffset(20, 20)
	Bar.Parent = Target
	game:GetService("TweenService"):Create(Bar, TweenInfo.new(1, Enum.EasingStyle.Quad), { Size = UDim2.fromOffset(300, 40) }):Play()
	local Dot = Instance.new("Frame")
	Dot.Name = "Dot"
	Dot.Size = UDim2.fromOffset(10, 10)
	Dot.Parent = Target
	local Time = 0
	game:GetService("RunService").Heartbeat:Connect(function(Step)
		Time += Step
		Dot.Position = UDim2.fromOffset(150 + math.sin(Time * 7) * 100, 100)
	end)
end
`);
const base = { fixture, roots: path.join(root, "tools/tests/oracle-root"), viewport: "400x300", "no-snapshots": true };

const scene = out => {
	const value = JSON.parse(fs.readFileSync(path.join(out, "scene.json"), "utf8"));
	delete value.diagnostics.timings;
	return JSON.stringify(value);
};
// A render in a process of its own, as a run from the start
const fresh = (time, name) => {
	const out = path.join(dir, "fresh", name);
	const args = Object.entries({ ...base, time, out }).flatMap(([key, value]) => (value === true ? [`--${key}`] : [`--${key}`, String(value)]));
	const result = spawnSync(process.execPath, ["cli/rur.mjs", "render", ...args, "--no-png", "--json"], { cwd: root, encoding: "utf8", windowsHide: true, env: { ...env, RUR_NO_SERVICE: "1" } });
	assert.equal(result.status, 0, result.stderr);
	return out;
};
const log = () => fs.readFileSync(path.join(dir, "state", "serve.log"), "utf8");

const client = await import("../../cli/client.mjs");
try {
	const served = async (time, name, extra = {}) => {
		const out = path.join(dir, "served", name);
		const result = await client.render({ ...base, ...extra, time, out, "no-png": true }, { env });
		assert.ok(result.summary, result.error);
		assert.equal(result.summary.status, "ok", result.summary.failure);
		return out;
	};
	const times = ["0.1", "0.1167", "0.5", "0.5", "1.25"];
	for (const [index, time] of times.entries()) {
		const out = await served(time, `t${index}`);
		assert.equal(scene(out), scene(fresh(time, `t${index}`)), `the scene at ${time} s`);
	}
	const lines = log().split("\n").filter(line => / render kept\.luau /.test(line));
	assert.match(lines[0], /\((fresh VM|standby),/, "the first render mounts the story");
	for (const [index, line] of lines.slice(1, times.length).entries()) {
		assert.match(line, new RegExp(`kept run from ${Number(times[index]).toFixed(4)} s`), "a later moment goes on from the one before");
	}
	// An earlier moment, or other options, starts over (and is right)
	assert.equal(scene(await served("0.3", "earlier")), scene(fresh("0.3", "earlier")));
	assert.match(log(), /kept run of kept\.luau not gone on from 1\.2500 s: 0\.3000 s is before/);
	await served("0.4", "background", { background: "10,20,30" });
	assert.doesNotMatch(log().split("\n").filter(line => / render kept\.luau /.test(line)).at(-1), /kept run from/, "other options mount the story again");
	// An edited source: the next moment starts over and shows the edit
	await served("0.5", "before-edit", { background: "10,20,30" });
	fs.writeFileSync(fixture, fs.readFileSync(fixture, "utf8").replace("UDim2.fromOffset(300, 40)", "UDim2.fromOffset(200, 60)"));
	const edited = await served("0.6", "edited", { background: "10,20,30" });
	assert.match(log(), /kept run of kept\.luau not gone on from 0\.5000 s: .*kept\.luau changed/);
	assert.equal(JSON.parse(fs.readFileSync(path.join(edited, "scene.json"), "utf8")).nodes.find(node => node.name === "Bar").rect[3] > 40, true, "the edit shows");
	console.log("kept runs: PASS (a render loop through the service goes on from the kept run and writes a run from the start's scenes; an earlier moment, other options and an edited source start over)");
} finally {
	await client.stop({ env });
	// (the service lets go of its log as it exits)
	fs.rmSync(dir, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
}
