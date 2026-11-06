// Offline regression checks for thumbnail fallback; no credentials or live requests are used.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import https from "node:https";
import { EventEmitter } from "node:events";

const cache = fs.mkdtempSync(path.join(os.tmpdir(), "rur-thumbnail-test-"));
const previousCache = process.env.RUR_CACHE_DIR;
process.env.RUR_CACHE_DIR = cache;
const originalGet = https.get;
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==", "base64");
let responses = [], requests = [];
https.get = (url, options, callback) => {
	requests.push(String(url));
	assert.equal(options.headers.Cookie, undefined);
	const next = responses.shift();
	assert.ok(next, `unexpected request: ${url}`);
	const request = new EventEmitter();
	request.destroy = error => request.emit("error", error);
	queueMicrotask(() => {
		if (next.error) return request.emit("error", new Error(next.error));
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

try {
	const { resolveUri } = await import("../../cli/assets.mjs");
	let id = 0;
	for (const failure of [
		[{ status: 400, body: '{"errors":[{"message":"Unsupported format"}]}' }],
		[{ error: "WebP request failed" }],
		[{ body: '{"data":[{"imageUrl":"https://cdn.example/webp"}]}' }, { status: 503, body: "unavailable" }],
		[{ body: "invalid metadata" }],
		[{ body: '{"data":[{"imageUrl":"https://cdn.example/webp"}]}' }, { body: "not an image" }],
	]) {
		responses = [...failure, { body: '{"data":[{"imageUrl":"https://cdn.example/png"}]}' }, { body: png }];
		requests = [];
		const uri = `rbxthumb://type=AvatarHeadShot&id=${++id}&w=150&h=150`;
		const result = await resolveUri(uri);
		assert.equal(result.error, undefined, JSON.stringify(result));
		assert.equal(result.mime, "image/png");
		assert.deepEqual(fs.readFileSync(result.file), png);
		assert.ok(requests.some(url => url.endsWith("format=Png")));
		assert.equal(responses.length, 0);
		const count = requests.length;
		assert.deepEqual(await resolveUri(uri), result);
		assert.equal(requests.length, count, "cached thumbnail should not download again");
	}
	responses = [{ error: "WebP failed" }, { body: '{"data":[{"imageUrl":"https://cdn.example/png"}]}' }, { status: 503, body: "unavailable" }];
	const failed = await resolveUri(`rbxthumb://type=AvatarHeadShot&id=${++id}`);
	assert.match(failed.error, /HTTP 503/);
	assert.equal(fs.readdirSync(path.join(cache, "assets")).length, 5, "failed downloads must not enter the cache");
	console.log("Thumbnail fallback: 5 failure paths, cache hits, and failed-download cache protection passed.");
} finally {
	https.get = originalGet;
	if (previousCache === undefined) delete process.env.RUR_CACHE_DIR;
	else process.env.RUR_CACHE_DIR = previousCache;
	assert.equal(path.dirname(path.resolve(cache)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(cache).startsWith("rur-thumbnail-test-"));
	fs.rmSync(cache, { recursive: true, force: true });
}
