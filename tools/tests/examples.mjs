// The example project (examples/project, docs/projects.md) mounts through its rur.json places and its
// stories run and lay out: what the README's quick start renders.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { engineBaseArgs, ensureEngine } from "../../cli/engine.mjs";
import { placeOf, placeRoots, readPlaces } from "../../cli/places.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const project = path.join(root, "examples", "project");
assert.deepEqual(readPlaces(project).map(place => place.name), ["main"]);
const engine = await ensureEngine();
const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rur-examples-test-"));
try {
	const render = (name, controls) => {
		const story = path.join(project, "src", "ReplicatedStorage", "Stories", `${name}.story.luau`);
		assert.equal(placeOf(project, story), "main");
		const out = path.join(directory, `${name}${controls ? "-controls" : ""}.json`);
		const result = spawnSync(engine, [...engineBaseArgs(), "--story", story, "--roots", placeRoots(project, "main").join(";"),
			"--viewport", "800x400", "--time", "1", "--no-snapshots", ...(controls ? ["--controls", JSON.stringify(controls)] : []),
			"--out", out], { encoding: "utf8", windowsHide: true });
		assert.equal(result.status, 0, result.stderr);
		const scene = JSON.parse(fs.readFileSync(out, "utf8"));
		assert.equal(scene.status, "ok", `${name}: ${scene.failure}`);
		return scene;
	};
	for (const [name, expected] of [["Button", ["Stack", "Primary", "Secondary"]], ["Card", ["Card", "Title", "Body"]], ["Badge", ["Badge"]]]) {
		const names = render(name).nodes.map(node => String(node.path).split(".").pop());
		for (const want of expected) assert.ok(names.includes(want), `${name}: no ${want} node in ${names.join(", ")}`);
	}
	// The generic (render) story gets its controls, --controls included, in a group too
	const badge = scene => scene.nodes.find(node => String(node.path).endsWith(".Badge"));
	const plain = badge(render("Badge")), changed = badge(render("Badge", { "Label": "Limited time sale", "Style.Wide": true }));
	assert.equal(changed.rect[2], plain.rect[2] * 2, "Style.Wide doubles the badge");
	assert.ok(changed.text.width > plain.text.width * 2, `the label changes (${plain.text.width} -> ${changed.text.width})`);
	console.log("Example project: its place, and the Button, Card and Badge stories mount and lay out (Badge with --controls)");

	// The Rojo + Wally example (examples/rojo-wally): its story requires ReplicatedStorage.Packages.TinyFusion, which only
	// the Rojo project mounts. `rur-native render` takes --rojo, finds default.project.json with no project options, and
	// refuses an option it does not know; the Node CLI does the same through cli/rojo.mjs.
	const rojoProject = path.join(root, "examples", "rojo-wally");
	const smoke = path.join("src", "shared", "Stories", "Tests", "SmokeTest.story.luau");
	const checkSmoke = (dir, what) => {
		const scene = JSON.parse(fs.readFileSync(path.join(dir, "scene.json"), "utf8"));
		const errors = (Array.isArray(scene.diagnostics.output) ? scene.diagnostics.output : []).filter(entry => entry.kind === "error");
		assert.equal(scene.status, "ok", `${what}: ${scene.failure}`);
		assert.deepEqual(errors, [], what);
		const names = scene.nodes.map(node => String(node.path).split(".").pop());
		for (const want of ["Panel", "Title", "Count"]) assert.ok(names.includes(want), `${what}: no ${want} node in ${names.join(", ")}`);
	};
	const native = (what, ...args) => {
		const dir = path.join(directory, what);
		const result = spawnSync(engine, ["render", ...args, "--story", smoke, "--viewport", "800x450", "--no-png", "--offline", "--dir", dir], { cwd: rojoProject, encoding: "utf8", windowsHide: true });
		return { result, dir };
	};
	const named = native("rojo", "--rojo", "default.project.json");
	assert.equal(named.result.status, 0, named.result.stderr);
	assert.match(named.result.stderr, /rur: Rojo project .*default\.project\.json: mounted ReplicatedStorage\/Packages, ReplicatedStorage\/Shared/);
	checkSmoke(named.dir, "rur-native render --rojo");
	if (!process.env.RUR_PROJECT) {
		const found = native("found");
		assert.equal(found.result.status, 0, found.result.stderr);
		assert.match(found.result.stderr, /found beside the story/);
		checkSmoke(found.dir, "rur-native render (default.project.json found)");
		const viaNode = spawnSync(process.execPath, [path.join(root, "cli", "rur.mjs"), "render", "--story", smoke, "--viewport", "800x450", "--no-png", "--offline", "--json", "--out", path.join(directory, "node-found")],
			{ cwd: rojoProject, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_NO_SERVICE: "1" } });
		assert.equal(viaNode.status, 0, viaNode.stderr);
		checkSmoke(path.join(directory, "node-found"), "rur render (default.project.json found)");
	}
	const viaNodeRojo = spawnSync(process.execPath, [path.join(root, "cli", "rur.mjs"), "render", "--rojo", path.join(rojoProject, "default.project.json"), "--story", path.join(rojoProject, smoke), "--viewport", "800x450", "--no-png", "--offline", "--json", "--out", path.join(directory, "node-rojo")],
		{ cwd: root, encoding: "utf8", windowsHide: true, env: { ...process.env, RUR_NO_SERVICE: "1" } });
	assert.equal(viaNodeRojo.status, 0, viaNodeRojo.stderr);
	checkSmoke(path.join(directory, "node-rojo"), "rur render --rojo");
	// An option rur-native does not know stops the command (it used to be skipped, with its value)
	const typo = native("typo", "--rojoo", "default.project.json");
	assert.equal(typo.result.status, 2, typo.result.stderr);
	assert.match(typo.result.stderr, /unknown option --rojoo/);
	// Without the project the failure says what was mounted and how to mount the package
	const missing = native("missing", "--mount", "ReplicatedStorage/Shared=src/shared");
	assert.equal(missing.result.status, 0, missing.result.stderr);
	const missingScene = JSON.parse(fs.readFileSync(path.join(missing.dir, "scene.json"), "utf8"));
	assert.equal(missingScene.status, "error");
	assert.match(missingScene.diagnostics.hint, /^ReplicatedStorage\.Packages was not mounted\. The run mounted ReplicatedStorage\/Shared = .*--rojo/);
	console.log("Rojo + Wally example: rur-native render --rojo, with default.project.json found, and rur render mount its Packages; unknown options fail; a missing Packages gets a hint");
} finally {
	assert.equal(path.dirname(path.resolve(directory)), path.resolve(os.tmpdir()));
	fs.rmSync(directory, { recursive: true, force: true });
}
