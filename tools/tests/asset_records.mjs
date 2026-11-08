// The asset cache serves only what rur downloaded (cli/assets.mjs downloaded): a file another tool put
// under an asset's name is downloaded again, a cached file changed in place too, and offline either is
// used as it is. HTTP responses are synthetic and no cookie is configured (assets.mjs runs from a
// temporary copy of the repo, so no checkout's .env is read).
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import https from "node:https";
import os from "node:os";
import path from "node:path";
import { EventEmitter } from "node:events";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "rur-asset-records-test-"));
const repo = path.join(temporary, "renderer");
const assets = path.join(temporary, "state", "assets");
const names = ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY", "RUR_COOKIE_ENV", "RUR_PROJECT", "RUR_CACHE_DIR", "RUR_STATE_DIR", "RUR_STUDIO_MCP_URL", "RUR_STUDIO_MCP_PORT"];
const previousEnv = new Map(names.map(name => [name, process.env[name]]));
const originalGet = https.get;
// The asset ("real", a 1 x 1 PNG) and a stand-in for it (other PNG bytes)
const real = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==", "base64");
const standIn = Buffer.concat([real, Buffer.from("stand-in")]);
const sha256 = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
let responses = [], requests = [];

https.get = (url, options, callback) => {
	requests.push(String(url));
	assert.equal(options.headers.Cookie, undefined, "no cookie is configured");
	const next = responses.shift();
	assert.ok(next, `unexpected request: ${url}`);
	const request = new EventEmitter();
	request.destroy = error => request.emit("error", error);
	queueMicrotask(() => {
		const response = new EventEmitter();
		response.statusCode = next.status || 200;
		response.headers = {};
		response.resume = () => {};
		callback(response);
		response.emit("data", Buffer.isBuffer(next.body) ? next.body : Buffer.from(next.body));
		response.emit("end");
	});
	return request;
};
const delivered = body => [{ body: JSON.stringify({ location: "https://cdn.example/hash" }) }, { body }];
const record = name => JSON.parse(fs.readFileSync(path.join(assets, "downloaded", `${name}.json`), "utf8"));

try {
	for (const name of ["assets.mjs", "paths.mjs"]) {
		fs.mkdirSync(path.join(repo, "cli"), { recursive: true });
		fs.copyFileSync(path.join(root, "cli", name), path.join(repo, "cli", name));
	}
	for (const name of names) delete process.env[name];
	process.env.RUR_STATE_DIR = path.join(temporary, "state");
	process.env.RUR_COOKIE_ENV = path.join(temporary, "absent.env");
	process.env.RUR_PROJECT = path.join(temporary, "no-project");
	const { resolveUri } = await import(pathToFileURL(path.join(repo, "cli", "assets.mjs")).href);
	const resolve = async (id, options) => {
		requests = [];
		const result = await resolveUri(`rbxassetid://${id}`, options);
		assert.equal(responses.length, 0, "every expected request was made");
		return result;
	};

	// A download is recorded, and the next resolve is a cache hit
	responses = delivered(real);
	const first = await resolve(501);
	assert.deepEqual(fs.readFileSync(first.file), real);
	assert.deepEqual(record("501.png"), { sha256: sha256(real), size: real.length });
	assert.equal((await resolve(501)).file, first.file);
	assert.equal(requests.length, 0, "a recorded download is served from the cache");

	// A stand-in another tool wrote under an asset's name is replaced by the asset (by rename: a render
	// directory's link to the stand-in keeps it)
	fs.mkdirSync(assets, { recursive: true });
	const foreign = path.join(assets, "502.png");
	fs.writeFileSync(foreign, standIn);
	const renderLink = path.join(temporary, "render-link.png");
	fs.linkSync(foreign, renderLink);
	responses = delivered(real);
	const replaced = await resolve(502);
	assert.equal(requests.length, 2, "an unrecorded cache file is downloaded again");
	assert.equal(replaced.file, foreign);
	assert.deepEqual(fs.readFileSync(foreign), real);
	assert.deepEqual(fs.readFileSync(renderLink), standIn, "the stand-in was replaced, not written into");
	assert.equal(record("502.png").sha256, sha256(real));

	// A stand-in under another extension goes when the asset arrives (it would be found first)
	const jpegName = path.join(assets, "504.jpg");
	fs.writeFileSync(path.join(assets, "504.png"), standIn);
	responses = delivered(Buffer.concat([Buffer.from([0xff, 0xd8, 0xff]), Buffer.from("jpeg")]));
	assert.equal((await resolve(504)).file, jpegName);
	assert.ok(!fs.existsSync(path.join(assets, "504.png")));

	// A recorded file changed in place is downloaded again
	fs.writeFileSync(first.file, standIn);
	responses = delivered(real);
	assert.deepEqual(fs.readFileSync((await resolve(501)).file), real);
	assert.equal(requests.length, 2);

	// An unrecorded file holding the asset's bytes keeps its file (render directories' links stay
	// current) and gets its record
	const legacy = path.join(assets, "505.png");
	fs.writeFileSync(legacy, real);
	const inode = fs.statSync(legacy, { bigint: true }).ino;
	responses = delivered(real);
	await resolve(505);
	assert.equal(fs.statSync(legacy, { bigint: true }).ino, inode);
	assert.equal(record("505.png").sha256, sha256(real));

	// A stand-in for an asset delivery refuses is not drawn online, and the refusal says why; offline the
	// cache is used as it is, without a request
	fs.writeFileSync(path.join(assets, "503.png"), standIn);
	responses = [{ body: JSON.stringify({ errors: [{ code: 401, message: "Authentication required to access Asset." }] }) }];
	const refused = await resolve(503);
	assert.equal(refused.file, undefined);
	assert.match(refused.error, /\(401: Authentication required to access Asset\.; private asset, no asset cookie is configured\).*503\.png is not rur's download/);
	const offline = await resolve(503, { offline: true });
	assert.equal(requests.length, 0);
	assert.deepEqual(fs.readFileSync(offline.file), standIn);
	console.log("Asset records: downloads recorded, stand-ins and in-place changes downloaded again (by rename), equal bytes kept, refusals explained, offline untouched.");
} finally {
	https.get = originalGet;
	for (const [name, value] of previousEnv) {
		if (value === undefined) delete process.env[name];
		else process.env[name] = value;
	}
	assert.equal(path.dirname(path.resolve(temporary)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(temporary).startsWith("rur-asset-records-test-"));
	fs.rmSync(temporary, { recursive: true, force: true });
}
