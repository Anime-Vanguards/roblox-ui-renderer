// Snapshot defaults must not import one project's Studio instances into another project's runtime.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import { snapshotDir, markdownCell } from "../../cli/rur.mjs";
import { luauString } from "../../cli/studio.mjs";

// Luau string literals: quotes and backslashes escaped, control characters as Luau's \ddd (not JSON's \uXXXX)
assert.equal(luauString('a"b\\c'), '"a\\"b\\\\c"');
assert.equal(luauString("line\nnext\ttab\u0001\u007f!"), '"line\\nnext\\ttab\\001\\127!"');
assert.equal(luauString("\u00e9\u{1f600}"), '"\u00e9\u{1f600}"');
assert.equal(luauString("\ud800"), '"\ufffd"');
assert.equal(luauString(12), '"12"');
// Markdown cells: pipes escaped, and a backslash cannot escape the cell's own closing pipe
assert.equal(markdownCell("a|b"), "a\\|b");
assert.equal(markdownCell("ends with \\"), "ends with \\\\");
assert.equal(markdownCell("x".repeat(199) + "\\|"), "x".repeat(199) + "\\\\");
assert.equal(markdownCell("two\r\nlines"), "two lines");

const root = fileURLToPath(new URL("../../", import.meta.url));
// No project or place has an implicit Studio snapshot: they load only when --snapshot-dir names one
{
	const other = path.resolve(root, "out/another-project");
	const snapshot = path.resolve(root, "out/a-snapshot");
	const exists = fs.existsSync;
	fs.existsSync = () => true;
	try {
		assert.equal(snapshotDir({}), undefined);
		assert.equal(snapshotDir({ project: other }), undefined);
		assert.equal(snapshotDir({ project: other, place: "client" }), undefined);
		assert.equal(snapshotDir({ project: other, roots: "custom" }), undefined);
		assert.equal(snapshotDir({ project: other, "snapshot-dir": snapshot }), snapshot);
		assert.equal(snapshotDir({ "snapshot-dir": snapshot, "no-snapshots": true }), snapshot);
	} finally {
		fs.existsSync = exists;
	}
}
console.log("CLI snapshot isolation: PASS (project, environment, roots, place, explicit opt-in)");

// Rojo projects, stories outside the mounted roots, fixture --controls, physical viewports,
// --json output, --git-ref, the plugin stand-in and `inspect tree --filter` ancestry.
{
	const { parseArgs, resolveViewport } = await import("../../cli/rur.mjs");
	const { rojoMounts } = await import("../../cli/rojo.mjs");
	const { treeLines } = await import("../../cli/inspect.mjs");
	const os = await import("node:os");

	assert.deepEqual(parseArgs(["render", "--mount", "A/B=x", "--mount", "C/D=y", "--json"]).mount, ["A/B=x", "C/D=y"]);
	assert.equal(resolveViewport({ viewport: "1800x1350@physical" }), "1200x900");
	assert.equal(resolveViewport({ viewport: "1000x500@physical", scale: "2" }), "500x250");
	assert.equal(resolveViewport({ viewport: "800x600" }), "800x600");
	assert.throws(() => resolveViewport({ viewport: "1001x900@physical" }), /not whole pixels/);

	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cli-test-"));
	const write = (file, text) => {
		fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
		fs.writeFileSync(path.join(dir, file), text);
	};
	write("plugin.project.json", JSON.stringify({ name: "MyPlugin", tree: { $path: "src" } }));
	write("game.project.json", JSON.stringify({ name: "Game", tree: { $className: "DataModel",
		ReplicatedStorage: { Shared: { $path: "src" }, Packages: { $className: "Folder", Inner: { $path: { optional: "missing" } } } },
		StarterGui: { Hud: { $className: "ScreenGui" } } } }));
	write("src/init.server.luau", "-- plugin entry");
	write("src/App.luau", `return function(Target, Controls)
	local Label = Instance.new("TextLabel")
	Label.Name = "Title"
	Label.Size = UDim2.fromOffset(200, 40)
	Label.Text = if Controls and Controls.Title then Controls.Title else "Hello"
	Label.Parent = Target
	return Label
end
`);
	write("stories/App.story.luau", `local App = require(game:GetService("ReplicatedStorage").MyPlugin.App)
return { controls = { Title = "Story" }, story = function(Props) App(Props.target, { Title = Props.controls.Title }) end }
`);
	write("stories/fixture.luau", `local App = require(game:GetService("ReplicatedStorage").MyPlugin.App)
return function(Target, Controls) App(Target, Controls) end
`);
	write("stories/Widget.story.luau", `local App = require(game:GetService("ReplicatedStorage").MyPlugin.App)
return function()
	local Widget = plugin:CreateDockWidgetPluginGui("Panel", DockWidgetPluginGuiInfo.new(Enum.InitialDockState.Float, true))
	App(Widget, { Title = "Setting " .. tostring(plugin:GetSetting("Theme")) })
	plugin:CreateToolbar("T"):CreateButton("B", "", "").Click:Connect(function() end)
end
`);

	assert.deepEqual(rojoMounts(path.join(dir, "plugin.project.json")), [{ kind: "mount", path: "ReplicatedStorage/MyPlugin", value: path.join(dir, "src") }]);
	assert.deepEqual(rojoMounts(path.join(dir, "plugin.project.json"), { at: "ServerStorage/P" })[0].path, "ServerStorage/P");
	assert.deepEqual(rojoMounts(path.join(dir, "game.project.json")), [
		{ kind: "mount", path: "ReplicatedStorage/Shared", value: path.join(dir, "src") },
		{ kind: "instance", path: "ReplicatedStorage/Packages", value: "Folder" },
		{ kind: "instance", path: "StarterGui/Hud", value: "ScreenGui" },
	]);

	// native/src/rojo.rs gives the same mounts as cli/rojo.mjs (`rur-native rojo-mounts`), in the same order
	{
		const { ensureEngine } = await import("../../cli/engine.mjs");
		const engine = await ensureEngine();
		const example = path.join(root, "examples", "rojo-wally", "default.project.json");
		for (const [file, at] of [[path.join(dir, "plugin.project.json")], [path.join(dir, "plugin.project.json"), "ServerStorage/P"], [path.join(dir, "game.project.json")], [example]]) {
			const result = spawnSync(engine, ["rojo-mounts", file, ...(at ? ["--rojo-at", at] : [])], { encoding: "utf8", windowsHide: true });
			assert.equal(result.status, 0, result.stderr);
			assert.deepEqual(JSON.parse(result.stdout), rojoMounts(file, { at }), `${path.basename(file)} ${at || ""}`);
		}
	}

	// `rur render --json`: stdout is exactly one JSON document
	const render = (...extra) => {
		const result = spawnSync(process.execPath, ["cli/rur.mjs", "render", "--rojo", path.join(dir, "plugin.project.json"), "--viewport", "600x450@physical", "--no-png", "--offline", "--json", ...extra],
			{ cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: "1" } });
		assert.equal(result.status, 0, result.stderr);
		const summary = JSON.parse(result.stdout);
		assert.equal(result.stdout.trim().split("\n").length, 1);
		assert.equal(summary.status, "ok", summary.failure);
		assert.deepEqual([summary.viewport.width, summary.viewport.height], [400, 300]);
		const scene = JSON.parse(fs.readFileSync(path.join(summary.out, "scene.json"), "utf8"));
		return { summary, scene, text: name => scene.nodes.find(node => node.name === name)?.props?.text };
	};
	const story = render("--story", path.join(dir, "stories/App.story.luau"), "--out", path.join(dir, "out/story"));
	assert.equal(story.text("Title"), "Story");
	assert.deepEqual(treeLines(story.summary.out, { filter: "Title" }).map(line => line.trim().split(" ").slice(0, 3).join(" ")),
		["ScreenGui RURStory [0,", "Frame StoryTarget [0,", "* TextLabel Title"]);
	const fixture = render("--fixture", path.join(dir, "stories/fixture.luau"), "--controls", '{"Title": "Controlled"}', "--out", path.join(dir, "out/fixture"));
	assert.equal(fixture.text("Title"), "Controlled");
	const widget = render("--story", path.join(dir, "stories/Widget.story.luau"), "--plugin", '{"Theme": "Dark"}', "--out", path.join(dir, "out/widget"));
	assert.equal(widget.scene.nodes.find(node => node.class === "DockWidgetPluginGui")?.visible, true);
	assert.equal(widget.text("Title"), "Setting Dark");

	// --git-ref: the committed story and sources, not the working tree's edits
	const git = (...args) => spawnSync("git", ["-C", dir, "-c", "user.name=rur", "-c", "user.email=rur@localhost", ...args], { encoding: "utf8", windowsHide: true });
	if (git("init", "-q").status === 0 && git("add", "-A").status === 0 && git("commit", "-qm", "base").status === 0) {
		write("stories/App.story.luau", fs.readFileSync(path.join(dir, "stories/App.story.luau"), "utf8").replace('"Story"', '"Edited"'));
		assert.equal(render("--story", path.join(dir, "stories/App.story.luau"), "--out", path.join(dir, "out/now")).text("Title"), "Edited");
		const before = render("--story", path.join(dir, "stories/App.story.luau"), "--git-ref", "HEAD", "--out", path.join(dir, "out/before"));
		assert.equal(before.text("Title"), "Story");
		assert.ok(before.summary.gitRef.commits[0].dir.startsWith(path.join(dir, "state", "git-refs")));
		assert.match(git("status", "--short", "--untracked-files=no").stdout, /^ M stories\/App\.story\.luau\s*$/);
	}
	fs.rmSync(dir, { recursive: true, force: true });
	console.log("CLI project options: PASS (rojo, native rojo parity, outside stories, fixture controls, @physical, --json, --git-ref, --plugin, tree ancestry)");
}

// Render diagnostics (cli/diagnostics.mjs): AutomaticSize-culled rows, wrapped lines a short box drops,
// glyphs no font has, script output in the summary, and paths through names holding dots.
diagnostics: {
	const { treeLines } = await import("../../cli/inspect.mjs");
	const { nodePaths } = await import("../../cli/diagnostics.mjs");
	const os = await import("node:os");
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cli-diag-"));
	fs.writeFileSync(path.join(dir, "diag.luau"), `return function(Target)
	print("hello")
	warn("careful")
	local Holder = Instance.new("Frame")
	Holder.Name = "Holder"
	Holder.Size = UDim2.fromOffset(200, 60)
	Holder.ClipsDescendants = true
	Holder.Parent = Target
	Instance.new("UIListLayout").Parent = Holder
	for i = 1, 4 do
		local Row = Instance.new("Frame")
		Row.Name = "Row" .. i
		Row.Size = UDim2.new(1, 0, 0, 0)
		Row.AutomaticSize = Enum.AutomaticSize.Y
		Row.Parent = Holder
		local Label = Instance.new("TextLabel")
		Label.Size = UDim2.fromOffset(180, 25)
		Label.Parent = Row
	end
	local Wrapped = Instance.new("TextLabel")
	Wrapped.Name = "Wrapped"
	Wrapped.Position = UDim2.fromOffset(250, 10)
	Wrapped.Size = UDim2.fromOffset(120, 30)
	Wrapped.TextWrapped = true
	Wrapped.TextSize = 14
	Wrapped.Text = "This is a long sentence that wraps over several lines and cannot fit"
	Wrapped.Parent = Target
	local Dotted = Instance.new("Frame")
	Dotted.Name = "Item.Name"
	Dotted.Position = UDim2.fromOffset(250, 110)
	Dotted.Size = UDim2.fromOffset(100, 30)
	Dotted.Parent = Target
	local Arrow = Instance.new("TextLabel")
	Arrow.Name = "Arrow"
	Arrow.Size = UDim2.fromScale(1, 1)
	Arrow.Text = "Sort \\u{25BE}"
	Arrow.Parent = Dotted
end
`);
	const result = spawnSync(process.execPath, ["cli/rur.mjs", "render", "--fixture", path.join(dir, "diag.luau"), "--viewport", "400x200", "--no-png", "--offline", "--json", "--out", path.join(dir, "out")],
		{ cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: "1" } });
	assert.equal(result.status, 0, result.stderr);
	const summary = JSON.parse(result.stdout);
	assert.deepEqual(JSON.parse(fs.readFileSync(path.join(dir, "out", "summary.json"), "utf8")).culled, summary.culled);
	// Rows 3 and 4 (centres below the 60 px clip) and their labels
	assert.equal(summary.culled.count, 2);
	assert.equal(summary.culled.nodes, 4);
	assert.match(summary.culled.examples[0], /Holder\.Row3 \(AutomaticSize/);
	assert.equal(summary.textLinesDropped.count, 1);
	assert.match(summary.textLinesDropped.examples[0], /\.Wrapped \(\d+ of \d+ lines dropped\)/);
	assert.equal(summary.missingGlyphs.length, 1);
	// Naming the character reads the font with opentype.js: a checkout without its packages installed (a
	// fresh worktree) reports the count alone, and this says so instead of failing on the name
	let named = true;
	try {
		createRequire(import.meta.url).resolve("opentype.js");
	} catch {
		named = false;
	}
	if (!named) {
		assert.equal(summary.missingGlyphs[0].code, null);
		assert.match(result.stderr, /rur: missing glyphs: glyphs in /);
		fs.rmSync(dir, { recursive: true, force: true });
		console.log("CLI render diagnostics: SKIPPED the missing glyph's name and the checks after it: opentype.js is not installed here (run npm install); culled rows, dropped lines and the glyph count passed");
		break diagnostics;
	}
	assert.equal(summary.missingGlyphs[0].code, "U+25BE");
	assert.equal(summary.missingGlyphs[0].paths[0], 'CoreGui.RURStory.StoryTarget["Item.Name"].Arrow');
	assert.deepEqual(summary.output, { total: 2, lines: ["print: hello", "warn: careful"] });
	assert.match(result.stderr, /rur: culled: 2 \(4 nodes;/);
	assert.match(result.stderr, /rur: text lines dropped: 1/);
	assert.match(result.stderr, /rur: missing glyphs: '▾' U\+25BE in /);
	assert.match(result.stderr, /rur: {3}print: hello/);

	const out = path.join(dir, "out");
	const tree = treeLines(out).join("\n");
	assert.match(tree, /Frame Row3 .*\[culled, not drawn: AutomaticSize, centre outside clip\]/);
	assert.match(tree, /TextLabel Wrapped .*\[\d+ wrapped lines? dropped/);
	assert.match(tree, /TextLabel Arrow .*\[missing glyph '▾' U\+25BE in /);
	assert.match(tree, /Frame "Item\.Name" /);
	// Dotted names: the quoted form and the plain one both match
	for (const filter of ['["Item.Name"].Arrow', "Item.Name.Arrow"]) {
		assert.ok(treeLines(out, { filter }).some(line => line.includes("* TextLabel Arrow")), filter);
	}
	const scene = JSON.parse(fs.readFileSync(path.join(out, "scene.json"), "utf8"));
	const paths = nodePaths(scene.nodes);
	assert.equal(paths.get(scene.nodes.find(node => node.name === "Arrow").id), 'CoreGui.RURStory.StoryTarget["Item.Name"].Arrow');
	assert.equal(paths.get(scene.nodes.find(node => node.name === "Wrapped").id), "CoreGui.RURStory.StoryTarget.Wrapped");
	const nodeJson = spawnSync(process.execPath, ["cli/inspect.mjs", "node", out, '["Item.Name"].Arrow'], { cwd: root, encoding: "utf8", windowsHide: true });
	const [arrow] = JSON.parse(nodeJson.stdout);
	assert.equal(arrow.unambiguousPath, 'CoreGui.RURStory.StoryTarget["Item.Name"].Arrow');
	assert.match(arrow.diagnostics[0], /^missing glyph/);
	fs.rmSync(dir, { recursive: true, force: true });
	console.log("CLI render diagnostics: PASS (culled rows, dropped lines, missing glyphs, script output, dotted paths)");
}

// `inspect crop`: the node the query names before nodes whose path only contains it, one inside the image
// before one outside, and a node wholly outside the image is an error naming its rect, not an empty PNG.
{
	const { PNG } = await import("pngjs");
	const os = await import("node:os");
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cli-crop-"));
	fs.writeFileSync(path.join(dir, "crop.luau"), `return function(Target)
	local Strip = Instance.new("Frame")
	Strip.Name = "Strip"
	Strip.Size = UDim2.fromOffset(400, 40)
	Strip.Parent = Target
	local List = Instance.new("UIListLayout")
	List.FillDirection = Enum.FillDirection.Horizontal
	List.SortOrder = Enum.SortOrder.LayoutOrder
	List.Parent = Strip
	for i = 14, 1, -1 do
		local Item = Instance.new("Frame")
		Item.Name = "Item" .. i
		Item.LayoutOrder = i
		Item.Size = UDim2.fromOffset(40, 40)
		Item.Parent = Strip
	end
	for _, Spec in { { "Far", 1000 }, { "Near", 100 } } do
		local Holder = Instance.new("Frame")
		Holder.Name = Spec[1]
		Holder.Position = UDim2.fromOffset(Spec[2], 100)
		Holder.Size = UDim2.fromOffset(60, 60)
		Holder.Parent = Target
		local Badge = Instance.new("Frame")
		Badge.Name = "Badge"
		Badge.Size = UDim2.fromOffset(20, 20)
		Badge.Parent = Holder
	end
end
`);
	const out = path.join(dir, "out");
	const result = spawnSync(process.execPath, ["cli/rur.mjs", "render", "--fixture", path.join(dir, "crop.luau"), "--viewport", "400x200", "--no-png", "--offline", "--json", "--out", out],
		{ cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: "1" } });
	assert.equal(result.status, 0, result.stderr);
	const scene = JSON.parse(fs.readFileSync(path.join(out, "scene.json"), "utf8"));
	const { width, height, scale = 1 } = scene.viewport;
	fs.writeFileSync(path.join(out, "render.png"), PNG.sync.write(new PNG({ width: Math.round(width * scale), height: Math.round(height * scale) })));
	// The scene lists the substring-only and off-image matches first, so taking the first match fails
	const index = name => scene.nodes.findIndex(node => String(node.path).endsWith(name));
	assert.ok(index(".Strip.Item14") < index(".Strip.Item10") && index(".Strip.Item10") < index(".Strip.Item1"));
	assert.ok(index(".Far.Badge") < index(".Near.Badge"));
	const crop = (query, file) => spawnSync(process.execPath, ["cli/inspect.mjs", "crop", out, query, "--zoom", "1", "--out", path.join(dir, file)], { cwd: root, encoding: "utf8", windowsHide: true });
	for (const [query, file, expected] of [["Item1", "item1.png", ".Strip.Item1"], ["Badge", "badge.png", ".Near.Badge"]]) {
		const picked = crop(query, file);
		assert.equal(picked.status, 0, picked.stderr);
		assert.ok(picked.stdout.split("\n")[0].endsWith(expected), picked.stdout);
		const png = PNG.sync.read(fs.readFileSync(path.join(dir, file)));
		assert.ok(png.width > 0 && png.height > 0);
	}
	const outside = crop("Item14", "item14.png");
	assert.equal(outside.status, 1, outside.stderr);
	assert.match(outside.stderr, /^CoreGui\.RURStory\.StoryTarget\.Strip\.Item14 is outside the image: rect \[520, 0, 40, 40\], image 400x200 \(logical px; render\.png \d+x\d+\)\n$/);
	assert.equal(fs.existsSync(path.join(dir, "item14.png")), false);
	fs.rmSync(dir, { recursive: true, force: true });
	console.log("CLI inspect crop: PASS (named node before substring matches, inside the image before outside, outside is an error)");
}

// --actions: a bad list is a usage error before anything runs (exit 64, no output directory);
// click / drag by node path (located when the action runs), and a path that matches nothing fails
// the render by name. --isolated / RUR_ISOLATED: the render service client neither starts nor contacts one.
{
	const { readActions, UsageError } = await import("../../cli/actions.mjs");
	const os = await import("node:os");
	const bad = (spec, pattern, options) => assert.throws(() => readActions(spec, options), error => error instanceof UsageError && pattern.test(error.message), spec);
	bad('[{"t":0.1,"type":"move","x":250,"y":250', /^--actions: not valid JSON: /);
	bad('{"type":"move","x":1,"y":1}', /^--actions: a JSON list of actions expected, not an object$/);
	bad('[{"type":"move","x":1,"y":1},{"type":"clik"}]', /^--actions\[1\] \(clik\): unknown type "clik" \(one of move, /);
	bad('[{"type":"move","x":"1","y":2}]', /^--actions\[0\] \(move\): `x` must be a number, not a string$/);
	bad('[{"type":"key"}]', /^--actions\[0\] \(key\): `key` \(a string\) is missing$/);
	bad('[{"type":"click"}]', /^--actions\[0\] \(click\): click: needs `node` \(or `target`\) or x, y$/);
	bad('[{"type":"click","node":"A","x":1,"y":1}]', /give a node or x, y, not both/);
	bad('[{"type":"click","node":"A","anchor":{"x":"0"}}]', /^--actions\[0\] \(click\): click: `anchor`: `x` must be a number/);
	bad('[{"type":"drag","from":"A"}]', /^--actions\[0\] \(drag\): drag: needs `to`/);
	bad('[{"t":0.9,"type":"drag","from":"A","to":"B"}]', /^--actions\[0\] \(drag\): ends at 1\.15 s, after the capture at --time 1; raise --time$/, { time: 1 });
	bad(path.join(os.tmpdir(), "rur-no-such-actions.json"), /no such file/);
	assert.deepEqual(readActions('[{"t":0.2,"type":"click","target":"Toggle","anchor":{"x":0.1},"offset":{"x":1,"y":0},"button":1,"hold":0.05},{"t":0.3,"type":"drag","node":"A","to":{"x":1,"y":2},"steps":3,"duration":0.1},{"frame":2,"type":"wheel","dy":100}]', { time: 1 }).length, 3);

	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cli-actions-"));
	fs.writeFileSync(path.join(dir, "toggle.luau"), `return function(Target)
	local Panel = Instance.new("Frame")
	Panel.Name = "Panel"
	Panel.Position = UDim2.fromOffset(20, 20)
	Panel.Size = UDim2.fromOffset(300, 120)
	Panel.Parent = Target
	local Toggle = Instance.new("TextButton")
	Toggle.Name = "Toggle"
	Toggle.Text = "Off"
	Toggle.Size = UDim2.fromOffset(120, 40)
	Toggle.Parent = Panel
	Toggle.Activated:Connect(function()
		Toggle.Text = if Toggle.Text == "Off" then "On" else "Off"
		-- The layout shifts after each click: the next click must still find the button
		Toggle.Position += UDim2.fromOffset(60, 30)
	end)
end
`);
	const render = (out, actions, extra = []) => spawnSync(process.execPath, ["cli/rur.mjs", "render", "--fixture", path.join(dir, "toggle.luau"), "--viewport", "400x200", "--no-png", "--offline", "--json",
		"--out", path.join(dir, out), "--actions", actions, ...extra], { cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: "1" } });

	const refused = render("bad", '[{"t":0.1,"type":"move","x":250,"y":250');
	assert.equal(refused.status, 64, refused.stderr);
	assert.equal(refused.stdout, "");
	assert.match(refused.stderr, /^rur: --actions: not valid JSON: /);
	assert.equal(fs.existsSync(path.join(dir, "bad")), false);
	const unknown = render("bad", '[{"type":"click","node":"Panel.Toggle"},{"type":"tap","x":1,"y":2}]');
	assert.equal(unknown.status, 64, unknown.stderr);
	assert.match(unknown.stderr, /^rur: --actions\[1\] \(tap\): unknown type "tap"/);
	assert.equal(fs.existsSync(path.join(dir, "bad")), false);

	const text = out => JSON.parse(fs.readFileSync(path.join(dir, out, "scene.json"), "utf8")).nodes.find(node => node.name === "Toggle")?.props?.text;
	// Three clicks, the second by a suffix path and an anchor, the third by coordinates on the moved button
	const clicked = render("clicked", JSON.stringify([
		{ t: 0.2, type: "click", node: "Panel.Toggle" },
		{ t: 0.4, type: "click", target: "StoryTarget.Panel.Toggle", anchor: { x: 0.1, y: 0.5 }, hold: 0.05 },
		{ t: 0.6, type: "click", x: 20 + 120 + 60, y: 20 + 60 + 20 },
	]));
	assert.equal(clicked.status, 0, clicked.stderr);
	assert.equal(JSON.parse(clicked.stdout).status, "ok");
	assert.equal(text("clicked"), "On");

	const missing = render("missing", '[{"t":0.2,"type":"click","node":"Panel.Toggle"},{"t":0.3,"type":"click","node":"Panel.Missing"}]');
	assert.equal(missing.status, 2, missing.stderr);
	const summary = JSON.parse(missing.stdout);
	assert.equal(summary.status, "error");
	assert.match(summary.failure, /--actions\[1\] \(click at t=0\.3\): no GUI node matches "Panel\.Missing"/);
	assert.equal(text("missing"), "On", "the rest of the timeline ran");

	// The service client with --isolated / RUR_NO_SERVICE: nothing started or contacted; render() runs in a
	// process of its own (here: fails on the missing fixture), stories() and status() answer null
	for (const env of [{ RUR_ISOLATED: "1" }, { RUR_NO_SERVICE: "1" }]) {
		const client = spawnSync(process.execPath, ["--input-type=module", "-e", `
import http from 'node:http';
const { render, stories, status } = await import('./cli/client.mjs');
http.request = () => { throw new Error('contacted a service'); };
const results = [await render({ fixture: 'x.luau' }), await stories('.'), await status()];
if (!results[0].error || /contacted a service/.test(results[0].error) || results[1] !== null || results[2] !== null) throw new Error(JSON.stringify(results));
`], { cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_ISOLATED: "", RUR_NO_SERVICE: "", ...env } });
		assert.equal(client.status, 0, client.stderr);
	}
	fs.rmSync(dir, { recursive: true, force: true });
	console.log("CLI actions: PASS (validation exits 64 before any output, click by path / suffix / anchor / coordinates after layout shifts, unknown path fails the render, --isolated service client)");
}

// A profiled render (--profile) runs in a process of its own, never through the render service, writes the
// engine's profile to <out>/profile and names it in the summary
{
	const os = await import("node:os");
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cli-profile-"));
	fs.writeFileSync(path.join(dir, "label.luau"), `return function(Target)
	local Label = Instance.new("TextLabel")
	Label.Size = UDim2.fromOffset(200, 40)
	Label.Text = "Profiled"
	Label.Parent = Target
end
`);
	const args = { fixture: path.join(dir, "label.luau"), viewport: "400x200", "no-png": true, offline: true, out: path.join(dir, "out"), profile: "work,heap,caches" };
	const result = spawnSync(process.execPath, ["--input-type=module", "-e", `
import http from 'node:http';
let contacted = false;
http.request = () => { contacted = true; throw new Error('contacted a service'); };
const { render } = await import('./cli/rur.mjs');
const summary = await render(${JSON.stringify(args)}, { service: true });
console.log(JSON.stringify({ summary, contacted }));
`], { cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_ISOLATED: "", RUR_NO_SERVICE: "" } });
	assert.equal(result.status, 0, result.stderr);
	const { summary, contacted } = JSON.parse(result.stdout.trim().split(/\r?\n/).pop());
	// (the service client falls back to a render of its own when a request fails: what counts is that none was made)
	assert.equal(contacted, false, "a profiled render never contacts the render service");
	assert.equal(summary.status, "ok");
	assert.equal(path.resolve(summary.profile), path.resolve(dir, "out", "profile"));
	const profile = JSON.parse(fs.readFileSync(path.join(summary.profile, "profile.json"), "utf8"));
	assert.equal(profile.run.command, "render");
	// (the mount is startup's, the run after it frames')
	assert.deepEqual(profile.phases, ["startup", "frames"]);
	assert.ok(profile.work.script_insns > 0, JSON.stringify(profile.work));
	assert.ok(profile.emulator.caches.some(cache => cache.name === "layout records" && cache.hits + cache.misses > 0), JSON.stringify(profile.emulator));
	assert.match(result.stderr, /heap at end: .* reachable in /);
	// the profile viewer reads it: the Caches view as text lists the layout's caches; every view as a PNG
	const viewer = spawnSync(process.execPath, [path.join("cli", "rur.mjs"), "profile", "render", summary.profile, "--view", "caches", "--out", "-"], { cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state") } });
	assert.equal(viewer.status, 0, viewer.stderr);
	assert.match(viewer.stdout, /ree's caches/);
	assert.match(viewer.stdout, /layout records/);
	const shots = path.join(dir, "views");
	const all = spawnSync(process.execPath, [path.join("cli", "rur.mjs"), "profile", "render", summary.profile, "--view", "all", "--out", shots, "--size", "100x30"], { cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state") } });
	assert.equal(all.status, 0, all.stderr);
	for (const view of ["overview", "flame", "calls", "source", "heap", "caches", "issues"]) {
		assert.ok(fs.existsSync(path.join(shots, `${view}.png`)), `${view}.png`);
	}
	assert.match(all.stdout, /!\[Overview\]\(/);
	fs.rmSync(dir, { recursive: true, force: true });
	console.log("CLI profile: PASS (render --profile runs without the render service, writes <out>/profile with the layout's caches, names it in the summary; profile render draws its views)");
}

// An entry script started through a linked folder runs: Node names the module by its real path and the
// command line's entry as written, and compared as text the two looked like different files, so the
// script ran nothing and exited 0
{
	const dir = fs.mkdtempSync(path.join(root, "out", "cli-linked-"));
	const link = path.join(dir, "linked");
	fs.symlinkSync(path.resolve(root), link, "junction");
	const env = { ...process.env, RUR_STATE_DIR: path.join(dir, "state") };
	try {
		const usage = spawnSync(process.execPath, [path.join(link, "cli", "rur.mjs")], { cwd: dir, encoding: "utf8", windowsHide: true, env });
		assert.equal(usage.status, 0, usage.stderr);
		assert.match(usage.stdout, /rur render --story/);
		const unknown = spawnSync(process.execPath, [path.join(link, "cli", "rur.mjs"), "no-such-command"], { cwd: dir, encoding: "utf8", windowsHide: true, env });
		assert.equal(unknown.status, 1, "an unknown command fails through a link too");
		const inspect = spawnSync(process.execPath, [path.join(link, "cli", "inspect.mjs"), "tree", path.join(dir, "no-such-render")], { cwd: dir, encoding: "utf8", windowsHide: true, env });
		assert.notEqual(inspect.status, 0, "inspect reports a missing render through a link too");
		// --state-dir is an option of the entry scripts under the checkout, which the link's path still names
		const stated = spawnSync(process.execPath, [path.join(link, "cli", "rur.mjs"), "--state-dir", path.join(dir, "other-state")], { cwd: dir, encoding: "utf8", windowsHide: true, env });
		assert.equal(stated.status, 0, stated.stderr);
		assert.match(stated.stdout, /rur render --story/);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
	console.log("CLI through a linked folder: PASS (rur and inspect run when started by a symlinked path, and take --state-dir)");
}
