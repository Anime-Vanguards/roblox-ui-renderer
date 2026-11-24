// Images asset delivery refuses are read from Studio through a Studio MCP bridge (cli/studio-images.mjs):
// stored as PNG in the asset cache with a "studio" record, never read without a configured bridge, and an
// id Studio cannot read keeps its error with Studio's reason. Many ids go in one call, compressed when
// Node supports it, until its budget is used; the rest go in the next. The one-shot `store` command the
// native resolver runs for a render's refused images answers what it stored. `rur assets warm` reads what
// the place names. A pinned version (rbxassetid://N&version=V) is read and cached as its own image, N-vV. Refusals are synthetic (the engine's downloader is checked by `cargo test download::`),
// and so is the bridge (an HTTP one, as robloxstudio-mcp --http-only serves).
import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { spawn } from "node:child_process";
import { PNG } from "pngjs";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "rur-asset-studio-test-"));
const repo = path.join(temporary, "renderer");
const state = path.join(temporary, "state");
const names = ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY", "RUR_COOKIE_ENV", "RUR_PROJECT", "RUR_CACHE_DIR", "RUR_STATE_DIR", "RUR_STUDIO_MCP_URL", "RUR_STUDIO_MCP_PORT", "RUR_STUDIO_IMAGES", "RUR_STUDIO"];
const previousEnv = new Map(names.map(name => [name, process.env[name]]));
const REFUSAL = "asset unavailable from asset delivery (401: Authentication required to access Asset.; private asset, no asset cookie is configured)";

// 2 x 1 pixels: opaque red, and green at alpha 0 (its colour must survive)
const PIXELS = Buffer.from([255, 0, 0, 255, 0, 255, 0, 0]);
let luauCalls = [];
let scans = 0;

// The bridge: one edit session; ids 31 and 32 are readable, 33 is not the creator's
const bridge = http.createServer((request, response) => {
	let body = "";
	request.on("data", chunk => body += chunk);
	request.on("end", () => {
		const reply = value => response.end(JSON.stringify(value));
		// An older bridge, with no /health (robloxstudio-mcp 3 is tools/tests/asset_studio_bridge.mjs)
		if (request.method === "GET" && request.url === "/health") {
			response.statusCode = 404;
			return response.end();
		}
		if (request.method === "GET" && request.url === "/instances") {
			return reply({ instances: [{ instanceId: "edit-1", role: "edit", placeName: "Test Place" }, { instanceId: "play-1", role: "server", placeName: "Test Place" }] });
		}
		assert.equal(`${request.method} ${request.url}`, "POST /mcp/execute_luau");
		const { code, sessionId } = JSON.parse(body);
		assert.equal(sessionId, "edit-1", "only edit sessions are asked");
		// The place scan (rur assets warm): its GUI names 41 (also pinned at version 3) and 42 (both readable),
		// 43 (not the creator's), and 45 in a spelling a Roblox client loads nothing for
		if (code.includes("GetDescendants")) {
			scans++;
			return reply({ content: [{ type: "text", text: JSON.stringify({ success: true, returnValue: JSON.stringify(["rbxassetid://41", "41", "rbxassetid://41&version=3", "42", "http://www.roblox.com/asset/?id=43", "rbxassetid://45?version=1"]) }) }] });
		}
		// Ids by number, pinned versions by content id
		const ids = /local Ids = \{ (.*) \}/.exec(code)[1].split(", ").map(entry => (entry.startsWith('"') ? JSON.parse(entry) : entry));
		assert.ok(ids.every(id => /^\d+$|^rbxassetid:\/\/\d+&version=\d+$/.test(id)), `only ids reach the Luau: ${ids}`);
		const compressed = code.includes("if true then");
		assert.equal(compressed, typeof zlib.zstdDecompressSync === "function", "Studio is asked to compress exactly when this Node can decompress");
		luauCalls.push(ids);
		// Two images fill a call's budget: the rest is asked for in the next call. 31 is compressed when requested.
		const value = ids.slice(0, 2).map(id => id === "33" || id === "43"
			? { error: "AssetService::CreateEditableImageAsync failed because no permission to load asset" }
			: id === "31" && compressed
				? { width: 2, height: 1, encoding: "zstd", pixels: zlib.zstdCompressSync(PIXELS).toString("base64") }
				: { width: 2, height: 1, encoding: "raw", pixels: PIXELS.toString("base64") });
		reply({ content: [{ type: "text", text: JSON.stringify({ success: true, returnValue: JSON.stringify(value) }) }] });
	});
});

// `node cli/studio-images.mjs store` with `ids` on stdin: its answer
function storeCommand(ids, env) {
	return new Promise((resolve, reject) => {
		const child = spawn(process.execPath, [path.join(repo, "cli", "studio-images.mjs"), "store"], { cwd: repo, windowsHide: true, env, stdio: ["pipe", "pipe", "pipe"] });
		let stdout = "", stderr = "";
		child.stdout.on("data", chunk => stdout += chunk);
		child.stderr.on("data", chunk => stderr += chunk);
		child.on("error", reject);
		child.on("close", status => status === 0 ? resolve(JSON.parse(stdout)) : reject(new Error(`store exited ${status}: ${stderr}`)));
		child.stdin.end(ids.join("\n"));
	});
}

try {
	fs.mkdirSync(path.join(repo, "cli"), { recursive: true });
	for (const name of ["assets.mjs", "paths.mjs", "studio-images.mjs", "asset-warm.mjs", "entry.mjs", "packages.mjs"]) fs.copyFileSync(path.join(root, "cli", name), path.join(repo, "cli", name));
	fs.symlinkSync(path.join(root, "node_modules"), path.join(repo, "node_modules"), "junction");
	await new Promise(resolve => bridge.listen(0, "127.0.0.1", resolve));
	for (const name of names) delete process.env[name];
	process.env.RUR_STATE_DIR = state;
	process.env.RUR_COOKIE_ENV = path.join(temporary, "absent.env");
	// No account broker either, whatever this machine has configured
	delete process.env.ROBLOX_BROKER_TOKEN;
	process.env.ROBLOX_BROKER_CREDENTIALS = path.join(temporary, "absent.json");
	process.env.RUR_PROJECT = path.join(temporary, "no-project");
	const { assetKeyFromUri, cachedSource, readRefusedFromStudio } = await import(pathToFileURL(path.join(repo, "cli", "assets.mjs")).href);
	// What the engine answers when asset delivery refuses every id it has not cached
	const refused = uris => new Map(uris.map(uri => [uri, { error: REFUSAL }]));
	const refuseUncached = async uris => new Map(uris.map(uri => {
		const id = assetKeyFromUri(uri);
		return [uri, cachedSource(id) ? { file: path.join(state, "assets", `${id}.png`), kind: "image", mime: "image/png" } : { error: REFUSAL }];
	}));

	const uris = ["rbxassetid://31", "rbxassetid://32", "rbxassetid://33"];

	// No bridge configured: refused, and Studio is not asked
	let results = refused(uris);
	await readRefusedFromStudio(results);
	assert.ok(uris.every(uri => results.get(uri).error === REFUSAL));
	assert.equal(luauCalls.length, 0);

	// A bridge: 31 and 32 come from Studio, 33 keeps the refusal and gains Studio's reason
	const address = `http://127.0.0.1:${bridge.address().port}`;
	process.env.RUR_STUDIO_MCP_URL = address;
	results = refused(uris);
	await readRefusedFromStudio(results);
	for (const uri of uris.slice(0, 2)) {
		const result = results.get(uri);
		assert.ok(result.file && result.kind === "image", `${uri} resolved`);
		const image = PNG.sync.read(fs.readFileSync(result.file));
		assert.deepEqual([image.width, image.height, ...image.data], [2, 1, ...PIXELS], "the pixels as Studio read them, colour under alpha 0 included");
		const record = JSON.parse(fs.readFileSync(path.join(path.dirname(result.file), "downloaded", `${path.basename(result.file)}.json`), "utf8"));
		assert.equal(record.source, "studio");
		assert.equal(record.size, fs.statSync(result.file).size);
		assert.equal(cachedSource(assetKeyFromUri(uri)), "studio", "read once: cached afterwards");
	}
	assert.match(results.get("rbxassetid://33").error, /Authentication required.*; Studio \(Test Place\) could not read it: .*no permission/);
	assert.deepEqual(luauCalls, [["31", "32", "33"], ["33"]], "the ids in one call, the rest in the next once its budget is used");

	// A pinned version: Studio loads it by its content id, and it is cached apart from the latest version
	luauCalls = [];
	const latestPng = fs.readFileSync(path.join(state, "assets", "31.png"));
	fs.writeFileSync(path.join(state, "assets", "31.png"), Buffer.concat([latestPng, Buffer.from("latest")]));
	results = refused(["rbxassetid://31&Version=02&x=y", "rbxassetid://31&version=2"]);
	await readRefusedFromStudio(results);
	assert.deepEqual(luauCalls, [["rbxassetid://31&version=2"]], "one read per version, by its numbers");
	for (const result of results.values()) assert.equal(path.basename(result.file), "31-v2.png");
	assert.equal(cachedSource("31-v2"), "studio");
	assert.ok(fs.readFileSync(path.join(state, "assets", "31.png")).equals(Buffer.concat([latestPng, Buffer.from("latest")])), "the latest version's file is left alone");

	// A model without a texture is not Studio's to read; RUR_STUDIO_IMAGES=0 turns reading off
	luauCalls = [];
	results = new Map([["rbxassetid://34", { error: "asset 34 is a model without an image texture" }]]);
	await readRefusedFromStudio(results);
	process.env.RUR_STUDIO_IMAGES = "0";
	await readRefusedFromStudio(refused(["rbxassetid://35"]));
	assert.equal(luauCalls.length, 0);
	delete process.env.RUR_STUDIO_IMAGES;
	// A bridge that is not there
	process.env.RUR_STUDIO_MCP_URL = "http://127.0.0.1:9";
	results = refused(["rbxassetid://36"]);
	await readRefusedFromStudio(results);
	assert.match(results.get("rbxassetid://36").error, /no Studio to read it from/);
	process.env.RUR_STUDIO_MCP_URL = address;

	// The one-shot command the engine runs for a render's refused images (native/src/download.rs studio_node):
	// what it stored is in the cache with its record, and each other id has Studio's reason
	luauCalls = [];
	const stored = await storeCommand(["31", "33", "31"], { ...process.env, RUR_STATE_DIR: state, RUR_STUDIO_MCP_URL: address });
	assert.deepEqual(stored.stored, ["31"]);
	assert.deepEqual(Object.keys(stored.errors), ["33"]);
	assert.match(stored.errors["33"], /^Studio \(Test Place\) could not read it: .*no permission/);
	assert.deepEqual(luauCalls, [["31", "33"]], "each id once");
	assert.equal(cachedSource("31"), "studio");

	// rur assets warm: needs a bridge; reads what the place names, then finds it cached; --refresh reads the
	// Studio images again
	const { warmAssets } = await import(pathToFileURL(path.join(repo, "cli", "asset-warm.mjs")).href);
	await assert.rejects(warmAssets({ env: {}, resolve: refuseUncached }), /set RUR_STUDIO_MCP_URL or \w+ to a Studio MCP bridge/);
	const pick = ({ images, cached, downloaded, fromStudio, refreshed, missing }) => ({ images, cached, downloaded, fromStudio, refreshed, missing: missing.map(entry => entry.id) });
	assert.deepEqual(pick(await warmAssets({ resolve: refuseUncached })), { images: 4, cached: 0, downloaded: 0, fromStudio: 3, refreshed: 0, missing: ["43"] });
	assert.equal(cachedSource("41-v3"), "studio", "the pinned version is read as its own image");
	luauCalls = [];
	assert.deepEqual(pick(await warmAssets({ resolve: refuseUncached })), { images: 4, cached: 3, downloaded: 0, fromStudio: 0, refreshed: 0, missing: ["43"] });
	assert.deepEqual(luauCalls, [["43"]], "a warm cache asks Studio only for what it could not read");
	assert.deepEqual(pick(await warmAssets({ refresh: true, resolve: refuseUncached })), { images: 4, cached: 0, downloaded: 0, fromStudio: 0, refreshed: 3, missing: ["43"] });
	assert.equal(scans, 3);
	console.log("Asset studio: refused images read from Studio, cached with a studio record, pinned versions apart, the one-shot store command, opt-out and assets warm passed.");
} finally {
	bridge.close();
	for (const [name, value] of previousEnv) {
		if (value === undefined) delete process.env[name];
		else process.env[name] = value;
	}
	assert.equal(path.dirname(path.resolve(temporary)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(temporary).startsWith("rur-asset-studio-test-"));
	fs.rmSync(temporary, { recursive: true, force: true });
}
