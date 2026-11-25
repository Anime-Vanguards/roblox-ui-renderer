// Snapshot defaults must not import one project's Studio instances into another project's runtime.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
import { recordSummary, snapshotDir } from "../../cli/rur.mjs";
import { luauString } from "../../cli/studio.mjs";

// Luau string literals: quotes and backslashes escaped, control characters as Luau's \ddd (not JSON's \uXXXX)
assert.equal(luauString('a"b\\c'), '"a\\"b\\\\c"');
assert.equal(luauString("line\nnext\ttab\u0001\u007f!"), '"line\\nnext\\ttab\\001\\127!"');
assert.equal(luauString("\u00e9\u{1f600}"), '"\u00e9\u{1f600}"');
assert.equal(luauString("\ud800"), '"\ufffd"');
assert.equal(luauString(12), '"12"');

// `rur record` takes the engine's summary from a run that ended: exit 0, or exit 1 with status "error" (the engine's exit
// for a failed run, which `rur record` reports with its own exit 2); an exit 1 without that summary is a failure
const failed = { status: "error", failure: "boom" };
assert.deepEqual(recordSummary(1, `rur: record error; 31 frames\n${JSON.stringify(failed)}\n`), failed);
assert.deepEqual(recordSummary(0, `${JSON.stringify({ status: "ok" })}\r\n`), { status: "ok" });
assert.equal(recordSummary(1, `${JSON.stringify({ status: "ok" })}\n`), null);
assert.equal(recordSummary(1, ""), null);
assert.equal(recordSummary(126, `${JSON.stringify(failed)}\n`), null);
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

	// A $path to a project file, as Rojo builds it. A root that is a $path to a DataModel project, with a service
	// beside it (a code project and a separately synced world, combined by a root project), is that DataModel tree:
	// the referenced project's nodes first, their paths relative to its file, then the root's own
	const nested = path.join(dir, "nested");
	const writeProject = (file, tree) => write(path.join("nested", file), JSON.stringify({ name: path.basename(file, ".project.json"), tree }));
	write("nested/src/shared/init.luau", "return {}");
	write("nested/src/server/Main.server.luau", "print('server')");
	write("nested/world/map/Spawn.model.json", JSON.stringify({ className: "SpawnLocation" }));
	writeProject("default.project.json", { $className: "DataModel",
		ReplicatedStorage: { Shared: { $path: "src/shared" }, Remotes: { $className: "Folder" } }, ServerScriptService: { Server: { $path: "src/server" } } });
	writeProject("world/workspace.project.json", { $className: "Workspace", Map: { $path: "map" } });
	writeProject("game.project.json", { $path: "default.project.json", Workspace: { $path: "world/workspace.project.json" } });
	assert.deepEqual(rojoMounts(path.join(nested, "game.project.json")), [
		{ kind: "mount", path: "ReplicatedStorage/Shared", value: path.join(nested, "src", "shared") },
		{ kind: "instance", path: "ReplicatedStorage/Remotes", value: "Folder" },
		{ kind: "mount", path: "ServerScriptService/Server", value: path.join(nested, "src", "server") },
		{ kind: "mount", path: "Workspace/Map", value: path.join(nested, "world", "map") },
	]);
	writeProject("full.project.json", { $className: "DataModel",
		ReplicatedStorage: { Shared: { $path: "src/shared" }, Remotes: { $className: "Folder" } }, ServerScriptService: { Server: { $path: "src/server" } },
		Workspace: { $path: "world/workspace.project.json" } });
	assert.deepEqual(rojoMounts(path.join(nested, "full.project.json")), rojoMounts(path.join(nested, "game.project.json")), "the same tree written out");
	// A chain of project files is followed, and a model project at its end stays one
	write("nested/lib/inner/a.luau", "return 1");
	writeProject("lib/outer.project.json", { $path: "inner.project.json" });
	writeProject("lib/inner.project.json", { $path: "inner" });
	writeProject("chain.project.json", { $path: "lib/outer.project.json" });
	assert.deepEqual(rojoMounts(path.join(nested, "chain.project.json")), [{ kind: "mount", path: "ReplicatedStorage/chain", value: path.join(nested, "lib", "inner") }]);
	// The node's $className replaces only a Folder root
	writeProject("kit.project.json", { $className: "Folder", Handle: { $className: "Part" } });
	writeProject("tool.project.json", { $className: "Tool" });
	writeProject("classes.project.json", { $className: "DataModel", ReplicatedStorage: { Kit: { $className: "Model", $path: "kit.project.json" }, Tool: { $path: "tool.project.json" } } });
	assert.deepEqual(rojoMounts(path.join(nested, "classes.project.json")), [
		{ kind: "instance", path: "ReplicatedStorage/Kit", value: "Model" },
		{ kind: "instance", path: "ReplicatedStorage/Kit/Handle", value: "Part" },
		{ kind: "instance", path: "ReplicatedStorage/Tool", value: "Tool" },
	]);
	// Refused: a child beside a DataModel root's $path that is no service, any other mix of classes, a cycle
	writeProject("lower.project.json", { $path: "default.project.json", lower: {} });
	writeProject("clash.project.json", { $className: "DataModel", ReplicatedStorage: { Kit: { $className: "Model", $path: "tool.project.json" } } });
	writeProject("cycle/a.project.json", { $className: "DataModel", ReplicatedStorage: { B: { $path: "b/b.project.json" } } });
	writeProject("cycle/b/b.project.json", { $path: "../a.project.json" });
	const refused = {
		"lower.project.json": "lower at the top of a DataModel tree is not a service",
		"clash.project.json": `Kit has "$className": "Model" and a $path to tool.project.json, whose tree is a Tool: Rojo takes both only when the $path is a Folder`,
		"cycle/a.project.json": `the project files' $paths form a cycle (${["a.project.json", path.join("b", "b.project.json"), "a.project.json"].join(" -> ")}): a project can't include itself`,
	};
	for (const [file, reason] of Object.entries(refused)) assert.throws(() => rojoMounts(path.join(nested, file)), { message: `${path.join(nested, file)}: ${reason}` });

	// A Wally package's own default.project.json (not a DataModel) gives way to the one DataModel project beside it that
	// mounts the story (native/src/rojo.rs project_for_source); with two, it stays and the line names them
	if (!process.env.RUR_PROJECT) {
		const { detectRojoProject, isDataModel } = await import("../../cli/rojo.mjs");
		const tree = (name, value) => write(`package/${name}`, JSON.stringify(value));
		const dev = { $className: "DataModel", ReplicatedStorage: { Packages: { $path: "Packages", Kit: { $path: "src" } } } };
		tree("default.project.json", { name: "Kit", tree: { $path: "src" } });
		tree("dev.project.json", { name: "dev", tree: dev });
		write("package/Packages/Signal.lua", "return {}");
		write("package/src/Components/Thing.story.lua", "return {}");
		const packageDir = fs.realpathSync.native(path.join(dir, "package"));
		const story = path.join(packageDir, "src", "Components", "Thing.story.lua");
		const args = { story };
		const note = detectRojoProject(args, path.join(dir, "elsewhere"));
		assert.equal(args.rojo, path.join(packageDir, "dev.project.json"));
		assert.equal(note, `rur: Rojo project ${args.rojo} (found beside the story; default.project.json beside it is the package's own tree, not a DataModel, and this is the one DataModel project there that mounts Thing.story.lua; --rojo picks another): mounted ReplicatedStorage.Packages, ReplicatedStorage.Packages.Kit`);
		tree("play.project.json", { name: "play", tree: dev });
		const both = { story };
		assert.match(detectRojoProject(both, path.join(dir, "elsewhere")), /several DataModel projects beside it mount Thing\.story\.lua \(dev\.project\.json, play\.project\.json\); --rojo picks another\): mounted ReplicatedStorage\.Kit$/);
		assert.equal(both.rojo, path.join(packageDir, "default.project.json"));
		assert.ok(isDataModel(path.join(packageDir, "dev.project.json")) && !isDataModel(path.join(packageDir, "default.project.json")));
	}

	// native/src/rojo.rs gives the same mounts as cli/rojo.mjs (`rur-native rojo-mounts`), in the same order, and refuses
	// the same projects with the same reasons
	{
		const { ensureEngine } = await import("../../cli/engine.mjs");
		const engine = await ensureEngine();
		const example = path.join(root, "examples", "rojo-wally", "default.project.json");
		const projects = [[path.join(dir, "plugin.project.json")], [path.join(dir, "plugin.project.json"), "ServerStorage/P"], [path.join(dir, "game.project.json")], [example],
			...["game", "full", "chain", "classes"].map(name => [path.join(nested, `${name}.project.json`)])];
		for (const [file, at] of projects) {
			const result = spawnSync(engine, ["rojo-mounts", file, ...(at ? ["--rojo-at", at] : [])], { encoding: "utf8", windowsHide: true });
			assert.equal(result.status, 0, result.stderr);
			assert.deepEqual(JSON.parse(result.stdout), rojoMounts(file, { at }), `${path.basename(file)} ${at || ""}`);
		}
		for (const [file, reason] of Object.entries(refused)) {
			const result = spawnSync(engine, ["rojo-mounts", path.join(nested, file)], { encoding: "utf8", windowsHide: true });
			assert.equal(result.status, 1, result.stderr);
			assert.equal(result.stderr.trim(), `rur-native rojo-mounts: ${path.join(nested, file)}: ${reason}`);
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

		// `rur diff` (rur-native diff): the committed story before, the edited one after, the label's change named and its
		// pixels counted; a folder git ignores that the project maps (an installed package) comes from the working tree
		const cli =(...extra) => spawnSync(process.execPath, ["cli/rur.mjs", ...extra],
			{ cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: "1" } });
		const diffed = cli("diff", "--git-ref", "HEAD", "--rojo", path.join(dir, "plugin.project.json"), "--story", path.join(dir, "stories/App.story.luau"),
			"--viewport", "600x450@physical", "--offline", "--json", "--out", path.join(dir, "out/diff"));
		assert.equal(diffed.status, 0, diffed.stderr);
		assert.equal(diffed.stdout.trim().split("\n").length, 1);
		const result = JSON.parse(diffed.stdout);
		assert.deepEqual([result.before.status, result.after.status], ["ok", "ok"]);
		assert.ok(result.before.gitRef.commits[0].dir.startsWith(path.join(dir, "state", "git-refs")), JSON.stringify(result.before.gitRef));
		assert.ok(result.changes.some(change => change.startsWith("~ ") && change.includes('text="Story"') && change.includes('text="Edited"')), result.changes.join("\n"));
		assert.ok(result.pixels.changed > 0 && result.pixels.changed < result.pixels.total, result.pixels.message);
		assert.ok(fs.existsSync(path.join(dir, "out/diff/diff.png")) && fs.existsSync(path.join(dir, "out/diff/before/summary.json")));
		write(".gitignore", "Packages/\n");
		write("packages.project.json", JSON.stringify({ name: "Packages", tree: { $className: "DataModel",
			ReplicatedStorage: { MyPlugin: { $path: "src" }, Packages: { $path: "Packages" } } } }));
		write("stories/Lib.story.luau", `local Lib = require(game:GetService("ReplicatedStorage").Packages.Lib)
return function(Target) local Label = Instance.new("TextLabel"); Label.Name = "Title"; Label.Text = Lib; Label.Parent = Target end
`);
		assert.equal(git("add", "-A").status, 0);
		assert.equal(git("commit", "-qm", "packages").status, 0);
		write("Packages/Lib.luau", 'return "installed"');
		const packages = cli("diff", "--git-ref", "HEAD", "--rojo", path.join(dir, "packages.project.json"), "--story", path.join(dir, "stories/Lib.story.luau"),
			"--no-png", "--offline", "--json", "--out", path.join(dir, "out/packages"));
		assert.equal(packages.status, 0, packages.stderr);
		assert.match(packages.stderr, /Packages does not exist at [0-9a-f]{10}; using the working tree's/);
		assert.equal(JSON.parse(packages.stdout).before.status, "ok");

		// `rur sweep` (rur-native sweep): the stories beside the Rojo project, each in a process of its own, reported (the
		// filter leaves out the copy --git-ref extracted into the state dir, which is in the project's folder here); every
		// story is clean, so it exits 0
		const swept =cli("sweep", "--rojo", path.join(dir, "plugin.project.json"), "--filter", path.join(dir, "stories", "App.story"), "--no-png", "--offline", "--jobs", "2", "--out", path.join(dir, "out/sweep"));
		assert.equal(swept.status, 0, swept.stderr);
		assert.equal(JSON.parse(swept.stdout).stories, 1);
		const report = JSON.parse(fs.readFileSync(path.join(dir, "out/sweep/report.json"), "utf8"));
		assert.deepEqual([report.records.length, report.records[0].status, report.records[0].clean], [1, "ok", true]);
		assert.ok(report.records[0].story.endsWith("stories/App.story.luau") && fs.existsSync(path.join(dir, "out/sweep", report.records[0].out, "summary.json")));
		assert.match(fs.readFileSync(path.join(dir, "out/sweep/summary.md"), "utf8"), /^# Story sweep\n/);

		// A story that fails makes the sweep exit 1, with the headline on stdout and report.json, summary.md and every
		// story's output written as for a clean sweep: the exit code agrees with the report's clean count, and `rur sweep`
		// hands on rur-native's code
		write("stories/Broken.story.luau", `return function(Target)
	local Label = Instance.new("TextLabel")
	Label.Name = "Partial"
	Label.Parent = Target
	error("broken on purpose")
end
`);
		const unclean = cli("sweep", "--rojo", path.join(dir, "plugin.project.json"), "--filter", `${path.join(dir, "stories", "App.story")}|${path.join(dir, "stories", "Broken.story")}`,
			"--no-png", "--offline", "--jobs", "2", "--out", path.join(dir, "out/sweep-unclean"));
		assert.equal(unclean.status, 1, unclean.stderr);
		const headline = JSON.parse(unclean.stdout);
		assert.deepEqual([headline.stories, headline.clean], [2, 1], unclean.stdout);
		const uncleanReport = JSON.parse(fs.readFileSync(path.join(dir, "out/sweep-unclean/report.json"), "utf8"));
		assert.deepEqual([uncleanReport.totals.stories, uncleanReport.totals.clean], [2, 1]);
		assert.deepEqual(uncleanReport.records.map(record => [path.basename(record.story), record.clean]), [["App.story.luau", true], ["Broken.story.luau", false]]);
		assert.match(uncleanReport.records[1].failure || uncleanReport.records[1].errors.join("\n"), /broken on purpose/, JSON.stringify(uncleanReport.records[1]));
		assert.ok(uncleanReport.records.every(record => fs.existsSync(path.join(dir, "out/sweep-unclean", record.out, "summary.json"))));
		assert.match(fs.readFileSync(path.join(dir, "out/sweep-unclean/summary.md"), "utf8"), /^# Story sweep\n/);
	}
	fs.rmSync(dir, { recursive: true, force: true });
	console.log("CLI project options: PASS (rojo, $path to project files at and below the root, native rojo parity, outside stories, fixture controls, @physical, --json, --git-ref, diff, sweep and its exit code, --plugin, tree ancestry)");
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

// A button whose font has no face to draw with (no Studio fonts: here a frozen font set with nothing in it, as
// RUR_FONT_ROOT names one) draws no text, and the render says so instead of reporting nothing missing: the family
// in summary.missingFonts (the scene's diagnostics.unavailableFonts), a "missing fonts" line on stderr and the
// button's note in `inspect`. The status stays ok, as with missing glyphs.
noFonts: {
	const { treeLines } = await import("../../cli/inspect.mjs");
	const os = await import("node:os");
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cli-nofonts-"));
	fs.mkdirSync(path.join(dir, "fonts"));
	fs.writeFileSync(path.join(dir, "button.luau"), `return function(Target)
	local Button = Instance.new("TextButton")
	Button.Name = "Play"
	Button.Size = UDim2.fromOffset(200, 50)
	Button.Text = "Play"
	Button.Parent = Target
end
`);
	const out = path.join(dir, "out");
	const result = spawnSync(process.execPath, ["cli/rur.mjs", "render", "--fixture", path.join(dir, "button.luau"), "--viewport", "400x200", "--no-png", "--offline", "--json", "--out", out],
		{ cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: "1", RUR_FONT_ROOT: path.join(dir, "fonts") } });
	assert.equal(result.status, 0, result.stderr);
	const summary = JSON.parse(result.stdout);
	assert.equal(summary.status, "ok");
	const scene = JSON.parse(fs.readFileSync(path.join(out, "scene.json"), "utf8"));
	const unavailable = scene.diagnostics.unavailableFonts;
	assert.ok(Array.isArray(unavailable) && unavailable.length > 0, JSON.stringify(scene.diagnostics));
	for (const family of unavailable) assert.ok(summary.missingFonts.includes(family), `${family} in ${JSON.stringify(summary.missingFonts)}`);
	assert.deepEqual(JSON.parse(fs.readFileSync(path.join(out, "summary.json"), "utf8")).missingFonts, summary.missingFonts);
	assert.match(result.stderr, /rur: missing fonts: .*rbxasset:\/\/fonts\/families\//);
	assert.match(treeLines(out).join("\n"), /TextButton Play .*\[text not drawn: no face of its font/);
	fs.rmSync(dir, { recursive: true, force: true });
	console.log("CLI render without fonts: PASS (the button's font in missingFonts, on stderr and in its note)");
}

// A rounded bar wider than a texture can be (9255 x 5 px under a UICorner, as a progress bar on a wide screen) is drawn
// where it shows. A render that draws no picture (here a viewport wider than a texture) fails: exit 2, status "error",
// the drawing error as its failure and on stderr, no png in its summary, and no render.png (an earlier one is removed).
{
	const { PNG } = await import("pngjs");
	const os = await import("node:os");
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cli-wide-"));
	fs.writeFileSync(path.join(dir, "bar.luau"), `return function(Target)
	local Bar = Instance.new("Frame")
	Bar.Name = "Bar"
	Bar.Position = UDim2.fromOffset(-200, 100)
	Bar.Size = UDim2.fromOffset(9255, 5)
	Bar.BackgroundColor3 = Color3.new(0, 0, 0)
	Bar.BackgroundTransparency = 0.45
	Bar.Parent = Target
	local Corner = Instance.new("UICorner")
	Corner.CornerRadius = UDim.new(0, 2.5)
	Corner.Parent = Bar
end
`);
	const out = path.join(dir, "out");
	const render = viewport => spawnSync(process.execPath, ["cli/rur.mjs", "render", "--fixture", path.join(dir, "bar.luau"), "--viewport", viewport, "--scale", "1", "--offline", "--json", "--out", out],
		{ cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: "1" } });
	const drawn = render("400x200");
	assert.equal(drawn.status, 0, drawn.stderr);
	const summary = JSON.parse(drawn.stdout);
	assert.equal(summary.status, "ok", summary.failure);
	assert.deepEqual(summary.renderErrors, []);
	assert.equal(summary.png, path.join(out, "render.png"));
	const png = PNG.sync.read(fs.readFileSync(summary.png));
	const pixel = (x, y) => [...png.data.subarray((y * png.width + x) * 4, (y * png.width + x) * 4 + 4)].join();
	// Rows 100 to 104 are the bar's across the picture, to its right edge; the rows beside it the background's
	for (const x of [0, 200, 399]) {
		assert.notEqual(pixel(x, 102), pixel(x, 50), `the bar at ${x}, 102`);
		assert.equal(pixel(x, 106), pixel(x, 50), `the background at ${x}, 106`);
	}

	const failed = render("9000x10");
	assert.equal(failed.status, 2, failed.stderr);
	const failure = JSON.parse(failed.stdout);
	assert.equal(failure.status, "error");
	assert.match(failure.failure, /^drawing failed: .*Invalid texture extent 9000x10/);
	assert.match(failure.renderErrors.join("\n"), /Invalid texture extent 9000x10/);
	assert.equal(failure.png, null);
	assert.deepEqual(JSON.parse(fs.readFileSync(path.join(out, "summary.json"), "utf8")).png, null);
	assert.ok(!fs.existsSync(path.join(out, "render.png")), "the earlier render.png is gone");
	assert.match(failed.stderr, /rur: drawing failed, no render\.png: .*Invalid texture extent 9000x10/);
	fs.rmSync(dir, { recursive: true, force: true });
	console.log("CLI wide rounded bar: PASS (drawn where it shows; a render without a picture exits 2 with status error and no png)");
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
	bad('[{"type":"wheel","node":"A","x":1,"y":1,"notches":1}]', /^--actions\[0\] \(wheel\): wheel: give a node or x, y, not both/);
	bad('[{"type":"wheel","x":1,"y":1,"offset":{"x":1},"notches":1}]', /^--actions\[0\] \(wheel\): wheel: `offset` needs a `node`$/);
	bad('[{"type":"wheel","target":5,"notches":1}]', /^--actions\[0\] \(wheel\): wheel: `target` must be a node path string, not a number$/);
	bad('[{"type":"wheel","node":"A"}]', /^--actions\[0\] \(wheel\): wheel: needs `notches`/);
	bad(path.join(os.tmpdir(), "rur-no-such-actions.json"), /no such file/);
	assert.deepEqual(readActions('[{"t":0.2,"type":"click","target":"Toggle","anchor":{"x":0.1},"offset":{"x":1,"y":0},"button":1,"hold":0.05},{"t":0.3,"type":"drag","node":"A","to":{"x":1,"y":2},"steps":3,"duration":0.1},{"frame":2,"type":"wheel","dy":100},{"type":"wheel","node":"Panel","anchor":{"y":0.2},"offset":{"x":-4},"notches":-1}]', { time: 1 }).length, 4);

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

// RUR_NATIVE_ENGINE naming a missing file (or a directory) is an error naming the path and what to do,
// never a fallback into a cold build.
{
	const os = await import("node:os");
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cli-missing-engine-"));
	const missingPath = path.join(dir, "no-such-rur-native.exe");
	const stateDir = path.join(dir, "state");
	const env = { ...process.env, RUR_NATIVE_ENGINE: missingPath, RUR_STATE_DIR: stateDir, RUR_NO_SERVICE: "1" };
	try {
		// Direct findEngine() / ensureEngine() check
		const checkMod = spawnSync(process.execPath, ["--input-type=module", "-e", `
import assert from "node:assert/strict";
import { findEngine, ensureEngine } from "./cli/engine.mjs";
assert.throws(() => findEngine(), err => err.message.includes(${JSON.stringify(missingPath)}) && /unset RUR_NATIVE_ENGINE/i.test(err.message));
await assert.rejects(() => ensureEngine(), err => err.message.includes(${JSON.stringify(missingPath)}) && /unset RUR_NATIVE_ENGINE/i.test(err.message));
`], { cwd: root, encoding: "utf8", windowsHide: true, env });
		assert.equal(checkMod.status, 0, checkMod.stderr);

		// A directory path is also refused as not a file
		const checkDir = spawnSync(process.execPath, ["--input-type=module", "-e", `
import assert from "node:assert/strict";
import { findEngine } from "./cli/engine.mjs";
assert.throws(() => findEngine(), err => err.message.includes(${JSON.stringify(dir)}) && /not a file/i.test(err.message));
`], { cwd: root, encoding: "utf8", windowsHide: true, env: { ...env, RUR_NATIVE_ENGINE: dir } });
		assert.equal(checkDir.status, 0, checkDir.stderr);

		// End-to-end CLI invocation refuses and never starts a build into RUR_STATE_DIR
		const cliRes = spawnSync(process.execPath, ["cli/rur.mjs", "cost", "--help"], { cwd: root, encoding: "utf8", windowsHide: true, env });
		assert.notEqual(cliRes.status, 0, "rur with missing RUR_NATIVE_ENGINE must exit non-zero");
		assert.ok(cliRes.stderr.includes(missingPath), `expected missing path in stderr, got: ${cliRes.stderr}`);
		assert.match(cliRes.stderr, /unset RUR_NATIVE_ENGINE/i);
		assert.equal(fs.existsSync(path.join(stateDir, "native-app")), false, "missing RUR_NATIVE_ENGINE must never trigger a native-app build");

		// `rur doctor` still runs and reports the missing RUR_NATIVE_ENGINE
		const docRes = spawnSync(process.execPath, ["cli/rur.mjs", "doctor"], { cwd: root, encoding: "utf8", windowsHide: true, env });
		assert.equal(docRes.status, 0, docRes.stderr);
		const doc = JSON.parse(docRes.stdout);
		assert.ok(doc.engine.includes(missingPath) && doc.engine.includes("missing"), doc.engine);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
	console.log("CLI missing RUR_NATIVE_ENGINE: PASS (errors with path and remedy, never starts a cold build)");
}

// `rur cost`, `rur cost compare`, and `rur sequence --cost`: deterministic Roblox UI cost pass, Studio CanvasGroup RT
// bytes, calibrated/refused timings, multi-frame sequence layout churn, and A/B diff.
{
	const os = await import("node:os");
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cost-cli-"));
	const env = { ...process.env, RUR_NO_SERVICE: "1" };
	const runRur = (...argv) => spawnSync(process.execPath, ["cli/rur.mjs", ...argv], { cwd: root, encoding: "utf8", windowsHide: true, env });
	try {
		const help = runRur("cost", "--help");
		assert.equal(help.status, 0, help.stderr);
		assert.match(help.stdout, /rur cost/);

		// Single-moment CanvasGroup fixture: exact Studio rectangular RT bytes (723,600 B, not square 1,116,000 B)
		// and bit-identical output across repeats
		const outA = path.join(dir, "cg-a");
		const outB = path.join(dir, "cg-b");
		const rA = runRur("cost", "--fixture", path.join(root, "fixtures", "canvasgroup.luau"), "--no-snapshots", "--json", "--out", outA);
		assert.equal(rA.status, 0, rA.stderr);
		const rB = runRur("cost", "--fixture", path.join(root, "fixtures", "canvasgroup.luau"), "--no-snapshots", "--json", "--out", outB);
		assert.equal(rB.status, 0, rB.stderr);
		const rawA = fs.readFileSync(path.join(outA, "cost.json"), "utf8");
		const rawB = fs.readFileSync(path.join(outB, "cost.json"), "utf8");
		assert.equal(rawA, rawB, "cost.json must be bit-identical across runs");
		const costA = JSON.parse(rawA);
		assert.equal(costA.schema, "rur-cost/1");
		assert.equal(costA.totals.canvasGroupTargets, 7);
		assert.equal(costA.totals.canvasRtBytes, 723600);
		assert.equal(costA.totals.glyphQuads, 35, "all 7 CanvasGroups render their 5-glyph Group label into their own textures");
		assert.equal(costA.totals.batchBreaksByCause.clip, 0, "CanvasGroup descendants (including overhanging nested groups) must not add spurious ancestor-clip breaks");
		assert.match(costA.fidelity.drawCalls, /unmeasured/i);
		assert.match(costA.fidelity.batchBreaks, /unmeasured/i);
		assert.doesNotMatch(costA.fidelity.batchBreaks, /roblox/i);
		assert.equal(costA.predicted["desktop-ref"].layoutCpuMs.status, "refused");
		assert.equal(costA.predicted["desktop-ref"].layoutCpuMs.ms, null);
		assert.equal(costA.predicted["desktop-ref"].fullTreeLayoutCpuMs.basis, "extrapolated");
		assert.equal(costA.predicted["phone-mid"].fullTreeLayoutCpuMs.basis, "anchored_extrapolation");
		assert.equal(costA.predicted["phone-mid"].drawPassCpuMs.status, "refused");
		assert.equal(costA.predicted["phone-mid"].drawPassCpuMs.ms, null);
		assert.equal(costA.predicted["phone-mid"].gpuUiMs.status, "refused");
		assert.equal(costA.predicted["phone-mid"].gpuUiMs.ms, null);

		// `rur cost compare`: identical reports -> unchanged; equivalence gate refuses incomparable reports
		const sameCmp = runRur("cost", "compare", outA, outB, "--json");
		assert.equal(sameCmp.status, 0, sameCmp.stderr);
		const sameJson = JSON.parse(sameCmp.stdout);
		assert.equal(sameJson.verdict, "unchanged");
		assert.equal(sameJson.equivalence.passed, true);

		// Non-cost file (scene.json) passed to compare -> non-zero error identifying scene.json
		const scenePath = path.join(dir, "cg.scene.json");
		const rEng = runRur("render", "--fixture", path.join(root, "fixtures", "canvasgroup.luau"), "--no-snapshots", "--no-png", "--out", path.join(dir, "cg-render"));
		assert.equal(rEng.status, 0, rEng.stderr);
		fs.copyFileSync(path.join(dir, "cg-render", "scene.json"), scenePath);
		const badSceneCmp = runRur("cost", "compare", scenePath, outA);
		assert.notEqual(badSceneCmp.status, 0, "scene.json must be rejected by cost compare");
		assert.match(badSceneCmp.stderr, /scene\.json/i);

		// Live vs --scene -> refused (non-zero)
		const outSceneCost = path.join(dir, "cg-scene-cost");
		const rSceneCost = runRur("cost", "--scene", scenePath, "--out", outSceneCost);
		assert.equal(rSceneCost.status, 0, rSceneCost.stderr);
		const liveVsScene = runRur("cost", "compare", outA, outSceneCost, "--json");
		assert.notEqual(liveVsScene.status, 0, "live vs --scene must be refused");
		assert.equal(JSON.parse(liveVsScene.stdout).verdict, "refused");
		assert.match(liveVsScene.stderr, /hiddenInShown\.complete|--scene/i);

		// Different viewport -> refused (non-zero)
		const outSmallVp = path.join(dir, "cg-small-vp");
		const rSmallVp = runRur("cost", "--fixture", path.join(root, "fixtures", "canvasgroup.luau"), "--no-snapshots", "--viewport", "800x400", "--out", outSmallVp);
		assert.equal(rSmallVp.status, 0, rSmallVp.stderr);
		const vpMismatch = runRur("cost", "compare", outA, outSmallVp, "--json");
		assert.notEqual(vpMismatch.status, 0, "viewport mismatch must be refused");
		assert.equal(JSON.parse(vpMismatch.stdout).verdict, "refused");
		assert.match(vpMismatch.stderr, /viewport/i);

		// Sequence cost mode: static fixture incurs 0 layout churn on frames 1..N
		const outSeq = path.join(dir, "seq-cost");
		const rSeq = runRur("cost", "--fixture", path.join(root, "fixtures", "corners.luau"), "--no-snapshots", "--from", "0", "--frames", "4", "--fps", "60", "--json", "--out", outSeq);
		assert.equal(rSeq.status, 0, rSeq.stderr);
		const seqCost = JSON.parse(fs.readFileSync(path.join(outSeq, "cost.json"), "utf8"));
		assert.equal(seqCost.mode, "sequence");
		assert.equal(seqCost.sequence.frames.length, 4);
		assert.ok(seqCost.sequence.frames[0].layout.passes >= 1);
		for (const frame of seqCost.sequence.frames.slice(1)) {
			assert.equal(frame.layout.passes, 0);
			assert.equal(frame.layout.stored + frame.layout.moved, 0);
		}

		// Single vs sequence -> refused (non-zero)
		const singleVsSeq = runRur("cost", "compare", outA, outSeq, "--json");
		assert.notEqual(singleVsSeq.status, 0, "single vs sequence must be refused");
		assert.equal(JSON.parse(singleVsSeq.stdout).verdict, "refused");
		assert.match(singleVsSeq.stderr, /mode/i);

		// `rur sequence --cost --no-png --no-video`: writes cost.json alongside trace.json (edit mode -> refuses per-frame layoutCpuMs)
		const outSeqFlag = path.join(dir, "seq-with-cost");
		const rSeqFlag = runRur("sequence", "--fixture", path.join(root, "fixtures", "corners.luau"), "--no-snapshots", "--from", "0", "--frames", "3", "--fps", "60", "--no-png", "--no-video", "--cost", "--json", "--out", outSeqFlag);
		assert.equal(rSeqFlag.status, 0, rSeqFlag.stderr);
		const seqSummary = JSON.parse(rSeqFlag.stdout);
		assert.ok(seqSummary.cost && fs.existsSync(seqSummary.cost), "sequence --cost writes cost.json");
		const seqFlagCost = JSON.parse(fs.readFileSync(seqSummary.cost, "utf8"));
		assert.equal(seqFlagCost.sequence.frames.length, 3);
		assert.equal(seqFlagCost.runMode, "edit");
		assert.equal(seqFlagCost.predicted["desktop-ref"].layoutCpuMs.status, "refused");
		assert.equal(seqFlagCost.predicted["desktop-ref"].layoutCpuMs.ms, null);

		// Counter-driven Heartbeat scroll + AbsolutePosition read: default sequence runs in play mode (1 beat/frame),
		// explicit --mode edit refuses per-frame layoutCpuMs, and compare refuses play vs edit.
		const counterScroll = path.join(dir, "counter-scroll.luau");
		fs.writeFileSync(counterScroll, `return function(Target)
	local RunService = game:GetService("RunService")
	local Scroller = Instance.new("ScrollingFrame")
	Scroller.Size = UDim2.fromOffset(600, 400)
	Scroller.CanvasSize = UDim2.fromOffset(0, 4000)
	Scroller.Parent = Target
	local Cards = {}
	for i = 1, 60 do
		local Card = Instance.new("Frame")
		Card.Position = UDim2.fromOffset(0, (i - 1) * 50)
		Card.Size = UDim2.fromOffset(200, 40)
		Card.Parent = Scroller
		local Inner = Instance.new("Frame")
		Inner.Size = UDim2.fromScale(1, 1)
		Inner.Parent = Card
		Cards[i] = Card
	end
	local step = 0
	RunService.Heartbeat:Connect(function()
		step += 1
		Scroller.CanvasPosition = Vector2.new(0, (step * 10) % 2000)
		local _ = Cards[1].AbsolutePosition
	end)
end
`);
		const outPlay = path.join(dir, "counter-play");
		const outEdit = path.join(dir, "counter-edit");
		const rPlay = runRur("cost", "--fixture", counterScroll, "--no-snapshots", "--viewport", "800x600", "--scale", "1", "--from", "0.5", "--frames", "5", "--fps", "60", "--json", "--out", outPlay);
		assert.equal(rPlay.status, 0, rPlay.stderr);
		assert.match(rPlay.stderr, /mode=play/);
		const cPlay = JSON.parse(fs.readFileSync(path.join(outPlay, "cost.json"), "utf8"));
		assert.equal(cPlay.runMode, "play");
		assert.equal(cPlay.layout.scrollPasses, 1);
		assert.equal(cPlay.layout.scrollMoved, 120);
		assert.ok(Math.abs(cPlay.predicted["desktop-ref"].layoutCpuMs.ms - 0.09944) < 1e-9, `expected 0.09944 ms, got ${cPlay.predicted["desktop-ref"].layoutCpuMs.ms}`);

		const rEdit = runRur("cost", "--fixture", counterScroll, "--no-snapshots", "--viewport", "800x600", "--scale", "1", "--from", "0.5", "--frames", "5", "--fps", "60", "--mode", "edit", "--json", "--out", outEdit);
		assert.equal(rEdit.status, 0, rEdit.stderr);
		assert.match(rEdit.stderr, /mode=edit/);
		assert.match(rEdit.stderr, /--mode play/);
		const cEdit = JSON.parse(fs.readFileSync(path.join(outEdit, "cost.json"), "utf8"));
		assert.equal(cEdit.runMode, "edit");
		assert.equal(cEdit.predicted["desktop-ref"].layoutCpuMs.status, "refused");
		assert.equal(cEdit.predicted["desktop-ref"].layoutCpuMs.ms, null);
		assert.equal(cEdit.sequence.summary.steadyStateLayoutCpuMs.status, "refused");
		assert.equal(cEdit.sequence.summary.steadyStateLayoutCpuMs["desktop-ref"], null);

		const playVsEdit = runRur("cost", "compare", outPlay, outEdit, "--json");
		assert.notEqual(playVsEdit.status, 0, "play vs edit comparison must be refused");
		assert.equal(JSON.parse(playVsEdit.stdout).verdict, "refused");
		assert.match(playVsEdit.stderr, /runMode/i);
		assert.match(playVsEdit.stderr, /--mode/i);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
	console.log("CLI cost: PASS (rur cost, compare, sequence churn, and sequence --cost)");
}
