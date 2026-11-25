// tools/emulator.mjs pin: rur's own ror-archive entry follows the commit the pinned engine reads archives with,
// and this checkout's does
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { ROOT, archiveReaderWithEngine, emulatorLink, pinnedCommit } from "../emulator.mjs";

const OLD = "1".repeat(40);
const NEW = "2".repeat(40);
const ENGINE = "e".repeat(40);
const URL = "https://example.test/renderer";

const manifest = (rev) => [
	"[dependencies]",
	`roblox-engine-emulator = { git = "https://example.test/engine", rev = "${ENGINE}", default-features = false }`,
	"# the archive reader, at the commit the engine depends on",
	`ror-archive = { git = "${URL}", rev = "${rev}" }`,
	'rustc-hash = "=2.1.3"',
	"",
].join("\n");
const locked = (...revs) => revs.map(rev => [
	"[[package]]",
	'name = "ror-archive"',
	'version = "0.10.0"',
	`source = "git+${URL}?rev=${rev}#${rev}"`,
	"dependencies = [",
	' "blake3",',
	"]",
	"",
].join("\n")).join("\n") + [
	"",
	"[[package]]",
	'name = "ror-mesh"',
	'version = "0.10.0"',
	`source = "git+${URL}?rev=${revs.at(-1)}#${revs.at(-1)}"`,
	"",
].join("\n");

// The engine reads with the commit the manifest names: nothing to move
assert.equal(archiveReaderWithEngine(manifest(OLD), locked(OLD)), null);

// A pin moved the engine to one that reads with another commit: the lock names both, and the manifest follows
const moved = archiveReaderWithEngine(manifest(OLD), locked(OLD, NEW));
assert.deepEqual({ from: moved.from, to: moved.to }, { from: OLD, to: NEW });
assert.equal(moved.manifest, manifest(NEW), "only the entry's commit changes");
assert.equal(archiveReaderWithEngine(moved.manifest, locked(NEW)), null, "and the next settle has one commit");

// (a lock with Windows line endings reads the same)
assert.equal(archiveReaderWithEngine(manifest(OLD), locked(OLD, NEW).replaceAll("\n", "\r\n")).to, NEW);

// A manifest that does not depend on the reader itself is left alone
assert.equal(archiveReaderWithEngine(manifest(OLD).replace(/^ror-archive.*\n/m, ""), locked(NEW)), null);

// Three commits cannot be settled by rule
assert.throws(() => archiveReaderWithEngine(manifest(OLD), locked(OLD, NEW, ENGINE)), /3 commits/);

// This checkout: the reader rur depends on is the one its pinned engine reads with. (A linked checkout is whatever
// is being worked on there; the pin that ends the link settles it.)
if (!emulatorLink()) {
	const here = archiveReaderWithEngine(fs.readFileSync(path.join(ROOT, "native/Cargo.toml"), "utf8"), fs.readFileSync(path.join(ROOT, "native/Cargo.lock"), "utf8"));
	assert.ok(!here, here && `native/Cargo.toml has ror-archive at ${here.from.slice(0, 12)} and the pinned engine reads archives with ${here.to.slice(0, 12)}: node tools/emulator.mjs pin ${pinnedCommit()} moves it`);
}

console.log("emulator pin: ok");
