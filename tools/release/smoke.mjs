// Runs a release zip the way a user gets it, on a fresh runner:
//
//   node tools/release/smoke.mjs --zip <zip> --version <version> --png true|false
//
// The zip is extracted into an empty folder outside every checkout, then the executable prints its
// version and renders a story of examples/project. The per-user cache locations and the temporary
// folders point into a sandbox folder, and the run must not write anywhere else.
//
// There is no Roblox Studio on the runners, so Studio's fonts and textures are not covered. With
// --png false nothing is drawn on the GPU (the Windows runners have none): the story still runs,
// lays out and exports its scene and local assets.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { ROOT, options } from "./version.mjs";
import { inspect } from "./package.mjs";
import { outsideFileStates } from "./file-states.mjs";

const args = options(process.argv.slice(2));
const drawPng = args.png === "true";

// The sandbox: one folder holding the extracted zip, a copy of the example project and every folder
// the executable might use for its cache or temporary files
const sandbox = fs.mkdtempSync(path.join(os.tmpdir(), "rur-release-smoke-"));
const bin = path.join(sandbox, "bin");
const project = path.join(sandbox, "project");
const out = path.join(sandbox, "out");
const home = path.join(sandbox, "home");
const localAppData = path.join(sandbox, "local");
const xdgCache = path.join(sandbox, "xdg");
const temp = path.join(sandbox, "tmp");
const content = path.join(sandbox, "content"); // stands in for Studio's content folder; empty
for (const dir of [bin, home, localAppData, xdgCache, temp, content]) fs.mkdirSync(dir, { recursive: true });

function extractZip(zipFile, folder) {
	for (const entry of inspect(fs.readFileSync(zipFile))) {
		assert.ok(entry.name && !/[\\/]/.test(entry.name) && entry.name !== "..", "the archive is flat");
		fs.writeFileSync(path.join(folder, entry.name), entry.bytes, { mode: entry.mode & 0o777 });
	}
}
extractZip(args.zip, bin);

fs.cpSync(path.join(ROOT, "examples/project"), project, { recursive: true });
// Decoys: a Cargo.lock beside the project, an .env file and the checkout that holds this script must
// not make the executable think it runs in a checkout or pick up a project
fs.writeFileSync(path.join(project, "Cargo.lock"), "");
fs.writeFileSync(path.join(project, ".env"), "RUR_PROJECT=must-not-be-read\n");

const env = {
	...process.env,
	HOME: home,
	USERPROFILE: home,
	LOCALAPPDATA: localAppData,
	APPDATA: localAppData,
	XDG_CACHE_HOME: xdgCache,
	TMPDIR: temp,
	TMP: temp,
	TEMP: temp,
	RUR_ROBLOX_CONTENT: content,
	RUR_NODE: "missing-node-for-standalone-smoke", // downloads must not be attempted
};
for (const name of ["RUR_STATE_DIR", "RUR_CACHE_DIR", "RUR_REPO", "RUR_PROJECT", "RUR_OUT_DIR"]) delete env[name];

// The files outside the sandbox that a stray write could change: the checkout, the runner's
// workspace, the extracted binary and the project (the last two are inside the sandbox, so they are
// named here), plus the real cache and temporary locations. See outsideFileStates for what is
// watched and why the home folders are not walked. The real locations are read from process.env,
// which the redirection in `env` above leaves alone.
function outsideFiles() {
	return outsideFileStates([ROOT, process.env.GITHUB_WORKSPACE, bin, project], sandbox);
}

// Runs the executable from inside the project, as a user would, and returns its output
function run(executable, argv) {
	const result = spawnSync(executable, argv, { cwd: project, env, encoding: "utf8", timeout: 180000, windowsHide: true });
	assert.equal(result.status, 0, result.stderr || result.error?.message);
	return result;
}

// Where the executable puts its state: the per-user cache folder of the platform
function expectedStateDir() {
	if (process.platform === "win32") return path.join(localAppData, "rur");
	if (process.platform === "darwin") return path.join(home, "Library/Caches/rur");
	return path.join(xdgCache, "rur");
}

const before = outsideFiles();
const executable = path.join(bin, "rur-native" + (process.platform === "win32" ? ".exe" : ""));

assert.equal(run(executable, ["--version"]).stdout.trim(), `rur-native ${args.version}`);

const story = "src/ReplicatedStorage/Stories/Badge.story.luau";
run(executable, ["render", "--dir", out, "--story", story, "--viewport", "320x160", "--time", "1", ...(drawPng ? [] : ["--no-png"])]);
const scene = JSON.parse(fs.readFileSync(path.join(out, "scene.json"), "utf8"));
assert.equal(scene.status, "ok", scene.failure);
assert.ok(scene.nodes.some(node => node.path.endsWith(".Badge")));
if (drawPng) assert.equal(fs.readFileSync(path.join(out, "render.png")).subarray(1, 4).toString(), "PNG");

assert.ok(!fs.existsSync(path.join(project, ".cache")));
assert.ok(fs.existsSync(expectedStateDir()), "the per-user cache folder was created");
assert.deepEqual(outsideFiles(), before, "files changed outside the redirected cache, the output and the temporary folders");

console.log(`release smoke: ${args.version}, standalone story, ${drawPng ? "PNG" : "scene export"}, redirected cache, input/user files unchanged`);
console.log(`smoke output: ${sandbox}`);
