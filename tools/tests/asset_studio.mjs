// Images asset delivery refuses are read from Studio through a Studio MCP bridge (cli/studio-images.mjs):
// stored as PNG in the asset cache with a "studio" record, used from the cache afterwards, never read
// offline or without a configured bridge, and an id Studio cannot read keeps its error with Studio's
// reason. Many ids go in one call, compressed, until its budget is used; the rest go in the next. The
// bridge (an HTTP one, as robloxstudio-mcp --http-only serves) and asset delivery are synthetic.
import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { EventEmitter } from "node:events";
import { PNG } from "pngjs";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "rur-asset-studio-test-"));
const repo = path.join(temporary, "renderer");
const names = ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY", "RUR_COOKIE_ENV", "RUR_PROJECT", "RUR_CACHE_DIR", "RUR_STATE_DIR", "RUR_STUDIO_MCP_URL", "RUR_STUDIO_MCP_PORT", "RUR_STUDIO_IMAGES", "RUR_STUDIO"];
const previousEnv = new Map(names.map(name => [name, process.env[name]]));
const originalGet = https.get, originalRequest = https.request;

// 2 x 1 pixels: opaque red, and green at alpha 0 (its colour must survive)
const PIXELS = Buffer.from([255, 0, 0, 255, 0, 255, 0, 0]);
let luauCalls = [];
let scans = 0;

function refuse(callback) {
	const request = new EventEmitter();
	request.destroy = error => request.emit("error", error);
	request.end = () => {};
	queueMicrotask(() => {
		const response = new EventEmitter();
		response.statusCode = 200;
		response.headers = {};
		response.resume = () => {};
		callback(response);
		response.emit("data", Buffer.from(JSON.stringify({ errors: [{ code: 401, message: "Authentication required to access Asset." }] })));
		response.emit("end");
	});
	return request;
}
https.get = (url, options, callback) => refuse(callback);
https.request = (url, options, callback) => refuse(callback);

// The bridge: one edit session; ids 31 and 32 are readable, 33 is not the creator's
const bridge = http.createServer((request, response) => {
	let body = "";
	request.on("data", chunk => body += chunk);
	request.on("end", () => {
		const reply = value => response.end(JSON.stringify(value));
		if (request.method === "GET" && request.url === "/instances") {
			return reply({ instances: [{ instanceId: "edit-1", role: "edit", placeName: "Test Place" }, { instanceId: "play-1", role: "server", placeName: "Test Place" }] });
		}
		assert.equal(`${request.method} ${request.url}`, "POST /mcp/execute_luau");
		const { code, sessionId } = JSON.parse(body);
		assert.equal(sessionId, "edit-1", "only edit sessions are asked");
		// The place scan (rur assets warm): its GUI names 41 and 42 (both readable) and 43 (not the creator's)
		if (code.includes("GetDescendants")) {
			scans++;
			return reply({ content: [{ type: "text", text: JSON.stringify({ success: true, returnValue: JSON.stringify(["41", "42", "43"]) }) }] });
		}
		const ids = /local Ids = \{ ([\d, ]*) \}/.exec(code)[1].split(", ");
		luauCalls.push(ids);
		// Two images fill a call's budget: the rest is asked for in the next call. 31 travels compressed.
		const value = ids.slice(0, 2).map(id => id === "33" || id === "43"
			? { error: "AssetService::CreateEditableImageAsync failed because no permission to load asset" }
			: id === "31"
				? { width: 2, height: 1, encoding: "zstd", pixels: zlib.zstdCompressSync(PIXELS).toString("base64") }
				: { width: 2, height: 1, encoding: "raw", pixels: PIXELS.toString("base64") });
		reply({ content: [{ type: "text", text: JSON.stringify({ success: true, returnValue: JSON.stringify(value) }) }] });
	});
});

try {
	fs.mkdirSync(path.join(repo, "cli"), { recursive: true });
	for (const name of ["assets.mjs", "paths.mjs", "studio-images.mjs", "asset-warm.mjs"]) fs.copyFileSync(path.join(root, "cli", name), path.join(repo, "cli", name));
	fs.symlinkSync(path.join(root, "node_modules"), path.join(repo, "node_modules"), "junction");
	await new Promise(resolve => bridge.listen(0, "127.0.0.1", resolve));
	for (const name of names) delete process.env[name];
	process.env.RUR_STATE_DIR = path.join(temporary, "state");
	process.env.RUR_COOKIE_ENV = path.join(temporary, "absent.env");
	process.env.RUR_PROJECT = path.join(temporary, "no-project");
	const { resolveAll } = await import(pathToFileURL(path.join(repo, "cli", "assets.mjs")).href);

	const uris = ["rbxassetid://31", "rbxassetid://32", "rbxassetid://33"];

	// No bridge configured: refused, and Studio is not asked
	let results = await resolveAll(uris);
	assert.ok(uris.every(uri => /Authentication required/.test(results.get(uri).error)));
	assert.equal(luauCalls.length, 0);

	// A bridge: 31 and 32 come from Studio, 33 keeps the refusal and gains Studio's reason
	process.env.RUR_STUDIO_MCP_URL = `http://127.0.0.1:${bridge.address().port}`;
	results = await resolveAll(uris);
	for (const uri of uris.slice(0, 2)) {
		const result = results.get(uri);
		assert.ok(result.file && result.kind === "image", `${uri} resolved`);
		const image = PNG.sync.read(fs.readFileSync(result.file));
		assert.deepEqual([image.width, image.height, ...image.data], [2, 1, ...PIXELS], "the pixels as Studio read them, colour under alpha 0 included");
		const record = JSON.parse(fs.readFileSync(path.join(path.dirname(result.file), "downloaded", `${path.basename(result.file)}.json`), "utf8"));
		assert.equal(record.source, "studio");
		assert.equal(record.size, fs.statSync(result.file).size);
	}
	assert.match(results.get("rbxassetid://33").error, /Authentication required.*; Studio \(Test Place\) could not read it: .*no permission/);
	assert.deepEqual(luauCalls, [["31", "32", "33"], ["33"]], "the ids in one call, the rest in the next once its budget is used");

	// Read once: cached afterwards, and never read offline
	luauCalls = [];
	results = await resolveAll(uris.slice(0, 2));
	assert.ok(results.get("rbxassetid://31").file);
	await resolveAll(["rbxassetid://34"], { offline: true });
	assert.equal(luauCalls.length, 0, "a cached Studio read and an offline render ask Studio nothing");

	// RUR_STUDIO_IMAGES=0, and a bridge that is not there
	process.env.RUR_STUDIO_IMAGES = "0";
	await resolveAll(["rbxassetid://35"]);
	assert.equal(luauCalls.length, 0);
	delete process.env.RUR_STUDIO_IMAGES;
	process.env.RUR_STUDIO_MCP_URL = "http://127.0.0.1:9";
	results = await resolveAll(["rbxassetid://36"]);
	assert.match(results.get("rbxassetid://36").error, /no Studio to read it from/);

	// rur assets warm: needs a bridge; reads what the place names, then finds it cached; --refresh reads the
	// Studio images again
	const { warmAssets } = await import(pathToFileURL(path.join(repo, "cli", "asset-warm.mjs")).href);
	await assert.rejects(warmAssets({ env: {} }), /set RUR_STUDIO_MCP_URL or \w+ to a Studio MCP bridge/);
	process.env.RUR_STUDIO_MCP_URL = `http://127.0.0.1:${bridge.address().port}`;
	const pick = ({ images, cached, downloaded, fromStudio, refreshed, missing }) => ({ images, cached, downloaded, fromStudio, refreshed, missing: missing.map(entry => entry.id) });
	assert.deepEqual(pick(await warmAssets()), { images: 3, cached: 0, downloaded: 0, fromStudio: 2, refreshed: 0, missing: ["43"] });
	luauCalls = [];
	assert.deepEqual(pick(await warmAssets()), { images: 3, cached: 2, downloaded: 0, fromStudio: 0, refreshed: 0, missing: ["43"] });
	assert.deepEqual(luauCalls, [["43"]], "a warm cache asks Studio only for what it could not read");
	assert.deepEqual(pick(await warmAssets({ refresh: true })), { images: 3, cached: 0, downloaded: 0, fromStudio: 0, refreshed: 2, missing: ["43"] });
	assert.equal(scans, 3);
	console.log("Asset studio: refused images read from Studio, cached with a studio record, offline, opt-out and assets warm passed.");
} finally {
	bridge.close();
	https.get = originalGet;
	https.request = originalRequest;
	for (const [name, value] of previousEnv) {
		if (value === undefined) delete process.env[name];
		else process.env[name] = value;
	}
	assert.equal(path.dirname(path.resolve(temporary)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(temporary).startsWith("rur-asset-studio-test-"));
	fs.rmSync(temporary, { recursive: true, force: true });
}
