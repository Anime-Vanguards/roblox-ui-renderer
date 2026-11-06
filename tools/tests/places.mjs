// A project's places from its rur.json (cli/places.mjs; native/src/places.rs reads the same file).
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { DEFAULT_PLACE, defaultPlace, placeOf, placeRoots, projectRootList, readPlaces } from "../../cli/places.mjs";

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-places-test-"));
try {
	// No rur.json, or one without places: one default place at the project's root
	assert.deepEqual(readPlaces(dir), [{ name: DEFAULT_PLACE, roots: [dir] }]);
	fs.writeFileSync(path.join(dir, "rur.json"), JSON.stringify({ placeIds: {} }));
	assert.equal(defaultPlace(dir), DEFAULT_PLACE);
	assert.deepEqual(placeRoots(dir), [dir]);
	assert.equal(placeOf(dir, path.join(dir, "a", "b.story.luau")), DEFAULT_PLACE);

	// Declared places keep their order (a byte-order mark is fine; a repeated name is ignored)
	fs.writeFileSync(path.join(dir, "rur.json"), "﻿" + JSON.stringify({ places: [
		{ name: "client", roots: ["shared", "client"] },
		{ name: "server", roots: ["shared", "server"] },
		{ name: "client", roots: ["other"] },
		{ roots: ["nameless"] },
	] }));
	assert.deepEqual(readPlaces(dir).map(place => place.name), ["client", "server"]);
	assert.equal(defaultPlace(dir), "client");
	assert.deepEqual(placeRoots(dir, "server"), [path.join(dir, "shared"), path.join(dir, "server")]);
	assert.deepEqual(placeRoots(dir), placeRoots(dir, "client"));
	assert.deepEqual(placeRoots(dir, "missing"), []);
	assert.equal(placeOf(dir, path.join(dir, "server", "X.story.luau")), "server");
	assert.equal(placeOf(dir, path.join(dir, "shared", "X.story.luau")), "client", "a shared root runs with the first place");
	assert.equal(placeOf(dir, path.join(dir, "serverless", "X.story.luau")), "client", "a prefix of a folder name is not inside it");
	assert.equal(placeOf(os.tmpdir(), path.join(dir, "X.story.luau")), DEFAULT_PLACE);
	assert.deepEqual(projectRootList(dir).map(item => [path.basename(item.root), item.place]), [["shared", "client"], ["client", "client"], ["server", "server"]]);
	console.log("Project places: default place, declared order, roots, file places, shared roots");
} finally {
	fs.rmSync(dir, { recursive: true, force: true });
}
