// Offline cookie configuration checks. All credentials and HTTP responses here are synthetic.
import assert from "node:assert/strict";
import fs from "node:fs";
import https from "node:https";
import os from "node:os";
import path from "node:path";
import { EventEmitter } from "node:events";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "rur-cookie-test-"));
const repo = path.join(temporary, "checkout", "renderer");
const localEnv = path.join(repo, ".env");
const overrideEnv = path.join(temporary, "override.env");
const cache = path.join(temporary, "cache");
const names = ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY", "RUR_COOKIE_ENV", "RUR_PROJECT", "RUR_CACHE_DIR", "RUR_STUDIO_MCP_URL", "RUR_STUDIO_MCP_PORT"];
const previousEnv = new Map(names.map(name => [name, process.env[name]]));
const previousCwd = process.cwd();
const originalGet = https.get;
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==", "base64");
let responses = [], requests = [], sequence = 0;

function write(file, value) {
	fs.mkdirSync(path.dirname(file), { recursive: true });
	fs.writeFileSync(file, value);
}

async function load({ env = {}, local, override, legacy, sibling } = {}) {
	for (const name of names) delete process.env[name];
	process.env.RUR_CACHE_DIR = cache;
	Object.assign(process.env, env);
	for (const [file, content] of [[localEnv, local], [overrideEnv, override]]) {
		if (content === undefined) fs.rmSync(file, { force: true });
		else write(file, content);
	}
	return import(`${pathToFileURL(path.join(repo, "cli", "assets.mjs"))}?case=${++sequence}`);
}

async function checkCookie(assets, expected) {
	requests = [];
	responses = [{ status: 403 }, ...(expected ? [
		{ body: JSON.stringify({ location: "https://cdn.example/image" }) },
		{ status: 302, headers: { location: "https://other.example/image" } },
		{ body: png },
	] : [])];
	const result = await assets.resolveUri(`rbxassetid://${sequence}`);
	assert.equal(assets.hasCookie(), Boolean(expected));
	assert.equal(requests.length, expected ? 4 : 1);
	assert.equal(requests[0].cookie, undefined, "public delivery is attempted first");
	if (expected) {
		assert.equal(requests[1].host, "assetdelivery.roblox.com");
		assert.equal(requests[1].cookie, `.ROBLOSECURITY=${expected}`);
		assert.ok(requests.slice(2).every(request => request.cookie === undefined), "CDN and redirects must receive no cookie");
		assert.equal(result.mime, "image/png");
		assert.ok(!JSON.stringify(result).includes(expected), "result metadata must not contain the cookie");
		assert.deepEqual(fs.readFileSync(result.file), png, "only asset bytes enter the download cache");
	} else {
		assert.match(result.error, /HTTP 403/);
	}
	assert.equal(responses.length, 0);
}

https.get = (url, options, callback) => {
	requests.push({ host: new URL(url).hostname, cookie: options.headers.Cookie });
	const next = responses.shift();
	assert.ok(next, "unexpected mocked HTTP request");
	const request = new EventEmitter();
	request.destroy = error => request.emit("error", error);
	queueMicrotask(() => {
		const response = new EventEmitter();
		response.statusCode = next.status || 200;
		response.headers = next.headers || {};
		response.resume = () => {};
		callback(response);
		response.emit("data", Buffer.isBuffer(next.body) ? next.body : Buffer.from(next.body || ""));
		response.emit("end");
	});
	return request;
};

try {
	for (const name of ["assets.mjs", "paths.mjs"]) write(path.join(repo, "cli", name), fs.readFileSync(path.join(root, "cli", name)));
	process.chdir(temporary); // The repo's .env must work from another working directory.
	await checkCookie(await load({ local: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local # comment\n" }), "dummy-local");
	await checkCookie(await load({ local: 'export ROBLOX_ASSET_DELIVERY_COOKIE=".ROBLOSECURITY=dummy-quoted" # comment\r\n' }), "dummy-quoted");
	await checkCookie(await load({ local: "ROBLOSECURITY='dummy-alias'\n" }), "dummy-alias");
	await checkCookie(await load({ local: 'ROBLOX_ASSET_DELIVERY_COOKIE="dummy-$(literal)-${UNEXPANDED}"\n' }), "dummy-$(literal)-${UNEXPANDED}");
	await checkCookie(await load({
		env: { ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-inherited", ROBLOSECURITY: "dummy-inherited-alias", RUR_COOKIE_ENV: overrideEnv },
		local: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local", override: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override",
	}), "dummy-local");
	await checkCookie(await load({ env: { ROBLOSECURITY: ".ROBLOSECURITY=dummy-inherited-alias" }, local: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local\r\n" }), "dummy-local");
	await checkCookie(await load({ env: { ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-inherited" }, local: "ROBLOSECURITY='dummy-local-alias'\n" }), "dummy-local-alias");
	await checkCookie(await load({ env: { RUR_COOKIE_ENV: overrideEnv }, override: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override", local: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-local" }), "dummy-local");
	await checkCookie(await load({ env: { ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-inherited", ROBLOSECURITY: "dummy-inherited-alias", RUR_COOKIE_ENV: overrideEnv }, override: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override" }), "dummy-inherited");
	await checkCookie(await load({ env: { ROBLOSECURITY: ".ROBLOSECURITY=dummy-inherited-alias" } }), "dummy-inherited-alias");
	await checkCookie(await load({ env: { ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-inherited" }, local: "ROBLOX_ASSET_DELIVERY_COOKIE=\r\nROBLOSECURITY=''\r\n" }), "dummy-inherited");
	await checkCookie(await load({ env: { ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-inherited" }, local: "# cookie not configured\nOTHER_OPTION=dummy-unrelated\n" }), "dummy-inherited");
	await checkCookie(await load({ env: { ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-inherited" }, local: 'ROBLOX_ASSET_DELIVERY_COOKIE="unterminated\n' }), "dummy-inherited");
	await checkCookie(await load({ env: { ROBLOX_ASSET_DELIVERY_COOKIE: "", ROBLOSECURITY: "dummy-inherited-alias", RUR_COOKIE_ENV: overrideEnv }, override: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override" }), "dummy-inherited-alias");
	await checkCookie(await load({ env: { RUR_COOKIE_ENV: overrideEnv }, override: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-override" }), "dummy-override");
	await checkCookie(await load(), null);
	const cached = await load({ local: "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-cached" });
	assert.equal(cached.hasCookie(), true);
	write(localEnv, "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-changed");
	await checkCookie(cached, "dummy-cached");
	const absent = await load();
	assert.equal(absent.hasCookie(), false);
	write(localEnv, "ROBLOX_ASSET_DELIVERY_COOKIE=dummy-added");
	await checkCookie(absent, null);
	console.log(`Asset cookie: ${sequence} offline cases passed (precedence, parsing, working directory, caching, and header isolation).`);
} finally {
	https.get = originalGet;
	process.chdir(previousCwd);
	for (const [name, value] of previousEnv) {
		if (value === undefined) delete process.env[name];
		else process.env[name] = value;
	}
	assert.equal(path.dirname(path.resolve(temporary)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(temporary).startsWith("rur-cookie-test-"));
	fs.rmSync(temporary, { recursive: true, force: true });
}
