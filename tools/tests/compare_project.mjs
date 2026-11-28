// `rur compare` asks Studio for a story where the run's project mounts it, as the offline run mounts it (cli/rojo.mjs
// instancePathOf, cli/compare.mjs storyInstancePath): Rojo projects (--rojo, --project <file.project.json>, the one
// found beside the story), --mount, --roots and a project's rur.json places. A story no mount holds is refused before
// Studio is asked (exit 64), and a story that does not mount in Studio is not scored against an empty capture (exit 2).
// The names are checked against the engine's own (a render that prints every mounted script), and the command against
// a stand-in Studio MCP bridge.
import assert from "node:assert/strict";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { detectRojoProject, instancePathOf, runMounts } from "../../cli/rojo.mjs";
import { checkStudioMount, StudioFailed, storyInstancePath } from "../../cli/compare.mjs";
import { UsageError } from "../../cli/actions.mjs";
import { placeRoots } from "../../cli/places.mjs";
import { projectRoots } from "../../cli/rur.mjs";
import { ensureEngine } from "../../cli/engine.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-compare-project-"));
const at = file => path.join(dir, file);
const write = (file, text) => {
	fs.mkdirSync(path.dirname(at(file)), { recursive: true });
	fs.writeFileSync(at(file), text);
};
const STORY = "return function(Target)\n\tlocal Frame = Instance.new(\"Frame\")\n\tFrame.Size = UDim2.fromOffset(40, 20)\n\tFrame.Parent = Target\nend\n";

let server;
try {
	// A DataModel project (the issue's layout: the client folder under StarterPlayerScripts), a plugin's own tree, a
	// folder mounted as a service's contents, a nested default.project.json, --mount folders and files, layout roots
	write("game.project.json", JSON.stringify({ name: "Game", tree: { $className: "DataModel",
		ReplicatedStorage: { Shared: { $path: "src/shared" } },
		StarterPlayer: { $className: "StarterPlayer", StarterPlayerScripts: { $className: "StarterPlayerScripts", Client: { $path: "src/client" } } },
		ServerStorage: { $path: "src/server" } } }));
	write("plugin.project.json", JSON.stringify({ name: "MyPlugin", tree: { $path: "src/plugin" } }));
	const files = {
		"src/client/Ui/Hud.story.luau": STORY,
		"src/client/init.client.luau": "-- client entry\n",
		"src/shared/Panel/init.luau": "-- the folder's script: init.luau comes first\nreturn {}\n",
		"src/shared/Panel/init.server.luau": "-- after the folder's first init script: not mounted\n",
		"src/shared/Card/init.luau": STORY,
		"src/shared/Card/Parts.luau": "return {}\n",
		"src/shared/Run.server.luau": "-- a server script\n",
		"src/shared/Legacy.story.lua": STORY,
		"src/shared/Absent.story.luau": STORY,
		"src/shared/.hidden/Hidden.story.luau": STORY,
		"src/shared/.luau": "return {}\n",
		"src/shared/.git/Inside.story.luau": STORY,
		"src/shared/init.story.luau": STORY,
		"src/shared/Odd.Server.luau": STORY,
		"src/shared/Helper.plugin.luau": "-- a plugin's script\n",
		"src/shared/Upper.LUAU": STORY,
		"src/shared/Kit/default.project.json": JSON.stringify({ name: "UiKit", tree: { $path: "lib" } }),
		"src/shared/Kit/lib/Button.story.luau": STORY,
		"src/shared/Kit/Other.story.luau": STORY,
		"src/server/Tool.story.luau": STORY,
		"src/server/init.luau": "return {}\n",
		"src/plugin/App.story.luau": STORY,
		"screens/Main.story.luau": STORY,
		"single.story.luau": STORY,
		"layout/ReplicatedStorage/Stories/Badge.story.luau": STORY,
		"layout/Loose.story.luau": STORY,
		"stories/Outside.story.luau": STORY,
	};
	for (const [file, text] of Object.entries(files)) write(file, text);

	const game = runMounts({ rojo: at("game.project.json") });
	const mounts = [...game, ...runMounts({ mount: [`StarterGui/Screens=${at("screens")}`, `ReplicatedStorage/Single=${at("single.story.luau")}`] })];
	const roots = [at("layout")];
	const expected = {
		"src/client/Ui/Hud.story.luau": "StarterPlayer.StarterPlayerScripts.Client.Ui.Hud.story",
		"src/client/init.client.luau": "StarterPlayer.StarterPlayerScripts.Client",
		"src/shared/Panel/init.luau": "ReplicatedStorage.Shared.Panel",
		"src/shared/Card/init.luau": "ReplicatedStorage.Shared.Card",
		"src/shared/Card/Parts.luau": "ReplicatedStorage.Shared.Card.Parts",
		"src/shared/Run.server.luau": "ReplicatedStorage.Shared.Run",
		"src/shared/Legacy.story.lua": "ReplicatedStorage.Shared.Legacy.story",
		// (an init file that isn't the folder's script mounts by its own rule; a suffix takes the rule only in lower
		// case, so `.Server.luau` is a module's name; `.plugin.luau` is a Script)
		"src/shared/init.story.luau": "ReplicatedStorage.Shared.init.story",
		"src/shared/Odd.Server.luau": "ReplicatedStorage.Shared.Odd.Server",
		"src/shared/Helper.plugin.luau": "ReplicatedStorage.Shared.Helper",
		// (names that start with a dot mount as Rojo builds them: a dot folder is a Folder, and a file named only
		// by a module's suffix keeps it)
		"src/shared/.hidden/Hidden.story.luau": "ReplicatedStorage.Shared..hidden.Hidden.story",
		"src/shared/.luau": "ReplicatedStorage.Shared..luau",
		"src/shared/Kit/lib/Button.story.luau": "ReplicatedStorage.Shared.UiKit.Button.story",
		"src/server/Tool.story.luau": "ServerStorage.Tool.story",
		"screens/Main.story.luau": "StarterGui.Screens.Main.story",
		"single.story.luau": "ReplicatedStorage.Single",
		"layout/ReplicatedStorage/Stories/Badge.story.luau": "ReplicatedStorage.Stories.Badge.story",
	};
	const name = file => instancePathOf(at(file), { roots, mounts })?.join(".") ?? null;
	for (const [file, full] of Object.entries(expected)) assert.equal(name(file), full, file);
	// Not mounted: an init script after the folder's first one, what a .git holds (an empty Folder), a script suffix in
	// upper case, what a nested project doesn't map, a service folder's own init script, a file at a root's top, a file
	// outside every mount
	for (const file of ["src/shared/Panel/init.server.luau", "src/shared/.git/Inside.story.luau", "src/shared/Upper.LUAU", "src/shared/Kit/Other.story.luau", "src/server/init.luau", "layout/Loose.story.luau", "stories/Outside.story.luau"]) {
		assert.equal(name(file), null, file);
	}

	// Every way a run chooses its project gives the offline run's names
	const parts = (file, args) => storyInstancePath(at(file), { roots: projectRoots(args), mounts: runMounts(args) }).join(".");
	assert.equal(parts("src/client/Ui/Hud.story.luau", { rojo: at("game.project.json") }), expected["src/client/Ui/Hud.story.luau"]);
	assert.equal(parts("src/client/Ui/Hud.story.luau", { project: at("game.project.json") }), expected["src/client/Ui/Hud.story.luau"]);
	assert.equal(parts("src/plugin/App.story.luau", { rojo: at("plugin.project.json") }), "ReplicatedStorage.MyPlugin.App.story");
	assert.equal(parts("src/plugin/App.story.luau", { rojo: at("plugin.project.json"), "rojo-at": "ServerStorage/Tools" }), "ServerStorage.Tools.App.story");
	assert.equal(parts("screens/Main.story.luau", { mount: `StarterGui/Screens=${at("screens")}` }), "StarterGui.Screens.Main.story");
	assert.equal(parts("layout/ReplicatedStorage/Stories/Badge.story.luau", { roots: at("layout") }), "ReplicatedStorage.Stories.Badge.story");
	// The project found beside the story, with no project options (cli/rur.mjs main detects it before compare runs)
	write("auto/default.project.json", JSON.stringify({ name: "Auto", tree: { $className: "DataModel", ReplicatedStorage: { Shared: { $path: "src/shared" } } } }));
	write("auto/src/shared/Stories/Card.story.luau", STORY);
	const detected = { story: at("auto/src/shared/Stories/Card.story.luau") };
	assert.match(detectRojoProject(detected, at("elsewhere")), /^rur: Rojo project /);
	assert.equal(parts("auto/src/shared/Stories/Card.story.luau", detected), "ReplicatedStorage.Shared.Stories.Card.story");
	// A project's rur.json places are layout roots
	write("placed/rur.json", JSON.stringify({ places: [{ name: "client", roots: ["shared", "client"] }] }));
	write("placed/client/StarterGui/Menu.story.luau", STORY);
	assert.equal(storyInstancePath(at("placed/client/StarterGui/Menu.story.luau"), { roots: placeRoots(at("placed"), "client") }).join("."), "StarterGui.Menu.story");
	// A story the project doesn't hold: refused, naming what the run mounts
	assert.throws(() => storyInstancePath(at("stories/Outside.story.luau"), { mounts: game }), error => error instanceof UsageError
		&& /Outside\.story\.luau is not in the run's project \(ReplicatedStorage\.Shared = .*\), so Studio has no module of it to mount: name the project that holds it/.test(error.message));
	assert.throws(() => storyInstancePath(at("stories/Outside.story.luau"), {}), /not in the run's project \(it has no roots or mounts\)/);

	// Studio's answer: only a story that mounted is captured
	assert.equal(checkStudioMount({ status: "ok", nodes: [] }, { source: "A" }).status, "ok");
	assert.throws(() => checkStudioMount({ status: "error", failure: "story module not found in Studio: A.B" }, { source: "B.story" }),
		error => error instanceof StudioFailed && error.message === "Studio could not mount B.story: story module not found in Studio: A.B (is the project synced into that Studio?); nothing was compared");
	assert.throws(() => checkStudioMount({ status: "error", failure: "boom" }, { source: "B.story" }), { message: "Studio could not mount B.story: boom; nothing was compared" });
	assert.throws(() => checkStudioMount(undefined, { source: "B.story" }), StudioFailed);

	// The engine mounts each file at the name above (and the unmounted ones nowhere): a story outside the project, run
	// as a fixture, prints every script in the DataModel
	const engine = process.env.RUR_NATIVE_ENGINE || await ensureEngine();
	const env = { ...process.env, RUR_NATIVE_ENGINE: engine, RUR_STATE_DIR: at("state"), RUR_NO_SERVICE: "1" };
	for (const key of ["RUR_PROJECT", "RUR_COMPARE_CAPTURE", "RUR_TEXT_SUBPIXEL", "RUR_STUDIO_CAPTURE", "RUR_STUDIO", "RUR_SIM_FLAGS"]) delete env[key];
	// (run from the project's folder, as the story's path relative to it is what Studio used to be asked for)
	const cli = (args, extraEnv = {}) => new Promise((resolve, reject) => {
		const child = spawn(process.execPath, [path.join(root, "cli", "rur.mjs"), ...args], { cwd: dir, windowsHide: true, env: { ...env, ...extraEnv } });
		let stdout = "", stderr = "";
		child.stdout.on("data", chunk => { stdout += chunk; });
		child.stderr.on("data", chunk => { stderr += chunk; });
		child.on("error", reject);
		child.on("close", status => resolve({ status, stdout, stderr }));
	});
	write("probe/Probe.story.luau", `for _, Item in game:GetDescendants() do
	if Item:IsA("LuaSourceContainer") then
		print("MOUNTED " .. Item.ClassName .. " " .. Item:GetFullName())
	end
end
return function() end
`);
	const probe = await cli(["render", "--rojo", at("game.project.json"), "--mount", `StarterGui/Screens=${at("screens")}`, "--mount", `ReplicatedStorage/Single=${at("single.story.luau")}`,
		"--roots", at("layout"), "--story", at("probe/Probe.story.luau"), "--no-png", "--offline", "--json", "--out", at("out/probe")]);
	assert.equal(probe.status, 0, probe.stderr);
	const output = JSON.parse(fs.readFileSync(at("out/probe/scene.json"), "utf8")).diagnostics.output || [];
	const scripts = output.map(entry => String(entry.message)).filter(message => message.startsWith("MOUNTED ")).map(message => message.slice("MOUNTED ".length).split(/ (.*)/s));
	const mounted = new Set(scripts.map(([, full]) => full));
	for (const [file, full] of Object.entries(expected)) assert.ok(mounted.has(full), `the engine mounts ${file} at ${full} (it mounted ${[...mounted].join(", ")})`);
	for (const unmounted of ["Inside.story", "Upper", "Other.story", "Loose.story"]) assert.ok(![...mounted].some(full => full.endsWith(unmounted)), unmounted);
	// (the folder's first init script alone makes Panel; the plugin's is a Script, `.Server.luau` a module)
	const classes = full => scripts.filter(([, name]) => name === full).map(([kind]) => kind);
	assert.deepEqual(classes("ReplicatedStorage.Shared.Panel"), ["ModuleScript"]);
	assert.deepEqual(classes("ReplicatedStorage.Shared.Helper"), ["Script"]);
	assert.deepEqual(classes("ReplicatedStorage.Shared.Odd.Server"), ["ModuleScript"]);

	// A stand-in Studio MCP bridge (cli/studio.mjs: newline-delimited JSON-RPC, execute_luau results paged through
	// _G.__RUR_RESULT): it has the modules `present` names, answers the mount script as Studio's would and captures a
	// 200 x 100 viewport at scale 1 (cli/studio.mjs captureEngine)
	const present = new Set([expected["src/client/Ui/Hud.story.luau"], "ReplicatedStorage.Shared.Stories.Card.story"]);
	const seen = { mounts: [], unmounts: 0 };
	const answer = code => {
		if (code.includes('GetFastFlag("LegacyTextSubpixelRendering")')) return "false";
		if (code.includes("Input.MouseIconEnabled = false")) return "false";
		if (code.includes("MouseIconEnabled = true")) return "ok";
		if (code.includes('return "unmounted"')) {
			seen.unmounts++;
			return "unmounted";
		}
		if (code.includes("CurrentCamera.ViewportSize return tostring")) return "200,100";
		if (code.includes("CaptureService:CaptureScreenshot")) {
			const pixels = Buffer.alloc(5);
			pixels.writeUInt16LE(200 * 100, 0);
			pixels.set([46, 46, 46], 2);
			return Buffer.concat([Buffer.from("200 100 200 100 200.000000|", "latin1"), pixels]);
		}
		const literal = /local PathParts = HttpService:JSONDecode\("((?:[^"\\]|\\.)*)"\)/.exec(code);
		if (literal) {
			const pathParts = JSON.parse(literal[1].replace(/\\(.)/g, "$1"));
			seen.mounts.push(pathParts.join("."));
			const found = present.has(pathParts.join("."));
			return JSON.stringify({ status: found ? "ok" : "error", failure: found ? undefined : `story module not found in Studio: ${pathParts.join(".")}`,
				viewport: [200, 100], placeId: 0, target: [0, 0, 200, 100], nodes: [], frozen: false, probes: {} });
		}
		throw new Error(`the stand-in Studio has no answer for ${code.slice(0, 80)}`);
	};
	let stored = "";
	server = net.createServer(socket => {
		let buffer = "";
		socket.on("data", chunk => {
			buffer += chunk.toString("utf8");
			for (let index; (index = buffer.indexOf("\n")) >= 0;) {
				const line = buffer.slice(0, index).trim();
				buffer = buffer.slice(index + 1);
				const message = line ? JSON.parse(line) : {};
				if (message.id === undefined) continue;
				const reply = text => socket.write(`${JSON.stringify({ jsonrpc: "2.0", id: message.id, result: message.method === "initialize" ? {} : { content: [{ type: "text", text }] } })}\n`);
				if (message.method !== "tools/call") reply("");
				else if (message.params.name === "list_roblox_studios") reply(JSON.stringify({ studios: [{ id: "stand-in", name: "FakeStudio - Roblox Studio" }] }));
				else {
					const code = message.params.arguments.code;
					const page = /^return string\.sub\(_G\.__RUR_RESULT, (\d+), (\d+)\)$/.exec(code);
					if (page) reply(stored.slice(Number(page[1]) - 1, Number(page[2])));
					else if (code.startsWith("_G.__RUR_RESULT = nil")) reply("0");
					else {
						const inner = code.slice(code.indexOf("(function()\n") + "(function()\n".length, code.lastIndexOf("\nend)()\nif type(__rur_result)"));
						stored = Buffer.from(answer(inner)).toString("base64");
						reply(String(stored.length));
					}
				}
			}
		});
	});
	await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
	const port = String(server.address().port);
	const backend = [];
	const studioEnv = {
		RUR_STUDIO_MCP_PORT: port, RUR_STUDIO_LOCK: at("studio.lock"), RUR_STUDIO_CAPTURE: "engine",
	};
	const compare = (out, ...args) => cli(["compare", ...args, ...backend, "--studio", "FakeStudio", "--viewport", "200x100", "--time", "0", "--no-calibrate", "--no-clock", "--no-snapshots", "--offline", "--json", "--out", at(`out/${out}`)], studioEnv);

	// --rojo: Studio is asked for the module where the project mounts it, and the compare is scored
	const hud = await compare("hud", "--rojo", at("game.project.json"), "--story", at("src/client/Ui/Hud.story.luau"));
	assert.deepEqual(seen.mounts, [expected["src/client/Ui/Hud.story.luau"]]);
	assert.equal(hud.status, 0, hud.stderr);
	assert.equal(JSON.parse(fs.readFileSync(at("out/hud/report.json"), "utf8")).studio.status, "ok");
	assert.equal(seen.unmounts, 1);
	// No project options: the project beside the story
	const card = await compare("card", "--story", at("auto/src/shared/Stories/Card.story.luau"));
	assert.equal(card.status, 0, card.stderr);
	assert.equal(seen.mounts.at(-1), "ReplicatedStorage.Shared.Stories.Card.story");
	// A module Studio doesn't have: exit 2 with Studio's reason, no capture taken and no report
	const missing = await compare("missing", "--rojo", at("game.project.json"), "--story", at("src/shared/Absent.story.luau"));
	assert.equal(missing.status, 2, missing.stderr);
	assert.match(missing.stderr, /rur compare: Studio could not mount Absent\.story\.luau: story module not found in Studio: ReplicatedStorage\.Shared\.Absent\.story \(is the project synced into that Studio\?\); nothing was compared/);
	assert.equal(fs.existsSync(at("out/missing/report.json")), false);
	assert.equal(fs.existsSync(at("out/missing/studio-capture.json")), false);
	assert.equal(seen.unmounts, 3, "the failed mount is unmounted");
	// A story outside the project: exit 64 before Studio is asked
	const asked = seen.mounts.length;
	const outside = await compare("outside", "--rojo", at("game.project.json"), "--story", at("stories/Outside.story.luau"));
	assert.equal(outside.status, 64, outside.stderr);
	assert.match(outside.stderr, /^rur: compare: .*Outside\.story\.luau is not in the run's project/m);
	assert.equal(seen.mounts.length, asked);
} finally {
	server?.close();
	fs.rmSync(dir, { recursive: true, force: true });
}
console.log("compare projects: ok (Rojo projects, nested projects, --mount, --roots, places, detection, the engine's names, exit 2 and 64)");
