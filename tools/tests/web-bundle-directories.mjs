// tools/web/bundle.mjs directoryList: the page makes the parent of every file it writes. A story outside
// the project sat under /ext/<hash of the file>/ while the list held /ext/<hash of its folder>/<folder>,
// so the page's FS.writeFile failed (ErrnoError) before the story mounted.
import assert from "node:assert/strict";
import { directoryList } from "../web/bundle.mjs";

assert.deepEqual(
	directoryList(
		["/project", "/project/source", "/project/source/Empty", "/ext/881beadd0f76/proto"],
		["/ext/49ad7cbae204/HeaderMotion.story.luau", "/project/source/Module.luau", "/rur/web-images.json"],
	),
	["/ext/49ad7cbae204", "/project", "/project/source", "/project/source/Empty", "/rur"],
);
assert.deepEqual(directoryList([], ["/top.json"]), ["/"]);

// native/web/host.js wheelNotches + native/web/index.html wheel listener:
// Shift+wheel swapped onto deltaX by the browser still produces notches and forwards `shift: true`.
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const hostSource = fs.readFileSync(path.join(root, "native", "web", "host.js"), "utf8");
const sandbox = { console, Math, Number, Array, Object, String, Boolean, JSON, Uint8Array, Promise, performance: { now: () => 0 }, setTimeout };
sandbox.globalThis = sandbox;
vm.runInNewContext(hostSource, sandbox);
const { wheelNotches } = sandbox.RurHost;

assert.equal(wheelNotches({ deltaX: 0, deltaY: 100, deltaMode: 0, shiftKey: false }, 600), -0.5, "plain vertical wheel down");
assert.equal(wheelNotches({ deltaX: 0, deltaY: -100, deltaMode: 0, shiftKey: false }, 600), 0.5, "plain vertical wheel up");
assert.equal(wheelNotches({ deltaX: 100, deltaY: 0, deltaMode: 0, shiftKey: true }, 600), -0.5, "Shift+wheel swapped to +deltaX");
assert.equal(wheelNotches({ deltaX: -100, deltaY: 0, deltaMode: 0, shiftKey: true }, 600), 0.5, "Shift+wheel swapped to -deltaX");
assert.equal(wheelNotches({ deltaX: 3, deltaY: 0, deltaMode: 1, shiftKey: true }, 600), -0.5, "Shift+wheel line deltaMode");
assert.equal(wheelNotches({ deltaX: 100, deltaY: 0, deltaMode: 0, shiftKey: false }, 600), 0, "unshifted deltaX stays ignored");

const indexHtml = fs.readFileSync(path.join(root, "native", "web", "index.html"), "utf8");
assert.match(indexHtml, /type:\s*"wheel"[^}]*shift:\s*Boolean\(event\.shiftKey\)/, "index.html must forward shiftKey on wheel events");

console.log("web bundle directories & host wheel: ok");
