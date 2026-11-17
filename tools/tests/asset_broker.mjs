// An account broker (a service that holds the Roblox session) replaces the cookie: where its credentials
// come from, which requests go to it and what they carry, which addresses are refused, and how its own
// refusals read. All credentials, hosts and HTTP responses here are synthetic.
import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import os from "node:os";
import path from "node:path";
import { EventEmitter } from "node:events";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "rur-broker-test-"));
const repo = path.join(temporary, "renderer");
const home = path.join(temporary, "home");
const homeFile = path.join(home, ".config", "roblox-broker", "credentials.json");
const namedFile = path.join(temporary, "named.json");
const names = ["ROBLOX_BROKER_URL", "ROBLOX_BROKER_TOKEN", "ROBLOX_BROKER_CREDENTIALS", "ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY", "RUR_COOKIE_ENV", "RUR_PROJECT", "RUR_CACHE_DIR", "RUR_STATE_DIR", "RUR_STUDIO_MCP_URL", "RUR_STUDIO_IMAGES", "USERPROFILE", "HOME"];
const previousEnv = new Map(names.map(name => [name, process.env[name]]));
const originals = { http: [http.get, http.request], https: [https.get, https.request] };
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==", "base64");
const DELIVERY = "https://assetdelivery.roblox.com";
let requests = [], brokerAnswer = null, sequence = 0, nextId = 100;

function write(file, value) {
	fs.mkdirSync(path.dirname(file), { recursive: true });
	fs.writeFileSync(file, value);
}

// Asset delivery refuses every bare request (the assets are private); a broker places what it is asked
// for on the CDN unless the case gives its answer; the CDN serves the image, once through a redirect.
function answer(request) {
	const { host, pathname } = new URL(request.url);
	if (host === "assetdelivery.roblox.com") return { status: 401, body: JSON.stringify({ errors: [{ code: 0, message: "Authentication required to access Asset." }] }) };
	if (host === "cdn.example") return pathname.endsWith("7") ? { status: 302, headers: { location: `https://other.example${pathname}` } } : { body: png };
	if (host === "other.example") return { body: png };
	if (brokerAnswer) return brokerAnswer;
	if (request.method === "POST") return { body: JSON.stringify(JSON.parse(request.body).map(entry => ({ requestId: entry.requestId, location: `https://cdn.example/${entry.assetId}` }))) };
	return { body: JSON.stringify({ location: `https://cdn.example/${pathname.split("/").pop()}` }) };
}

for (const [scheme, client] of [["http:", http], ["https:", https]]) {
	const send = (method, url, options, callback) => {
		assert.equal(new URL(url).protocol, scheme, "each URL goes out through its own scheme's client");
		const record = { method, url: String(url), authorization: options.headers.Authorization, cookie: options.headers.Cookie, csrf: options.headers["X-CSRF-TOKEN"], type: options.headers["Content-Type"], body: "" };
		requests.push(record);
		const request = new EventEmitter();
		request.destroy = error => request.emit("error", error);
		request.end = data => { record.body = String(data || ""); };
		queueMicrotask(() => {
			const reply = answer(record);
			const response = new EventEmitter();
			response.statusCode = reply.status || 200;
			response.headers = reply.headers || {};
			response.resume = () => {};
			callback(response);
			response.emit("data", Buffer.isBuffer(reply.body) ? reply.body : Buffer.from(reply.body || ""));
			response.emit("end");
		});
		return request;
	};
	client.get = (url, options, callback) => send("GET", url, options, callback);
	client.request = (url, options, callback) => send(options.method, url, options, callback);
}

// A fresh copy of the asset module under this environment and these credentials files
async function load({ env = {}, named, saved } = {}) {
	for (const name of names) delete process.env[name];
	Object.assign(process.env, { RUR_STATE_DIR: path.join(temporary, "state"), RUR_COOKIE_ENV: path.join(temporary, "absent.env"), RUR_PROJECT: path.join(temporary, "no-project"), RUR_STUDIO_IMAGES: "0", USERPROFILE: home, HOME: home }, env);
	for (const [file, content] of [[namedFile, named], [homeFile, saved]]) {
		if (content === undefined) fs.rmSync(file, { force: true });
		else write(file, typeof content === "string" ? content : JSON.stringify(content));
	}
	requests = [];
	brokerAnswer = null;
	return import(`${pathToFileURL(path.join(repo, "cli", "assets.mjs"))}?case=${++sequence}`);
}

// One private asset through the broker at `base` with `token`: the bare attempt, the broker, the CDN
async function checkBroker(assets, base, token) {
	const id = nextId += 10;
	const result = await assets.resolveUri(`rbxassetid://${id}`);
	assert.equal(result.error, undefined);
	assert.deepEqual(fs.readFileSync(result.file), png);
	assert.deepEqual(requests.map(request => `${request.method} ${request.url}`), [
		`GET ${DELIVERY}/v1/assetId/${id}`,
		`GET ${base}/rbx/assetdelivery.roblox.com/v1/assetId/${id}`,
		`GET https://cdn.example/${id}`,
	]);
	assert.deepEqual(requests.map(request => request.authorization), [undefined, `Bearer ${token}`, undefined], "only the broker receives the token");
	assert.ok(requests.every(request => request.cookie === undefined), "no Cookie header in broker mode");
	assert.equal(assets.hasCookie(), true);
	assert.equal(assets.accountAccess(), "broker");
	assert.ok(!JSON.stringify(result).includes(token), "result metadata must not contain the token");
}

try {
	for (const name of ["assets.mjs", "paths.mjs"]) write(path.join(repo, "cli", name), fs.readFileSync(path.join(root, "cli", name)));
	const fromEnv = { ROBLOX_BROKER_URL: "https://env.example", ROBLOX_BROKER_TOKEN: "dummy-env-token" };
	const named = { url: "https://named.example/", token: "dummy-named-token", name: "ignored" };
	const saved = { url: "https://saved.example/base", token: "dummy-saved-token" };

	// Discovery: the environment pair, then the named file, then the one under the home folder; a cookie
	// that is also configured is never sent
	const cookie = { ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-cookie" };
	await checkBroker(await load({ env: { ...fromEnv, ...cookie, ROBLOX_BROKER_CREDENTIALS: namedFile }, named, saved }), "https://env.example", "dummy-env-token");
	await checkBroker(await load({ env: { ...cookie, ROBLOX_BROKER_CREDENTIALS: namedFile }, named, saved }), "https://named.example", "dummy-named-token");
	await checkBroker(await load({ env: cookie, named, saved }), "https://saved.example/base", "dummy-saved-token");
	// A token without an address (or the reverse) in the environment is not a pair: the file is used
	await checkBroker(await load({ env: { ROBLOX_BROKER_TOKEN: "dummy-env-token" }, saved }), "https://saved.example/base", "dummy-saved-token");
	await checkBroker(await load({ env: { ROBLOX_BROKER_URL: "https://env.example" }, saved }), "https://saved.example/base", "dummy-saved-token");
	// A named file that is absent or incomplete does not fall through to the home folder's
	for (const content of [undefined, "not json", { url: "https://named.example" }, { token: "dummy-named-token" }]) {
		const assets = await load({ env: { ROBLOX_BROKER_CREDENTIALS: namedFile }, named: content, saved });
		assert.equal(assets.accountAccess(), null);
		assert.equal(assets.hasCookie(), false);
	}
	// No broker: the cookie, as before
	const withCookie = await load({ env: cookie });
	assert.equal(withCookie.accountAccess(), "cookie");
	await withCookie.resolveUri("rbxassetid://91");
	assert.deepEqual(requests.slice(0, 2).map(request => [request.url, request.cookie, request.authorization]), [
		[`${DELIVERY}/v1/assetId/91`, undefined, undefined],
		[`${DELIVERY}/v1/assetId/91`, ".ROBLOSECURITY=dummy-cookie", undefined],
	]);

	// The token never follows a redirect, from the CDN or from the broker itself
	let assets = await load({ env: fromEnv });
	assert.deepEqual(fs.readFileSync((await assets.resolveUri("rbxassetid://97")).file), png);
	assert.deepEqual(requests.map(request => [new URL(request.url).host, request.authorization]), [
		["assetdelivery.roblox.com", undefined], ["env.example", "Bearer dummy-env-token"], ["cdn.example", undefined], ["other.example", undefined],
	]);
	assets = await load({ env: fromEnv });
	brokerAnswer = { status: 302, headers: { location: "https://other.example/stolen" } };
	assert.match((await assets.resolveUri("rbxassetid://98")).error, /HTTP 302/);
	assert.ok(requests.every(request => new URL(request.url).host !== "other.example"), "a redirect from the broker is not followed");

	// The batch lookup: one POST to the broker with the token, no cookie, and no CSRF round of its own
	assets = await load({ env: { ...fromEnv, ...cookie } });
	const results = await assets.resolveAll(["rbxassetid://201", "rbxassetid://202"]);
	for (const id of ["201", "202"]) assert.deepEqual(fs.readFileSync(results.get(`rbxassetid://${id}`).file), png);
	assert.deepEqual(requests.map(request => `${request.method} ${request.url}`).sort(), [
		"GET https://cdn.example/201",
		"GET https://cdn.example/202",
		"POST https://env.example/rbx/assetdelivery.roblox.com/v1/assets/batch",
	]);
	const batch = requests.find(request => request.method === "POST");
	assert.deepEqual([batch.authorization, batch.cookie, batch.csrf, batch.type], ["Bearer dummy-env-token", undefined, undefined, "application/json"]);
	assert.equal(batch.body, '[{"requestId":"0","assetId":201},{"requestId":"1","assetId":202}]');
	assert.ok(requests.filter(request => request !== batch).every(request => request.authorization === undefined && request.cookie === undefined));
	assets = await load({ env: fromEnv });
	brokerAnswer = { status: 403, headers: { "x-csrf-token": "dummy-csrf" } };
	await assets.resolveAll(["rbxassetid://211", "rbxassetid://212"]);
	assert.equal(requests.filter(request => request.method === "POST").length, 1, "the broker answers the CSRF challenge: the batch is not repeated");
	assert.ok(requests.every(request => request.csrf === undefined));

	// The address: https, or http to this machine only. A refused one is never contacted, says why, and
	// does not fall back to the cookie.
	await checkBroker(await load({ env: { ...fromEnv, ROBLOX_BROKER_URL: "http://127.0.0.1:18080/" } }), "http://127.0.0.1:18080", "dummy-env-token");
	await checkBroker(await load({ saved: { ...saved, url: "http://localhost:18080" } }), "http://localhost:18080", "dummy-saved-token");
	for (const [url, reason] of [["http://broker.example", /must be https \(http only for 127\.0\.0\.1 or localhost\), not http:\/\/broker\.example/], ["http://127.0.0.1.example", /must be https/], ["ftp://127.0.0.1", /must be https/], ["broker.example", /is not a URL/]]) {
		assets = await load({ env: { ...fromEnv, ...cookie, ROBLOX_BROKER_URL: url } });
		assert.equal(assets.accountAccess(), null);
		const refused = await assets.resolveUri("rbxassetid://301");
		assert.match(refused.error, reason);
		await assets.resolveAll(["rbxassetid://302", "rbxassetid://303"]);
		assert.ok(requests.length > 0 && requests.every(request => request.url.startsWith(`${DELIVERY}/v1/assetId/`)), "only the bare lookups go out");
		assert.ok(requests.every(request => request.authorization === undefined && request.cookie === undefined));
	}

	// The broker's own refusal reads as its message, not a bare status; Roblox's answer through it is Roblox's
	assets = await load({ env: fromEnv });
	brokerAnswer = { status: 503, headers: { "x-broker-error": "account_signed_out" }, body: JSON.stringify({ error: "account_signed_out", message: "the shared account is signed out; the owner has been alerted" }) };
	assert.match((await assets.resolveUri("rbxassetid://401")).error, /asset 401 unavailable from asset delivery \(the account broker refused: the shared account is signed out; the owner has been alerted\)/);
	brokerAnswer = { status: 401, headers: { "x-broker-error": "token_invalid" }, body: JSON.stringify({ error: "token_invalid", message: "token dummy-env-token is unknown or revoked" }) };
	const invalid = (await assets.resolveUri("rbxassetid://402")).error;
	assert.match(invalid, /the account broker refused: token <token> is unknown or revoked/);
	assert.ok(!invalid.includes("dummy-env-token"), "error text never contains the token");
	brokerAnswer = { status: 429, headers: { "x-broker-error": "rate_limited", "retry-after": "3" }, body: "not json" };
	assert.match((await assets.resolveUri("rbxassetid://403")).error, /the account broker refused: rate_limited \(HTTP 429\)/);
	brokerAnswer = { status: 403, body: JSON.stringify({ errors: [{ code: 403, message: "User is not authorized to access Asset." }] }) };
	assert.match((await assets.resolveUri("rbxassetid://404")).error, /asset 404 unavailable from asset delivery \(403: User is not authorized to access Asset\.\)$/);

	// Child processes that download nothing get neither the cookie nor the token
	assert.deepEqual(assets.withoutCookie({ ROBLOX_BROKER_TOKEN: "a", Roblox_Broker_Token: "b", ROBLOSECURITY: "c", ROBLOX_BROKER_URL: "https://env.example", PATH: "kept" }), { ROBLOX_BROKER_URL: "https://env.example", PATH: "kept" });
	console.log(`Asset broker: ${sequence} offline cases passed (discovery order, the single and batch routes, no cookie, redirects, the address rule, refusals and child environments).`);
} finally {
	[http.get, http.request] = originals.http;
	[https.get, https.request] = originals.https;
	for (const [name, value] of previousEnv) {
		if (value === undefined) delete process.env[name];
		else process.env[name] = value;
	}
	assert.equal(path.dirname(path.resolve(temporary)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(temporary).startsWith("rur-broker-test-"));
	fs.rmSync(temporary, { recursive: true, force: true });
}
