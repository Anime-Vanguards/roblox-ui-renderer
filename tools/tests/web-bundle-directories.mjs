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
console.log("web bundle directories: ok");
