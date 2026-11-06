// resolveAll locates the asset ids it must download through asset delivery's batch endpoint (one
// request for many, with the cookie and the CSRF token it asks for), then downloads them from the CDN
// without the cookie. An id the batch does not place, or whose located download fails, goes through
// the per-asset lookup as before. All credentials and HTTP responses here are synthetic.
import assert from "node:assert/strict";
import fs from "node:fs";
import https from "node:https";
import os from "node:os";
import path from "node:path";
import { EventEmitter } from "node:events";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "rur-asset-batch-test-"));
const repo = path.join(temporary, "renderer");
const names = ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY", "RUR_COOKIE_ENV", "RUR_PROJECT", "RUR_CACHE_DIR", "RUR_STATE_DIR"];
const previousEnv = new Map(names.map(name => [name, process.env[name]]));
const originalGet = https.get, originalRequest = https.request;
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==", "base64");
const COOKIE = ".ROBLOSECURITY=dummy-batch";
let requests = [];

function respond(callback, { status = 200, headers = {}, body = "" }) {
	const request = new EventEmitter();
	request.destroy = error => request.emit("error", error);
	request.end = () => {};
	queueMicrotask(() => {
		const response = new EventEmitter();
		response.statusCode = status;
		response.headers = headers;
		response.resume = () => {};
		callback(response);
		response.emit("data", Buffer.isBuffer(body) ? body : Buffer.from(body));
		response.emit("end");
	});
	return request;
}

// Asset delivery: the batch endpoint places 11 and 12 (13 is "not found"); the per-asset route places 13
// and 14. The CDN serves every file but 12's batch location (an expired link).
https.request = (url, options, callback) => {
	const { hostname, pathname } = new URL(url);
	const token = options.headers["X-CSRF-TOKEN"];
	requests.push({ method: options.method, host: hostname, path: pathname, cookie: options.headers.Cookie, token });
	assert.equal(`${options.method} ${hostname}${pathname}`, "POST assetdelivery.roblox.com/v1/assets/batch");
	if (!token) return respond(callback, { status: 403, headers: { "x-csrf-token": "dummy-token" } });
	return respond(callback, { body: JSON.stringify([
		{ requestId: "0", location: "https://cdn.example/11" },
		{ requestId: "1", location: "https://cdn.example/12-expired" },
		{ requestId: "2", errors: [{ code: 404, message: "Request asset was not found" }] },
	]) });
};
https.get = (url, options, callback) => {
	const { hostname, pathname } = new URL(url);
	requests.push({ method: "GET", host: hostname, path: pathname, cookie: options.headers.Cookie });
	if (hostname === "cdn.example") return respond(callback, pathname.endsWith("-expired") ? { status: 403 } : { body: png });
	const id = pathname.split("/").pop();
	return respond(callback, { body: JSON.stringify({ location: `https://cdn.example/${id}-lookup` }) });
};

try {
	for (const name of ["assets.mjs", "paths.mjs"]) {
		fs.mkdirSync(path.join(repo, "cli"), { recursive: true });
		fs.copyFileSync(path.join(root, "cli", name), path.join(repo, "cli", name));
	}
	for (const name of names) delete process.env[name];
	process.env.RUR_STATE_DIR = path.join(temporary, "state");
	process.env.RUR_COOKIE_ENV = path.join(temporary, "absent.env");
	process.env.RUR_PROJECT = path.join(temporary, "no-project");
	process.env.ROBLOX_ASSET_DELIVERY_COOKIE = COOKIE;
	const { resolveAll } = await import(pathToFileURL(path.join(repo, "cli", "assets.mjs")).href);

	// 11 to 13 are located in one batch request (answered after the CSRF token); 14 is cached already
	await resolveAll(["rbxassetid://14"]);
	requests = [];
	const results = await resolveAll(["rbxassetid://11", "rbxassetid://12", "rbxassetid://13", "rbxassetid://14", "rbxassetid://11"]);
	const posts = requests.filter(request => request.method === "POST");
	assert.equal(posts.length, 2, "one batch request, repeated once with the CSRF token");
	assert.deepEqual(posts.map(post => post.token), [undefined, "dummy-token"]);
	assert.ok(posts.every(post => post.cookie === COOKIE), "the batch request carries the cookie");
	for (const id of ["11", "12", "13", "14"]) assert.deepEqual(fs.readFileSync(results.get(`rbxassetid://${id}`).file), png, `asset ${id}`);
	const gets = requests.filter(request => request.method === "GET").map(request => `${request.host}${request.path}`).sort();
	assert.deepEqual(gets, [
		"assetdelivery.roblox.com/v1/assetId/12", // its located download failed
		"assetdelivery.roblox.com/v1/assetId/13", // the batch did not place it
		"cdn.example/11",
		"cdn.example/12-expired",
		"cdn.example/12-lookup",
		"cdn.example/13-lookup",
	], "a located asset is not looked up again; the cached one is not fetched");
	assert.ok(requests.filter(request => request.host !== "assetdelivery.roblox.com").every(request => request.cookie === undefined), "the CDN never receives the cookie");

	// Cached, or offline: no batch request
	requests = [];
	await resolveAll(["rbxassetid://11", "rbxassetid://12"]);
	await resolveAll(["rbxassetid://21", "rbxassetid://22"], { offline: true });
	assert.equal(requests.length, 0);
	console.log("Asset batch: located downloads, CSRF, cookie isolation, fallbacks, cache and offline passed.");
} finally {
	https.get = originalGet;
	https.request = originalRequest;
	for (const [name, value] of previousEnv) {
		if (value === undefined) delete process.env[name];
		else process.env[name] = value;
	}
	assert.equal(path.dirname(path.resolve(temporary)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(temporary).startsWith("rur-asset-batch-test-"));
	fs.rmSync(temporary, { recursive: true, force: true });
}
