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
} finally {
	assert.equal(path.dirname(path.resolve(directory)), path.resolve(os.tmpdir()));
	fs.rmSync(directory, { recursive: true, force: true });
}
