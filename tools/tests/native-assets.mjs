// All credentials are synthetic; missing offline assets must never make an HTTP request.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import readline from "node:readline";
import { fileURLToPath, pathToFileURL } from "node:url";
import { NativeAssetWorker } from "../../cli/native-assets.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rur-native-assets-test-"));
const cache = path.join(directory, "assets");
fs.mkdirSync(cache);
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==", "base64");
fs.writeFileSync(path.join(cache, "123.png"), png);
fs.writeFileSync(path.join(cache, "thumb-AvatarHeadShot-42-150x150-webp.png"), png);
fs.writeFileSync(path.join(cache, "999.rbxmx"), '<roblox><url>rbxassetid://124</url></roblox>');
const preload = path.join(directory, "offline-guard.mjs");
fs.writeFileSync(preload, 'import https from "node:https"; https.get = () => { throw new Error("NETWORK WAS ATTEMPTED"); };');
const child = spawn(process.execPath, ["--import", pathToFileURL(preload).href, path.join(root, "cli/native-assets.mjs"), "--offline"], {
	cwd: root, windowsHide: true, stdio: ["pipe", "pipe", "pipe"],
	env: { ...process.env, RUR_CACHE_DIR: directory, ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-native-assets", ROBLOSECURITY: "dummy-native-assets", RUR_COOKIE_ENV: path.join(directory, "absent.env"), RUR_STUDIO_IMAGES: "0" },
});
const lines = readline.createInterface({ input: child.stdout });
const iterator = lines[Symbol.asyncIterator]();
let stderr = "";
child.stderr.on("data", chunk => { stderr += chunk; });
let id = 0;
const request = async payload => {
	child.stdin.write(JSON.stringify({ id: ++id, ...payload }) + "\n");
	const { value, done } = await iterator.next();
	assert.ok(!done, `asset helper exited: ${stderr}`);
	assert.ok(!value.includes("dummy-native-assets"), "credentials never enter the response");
	const result = JSON.parse(value);
	assert.equal(result.id, id);
	assert.equal(result.error, undefined);
	return result;
};
const timer = setTimeout(() => child.kill(), 30000);
try {
	const link = path.join(directory, "linked");
	fs.symlinkSync(root, link, process.platform === "win32" ? "junction" : "dir");
	const linked = spawnSync(process.execPath, [path.join(link, "cli/native-assets.mjs"), "--offline"], {
		cwd: directory, windowsHide: true, encoding: "utf8", timeout: 10000,
		env: { ...process.env, RUR_STATE_DIR: directory },
		input: '{"id":1,"images":[]}\n{"command":"quit"}\n',
	});
	assert.equal(linked.status, 0, linked.stderr);
	assert.deepEqual(JSON.parse(linked.stdout), { id: 1, images: [] }, "the helper starts through a linked folder");
	const worker = new NativeAssetWorker({ offline: true });
	const direct = await worker.prepare({ id: 7, images: [], scene: { nodes: [{ text: { lines: [{ glyphs: [[0, 41, 0, 18]] }] } }] } });
	assert.deepEqual(direct, { id: 7, images: [] }, "the helper answers images only (glyphs are rasterized natively)");
	const result = await request({ images: ["rbxassetid://123", "rbxassetid://123", "rbxthumb://type=AvatarHeadShot&id=42&w=150&h=150", "rbxassetid://999", "rbxassetid://404"], scene: {} });
	assert.equal(result.images.length, 4, "duplicate image requirements are resolved once");
	assert.deepEqual(fs.readFileSync(result.images.find(row => row.uri === "rbxassetid://123").file), png);
	assert.ok(result.images.find(row => row.uri.startsWith("rbxthumb:")).file, "cached thumbnails work offline");
	assert.match(result.images.find(row => row.uri === "rbxassetid://999").error, /offline/, "nested decal images preserve offline mode");
	assert.match(result.images.find(row => row.uri === "rbxassetid://404").error, /offline/);
	assert.ok(!JSON.stringify(result).includes("NETWORK WAS ATTEMPTED"));
	const fontsOffline = await request({ command: "fonts", diagnostics: { missingFontAssets: [12345] } });
	assert.equal(fontsOffline.fontsChanged, false, "offline font jobs never fetch");
	const scene = { fonts: [{ file: path.join(directory, "missing-font.ttf"), weight: 400 }], viewport: { scale: 1 }, nodes: [{ text: { weight: 400, lines: [{ glyphs: [[0, 41, 0, 18]] }] } }] };
	const ignored = await request({ images: [], scene });
	assert.deepEqual(ignored.images, []);
	assert.equal(ignored.glyphError, undefined, "scenes sent by older callers are ignored, not rasterized");
	assert.equal(ignored.pages, undefined);
	child.stdin.end('{"command":"quit"}\n');
	await new Promise(resolve => child.once("close", resolve));
	assert.equal(child.exitCode, 0);
	assert.equal(stderr, "");
	console.log("Native assets: cached images/thumbnails, offline decal isolation, deduplication and offline font checks passed (glyph pages: cargo test assets::tests).");
} finally {
	clearTimeout(timer);
	child.kill();
	lines.close();
	assert.equal(path.dirname(path.resolve(directory)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(directory).startsWith("rur-native-assets-test-"));
	fs.rmSync(directory, { recursive: true, force: true });
}
