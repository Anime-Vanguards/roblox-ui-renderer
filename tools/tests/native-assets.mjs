// The CLI's side of assets, which the engine downloads (native/src/download.rs, whose own checks are
// `cargo test --release download::`): `rur-native assets` answers the CLI (offline here, so no request goes
// out) and reports the account and the Open Cloud API key without their secrets; child processes that
// download nothing get no credentials. All credentials are synthetic.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { withoutCookie } from "../../cli/assets.mjs";
import { ensureEngine } from "../../cli/engine.mjs";

const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rur-native-assets-test-"));
const cache = path.join(directory, "assets");
fs.mkdirSync(path.join(cache, "downloaded"), { recursive: true });
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg==", "base64");
fs.writeFileSync(path.join(cache, "123.png"), png);
fs.writeFileSync(path.join(cache, "thumb-AvatarHeadShot-42-150x150-webp.png"), png);
fs.writeFileSync(path.join(cache, "999.rbxmx"), "<roblox><url>rbxassetid://124</url></roblox>");
fs.writeFileSync(path.join(cache, "77.ttf"), Buffer.from([0, 1, 0, 0, 1, 2, 3]));
const local = path.join(directory, "local image.png");
fs.writeFileSync(local, png);
const SECRETS = ["dummy-native-assets", "dummy-broker-token", "dummy-env-cookie", "dummy-file-cookie", "dummy-env-api-key", "dummy-file-api-key"];
const noSecret = (text, what) => {
	for (const secret of SECRETS) assert.ok(!text.includes(secret), `${what} must not contain a credential`);
};

// The engine for `rur-native assets` (RUR_NATIVE_ENGINE, as selftest sets it, else the current build)
const engine = await ensureEngine();

// `rur-native assets` with this environment only (no account variable or file of this machine's) over
// `repo`, a stand-in checkout whose .env a checkout build reads
const home = path.join(directory, "home");
const repo = path.join(directory, "checkout", "renderer");
fs.mkdirSync(home, { recursive: true });
fs.mkdirSync(repo, { recursive: true });
function assets(args, { env = {}, input = "" } = {}) {
	const clean = withoutCookie(process.env);
	for (const name of Object.keys(clean)) if (/^(?:ROBLOX_BROKER_|RUR_COOKIE_ENV$|RUR_PROJECT$|RUR_STATE_DIR$|RUR_CACHE_DIR$|RUR_STUDIO)/i.test(name)) delete clean[name];
	const result = spawnSync(engine, ["assets", ...args, "--repo", repo], {
		cwd: directory, encoding: "utf8", windowsHide: true, input, timeout: 60000,
		env: { ...clean, RUR_STATE_DIR: directory, USERPROFILE: home, HOME: home, ROBLOX_BROKER_CREDENTIALS: path.join(directory, "absent.json"), RUR_COOKIE_ENV: path.join(directory, "absent.env"), RUR_PROJECT: path.join(directory, "no-project"), RUR_STUDIO_IMAGES: "0", ...env },
	});
	assert.ok(result.status === 0 || result.status === 1, `rur-native assets ${args.join(" ")}: exit ${result.status} ${result.stderr}`);
	noSecret(result.stdout + result.stderr, "rur-native assets' output");
	return result;
}

try {
	// The account rur-native reports: which, never its value
	const access = env => JSON.parse(assets(["account", "--json"], { env }).stdout).access;
	assert.equal(access({}), null);
	assert.equal(access({ ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-env-cookie" }), "cookie");
	fs.writeFileSync(path.join(repo, ".env"), "ROBLOSECURITY=dummy-file-cookie\n");
	assert.equal(access({}), "cookie", "a checkout build reads its checkout's .env");
	fs.rmSync(path.join(repo, ".env"));
	const broker = { ROBLOX_BROKER_URL: "https://broker.example", ROBLOX_BROKER_TOKEN: "dummy-broker-token" };
	assert.equal(access({ ...broker, ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-env-cookie" }), "broker", "a broker comes before the cookie");
	assert.equal(access({ ...broker, ROBLOX_BROKER_URL: "http://broker.example", ROBLOX_ASSET_DELIVERY_COOKIE: "dummy-env-cookie" }), null, "a refused broker address does not fall back to the cookie");
	assert.equal(assets(["account"], { env: broker }).stdout.trim(), "broker");

	// The Open Cloud API key: from its variable or the RUR_COOKIE_ENV file, asked before the account, and
	// reported without its value; one that can't be a key is not used
	const account = env => JSON.parse(assets(["account", "--json"], { env }).stdout);
	const API_KEY = "ROBLOX_ASSET_DELIVERY_API_KEY";
	assert.deepEqual(account({}), { access: null, apiKey: false });
	assert.deepEqual(account({ [API_KEY]: "dummy-env-api-key" }), { access: null, apiKey: true });
	const cookieFile = path.join(directory, "cookie.env");
	fs.writeFileSync(cookieFile, ["ROBLOX_ASSET_DELIVERY_COOKIE=dummy-file-cookie", `${API_KEY}="dummy-file-api-key" # the key`, ""].join("\n"));
	assert.deepEqual(account({ RUR_COOKIE_ENV: cookieFile }), { access: "cookie", apiKey: true }, "the key beside the cookie in RUR_COOKIE_ENV's file");
	assert.equal(assets(["account"], { env: { RUR_COOKIE_ENV: cookieFile } }).stdout.trim(), "api key, then cookie");
	assert.equal(assets(["account"], { env: { [API_KEY]: "dummy-env-api-key" } }).stdout.trim(), "api key");
	assert.deepEqual(account({ [API_KEY]: "dummy env api key" }), { access: null, apiKey: false }, "a key with spaces is not used");
	assert.deepEqual(account({ ...broker, [API_KEY]: "dummy-env-api-key" }), { access: "broker", apiKey: true });
	fs.rmSync(cookieFile);

	// Offline resolution, as a render does it: the cache, thumbnails, local files, decals; no request
	const uris = ["rbxassetid://123", "rbxassetid://404", pathToFileURL(local).href, "rbxthumb://type=AvatarHeadShot&id=42&w=150&h=150", "rbxassetid://999"];
	const fetched = assets(["fetch", "--offline", "--json", "--stdin", uris[0]], { env: broker, input: uris.slice(1).join("\n") });
	assert.equal(fetched.status, 1, "some did not resolve");
	const rows = new Map(JSON.parse(fetched.stdout).map(({ uri, ...row }) => [uri, row]));
	assert.deepEqual([...rows.keys()], uris);
	const cached = rows.get("rbxassetid://123");
	assert.deepEqual([path.basename(cached.file), cached.kind, cached.mime, fs.readFileSync(cached.file).equals(png)], ["123.png", "image", "image/png", true]);
	assert.match(rows.get("rbxassetid://404").error, /offline and not cached/);
	assert.equal(path.resolve(rows.get(uris[2]).file), path.resolve(local));
	assert.equal(path.basename(rows.get(uris[3]).file), "thumb-AvatarHeadShot-42-150x150-webp.png", "cached thumbnails work offline");
	assert.match(rows.get("rbxassetid://999").error, /offline/, "nested decal images preserve offline mode");
	const fonts = assets(["fetch", "--offline", "--fonts", "--json", "77", "78"]);
	assert.deepEqual(JSON.parse(fonts.stdout).map(row => row.kind ?? "error"), ["font", "error"]);
	assert.ok(fs.readFileSync(path.join(cache, "fonts.json"), "utf8").includes("77.ttf"), "a cached font is listed in fonts.json");
	assert.match(assets(["fetch", "--offline", "rbxassetid://123"]).stdout, /^rbxassetid:\/\/123\t.*123\.png\r?\n$/);

	// Child processes that download nothing get neither the cookie, the token nor the API key
	assert.deepEqual(withoutCookie({ ROBLOX_BROKER_TOKEN: "a", Roblox_Broker_Token: "b", ROBLOSECURITY: "c", ROBLOX_ASSET_DELIVERY_API_KEY: "d", roblox_asset_delivery_api_key: "e", ROBLOX_BROKER_URL: "https://env.example", PATH: "kept" }), { ROBLOX_BROKER_URL: "https://env.example", PATH: "kept" });
	console.log("Native assets: rur-native assets reports the account and the API key without them and resolves offline (cache, thumbnails, files, decals, fonts) without a request; child environments carry no credential.");
} finally {
	assert.equal(path.dirname(path.resolve(directory)), path.resolve(os.tmpdir()));
	assert.ok(path.basename(directory).startsWith("rur-native-assets-test-"));
	fs.rmSync(directory, { recursive: true, force: true });
}
