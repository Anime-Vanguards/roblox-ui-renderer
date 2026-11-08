// Client of the render service (`rur-native serve`, native/src/serve.rs): a warm process with the GPU
// device, the glyph rasterizer and standby VMs (engine + project loaded), so a render costs only the
// story's own work. Started (detached) on first use; it exits 30 s after its last render.
//
//   const client = await import("<renderer>/cli/client.mjs");
//   await client.render({ story, project, out, ... }, { env })   → { summary } | { error }
//   await client.sequence({ story, project, out, from, to, fps, crop, ... }, { env }) → { summary } | { error }
//   await client.stories(project, { env })                       → [{ file, place, key, slug }] | null
//   await client.stop({ env })
//
// `env` is the environment a newly started service gets (RUR_PROJECT, RUR_COOKIE_ENV, RUR_STATE_DIR...).
// render() always answers: without a usable service it renders in a process of its own (the same files
// and pixels). stories() answers null without a service (callers then run `rur stories`).
//
// One service per renderer checkout and state dir (cli/paths.mjs): its port is derived from both
// (servicePorts: 4180-4579, a few candidates in case one is taken), and a service is used only when
// its /health names the same root and state dir, so two checkouts never share code or caches. A service
// of another engine build, or started with other asset helper code (cli/assets.mjs,
// cli/native-assets.mjs), is replaced. RUR_SERVE_PORT=<port> pins the port. RUR_ISOLATED=1 /
// RUR_NO_SERVICE=1 (in `env` or this process; `--isolated` / `--no-service`) turn it off.

import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { serviceDisabled, stateDir } from "./paths.mjs";
import { ensureEngine, findEngine } from "./engine.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const BASE_PORT = 4180;
const PORT_SPAN = 400;
const CANDIDATES = 4;

// Environment variables the engine reads (native/src/serve.rs ENGINE_ENV): a render whose values
// differ from the service's runs in a process of its own
export const ENGINE_ENV = ["RUR_CAMERA", "RUR_CLOCK", "RUR_ENGINE_DUMP", "RUR_ENGINE_TIMINGS", "RUR_EPOCH", "RUR_FONT_ROOT", "RUR_HINTED_MAX_SIZE", "RUR_LAYOUT_STATS", "RUR_LUAU_PROFILE",
	"RUR_NATIVE_ASSET_MB", "RUR_ORDER", "RUR_PLACE_ID", "RUR_PLAYER", "RUR_PLAYER_DISPLAY", "RUR_PLAYER_ID", "RUR_PROFILE_CALLBACKS", "RUR_REAL_CLOCK", "RUR_ROBLOX_CONTENT",
	"RUR_ROTATION_CLIPPING", "RUR_VERBOSE"];

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

const normalize = dir => {
	const resolved = path.resolve(String(dir));
	return process.platform === "win32" ? resolved.toLowerCase() : resolved;
};
const sameDir = (a, b) => normalize(a) === normalize(b);

// The ports this checkout's service may use with `env`'s state dir, in order
export function servicePorts(env = process.env) {
	const explicit = Number(env.RUR_SERVE_PORT);
	if (explicit) return [explicit];
	const hash = Number.parseInt(createHash("sha256").update(`${normalize(ROOT)}\0${normalize(stateDir(env))}`).digest("hex").slice(0, 8), 16);
	return Array.from({ length: CANDIDATES }, (_, i) => BASE_PORT + ((hash + i * 97) % PORT_SPAN));
}

// The Node code the service runs (its asset helper): a service started with other code is replaced
function codeStamp() {
	return createHash("sha256").update(["assets.mjs", "native-assets.mjs", "viewports.mjs", "paths.mjs"].map(name => {
		try {
			const stat = fs.statSync(path.join(ROOT, "cli", name));
			return `${name}:${stat.size}:${stat.mtimeMs}`;
		} catch {
			return `${name}:-`;
		}
	}).join("|")).digest("hex").slice(0, 12);
}

// Whether a service's answer (every JSON answer carries root and stateDir) is this checkout's with env's state dir
export function ownsService(json, env = process.env) {
	return !!json && typeof json.root === "string" && sameDir(json.root, ROOT)
		&& typeof json.stateDir === "string" && sameDir(json.stateDir, stateDir(env));
}

// Whether the service runs the current engine build and asset helper code
function current(json) {
	const engine = findEngine();
	return json.service === "rur-serve" && typeof json.executable === "string" && !!engine && sameDir(json.executable, engine) && json.code === codeStamp();
}

function call(port, method, route, body, timeoutMs) {
	return new Promise((resolve, reject) => {
		const data = body === undefined ? null : Buffer.from(JSON.stringify(body));
		const headers = { "x-rur": "1" };
		if (data) {
			headers["content-type"] = "application/json";
			headers["content-length"] = data.length;
		}
		const request = http.request({ host: "127.0.0.1", port, method, path: route, headers, agent: false, timeout: timeoutMs }, response => {
			const chunks = [];
			response.on("data", chunk => chunks.push(chunk));
			response.on("end", () => {
				let json = null;
				try {
					json = JSON.parse(Buffer.concat(chunks).toString("utf8"));
				} catch {
					json = null;
				}
				resolve({ status: response.statusCode, json });
			});
			response.on("error", reject);
		});
		request.on("timeout", () => request.destroy(Object.assign(new Error("request timed out"), { code: "ETIMEDOUT" })));
		request.on("error", reject);
		request.end(data || undefined);
	});
}

// What answers on `port`: { free: true } when nothing listens, else { json } (null for a non-service)
export async function probe(port, timeoutMs = 2000) {
	try {
		const response = await call(port, "GET", "/health", undefined, timeoutMs);
		return { json: response.json && typeof response.json.root === "string" ? response.json : null, status: response.status };
	} catch (error) {
		return error.code === "ECONNREFUSED" ? { free: true } : { json: null };
	}
}

// Starts a detached service on `port` and waits until it answers
async function start(env, port, timeoutMs = 30000) {
	const engine = await ensureEngine();
	const child = spawn(engine, ["serve", "--repo", ROOT, "--port", String(port), "--code", codeStamp()], {
		cwd: ROOT, env: { ...env }, detached: true, stdio: "ignore", windowsHide: true,
	});
	child.on("error", () => {});
	child.unref();
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		await sleep(20);
		const found = await probe(port);
		if (found.json && found.json.service === "rur-serve") {
			return ownsService(found.json, env);
		}
		// It exits at once when the port is taken; whoever holds it did not answer as a service
		if (child.exitCode !== null) {
			return false;
		}
	}
	return false;
}

// Waits until a service that is shutting down has let go of the port
async function closed(port, timeoutMs = 5000) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		if ((await probe(port, 1000)).free) {
			return;
		}
		await sleep(20);
	}
}

const located = new Map();
const warned = new Set();

function notice(key, message) {
	if (!warned.has(key)) {
		warned.add(key);
		process.stderr.write(`rur: ${message}\n`);
	}
}

// The port of this checkout's current service for env's state dir, starting one when none runs (and
// replacing one of another engine build or helper code); null when every candidate port is taken by
// something else or the service does not start
async function locate(env, { startIfMissing = true } = {}) {
	const key = normalize(stateDir(env));
	const ports = servicePorts(env);
	const known = located.get(key);
	const order = known ? [known, ...ports.filter(port => port !== known)] : ports;
	const foreign = [];
	for (const port of order) {
		let found = await probe(port);
		if (!found.free && ownsService(found.json, env) && !current(found.json)) {
			if (!startIfMissing) return port;
			// Another engine build or helper code (or the retired Node service): replaced
			await call(port, "POST", "/shutdown", {}, 5000).catch(() => {});
			await closed(port);
			found = await probe(port);
		}
		if (found.free) {
			if (!startIfMissing) continue;
			if (await start(env, port)) {
				located.set(key, port);
				return port;
			}
			foreign.push(`${port} (did not start)`);
			continue;
		}
		if (ownsService(found.json, env)) {
			located.set(key, port);
			return port;
		}
		foreign.push(found.json ? `${port} (${found.json.root}, state ${found.json.stateDir || "<renderer>/.cache"})` : `${port} (another program)`);
	}
	located.delete(key);
	if (startIfMissing && foreign.length) {
		notice(`busy:${key}`, `no render service for ${ROOT} (state ${stateDir(env)}): port ${foreign.join(", ")}; running without it`);
	}
	return null;
}

export async function request(method, route, body, { env = process.env, timeoutMs = 600000 } = {}) {
	if (serviceDisabled(env)) return null;
	let port = await locate(env);
	for (let attempt = 0; port !== null && attempt < 2; attempt++) {
		let response;
		try {
			response = await call(port, method, route, body, timeoutMs);
		} catch (error) {
			if (attempt || (error.code !== "ECONNREFUSED" && error.code !== "ECONNRESET")) {
				return null;
			}
			// It exited between the check and the request (idle): start again
			port = await locate(env);
			continue;
		}
		return ownsService(response.json, env) ? response : null;
	}
	return null;
}

// The values of the engine's environment variables in `env` (unset or empty ones left out)
function engineEnv(env) {
	return Object.fromEntries(ENGINE_ENV.filter(name => env[name]).map(name => [name, String(env[name])]));
}

// One `rur-native render` in the service (cli/rur.mjs render): its result, or null when the service
// cannot be used for it (disabled, not started, another engine environment, a failure: the caller
// then renders in a process of its own)
export async function renderInService(argv, out, { png = true, offline = false, placeholders = false } = {}, { env = process.env } = {}) {
	const response = await request("POST", "/render", { argv, out, png, offline, placeholders, env: engineEnv(env) }, { env });
	if (!response) return null;
	if (response.status !== 200) {
		if (!response.json.envMismatch) notice(`failed:${response.json.error}`, `the render service failed (${String(response.json.error).split("\n")[0]}); rendering without it`);
		return null;
	}
	return response.json.result;
}

// One `rur-native sequence` in the service (cli/rur.mjs sequence): its result, or null when the service
// cannot be used for it (the caller then runs one in a process of its own)
export async function sequenceInService(argv, out, sequence, { offline = false } = {}, { env = process.env } = {}) {
	const response = await request("POST", "/sequence", { argv, out, offline, sequence, env: engineEnv(env) }, { env });
	if (!response) return null;
	if (response.status !== 200) {
		if (!response.json.envMismatch) notice(`failed:${response.json.error}`, `the render service failed (${String(response.json.error).split("\n")[0]}); capturing without it`);
		return null;
	}
	return response.json.result;
}

const PATH_KEYS = ["story", "module", "model", "fixture", "scene", "out", "snapshot-dir"];
const INLINE = { actions: "[", controls: "{" };

// Paths resolved against the caller's working directory
function absolute(args) {
	const result = { ...args };
	for (const key of PATH_KEYS) {
		if (typeof result[key] === "string") {
			result[key] = path.resolve(result[key]);
		}
	}
	for (const [key, inline] of Object.entries(INLINE)) {
		if (typeof result[key] === "string" && !result[key].trim().startsWith(inline)) {
			result[key] = path.resolve(result[key]);
		}
	}
	return result;
}

// `rur render` with the same options, as an object, through the service when it can be used
export async function render(args, { env = process.env } = {}) {
	const { render: renderStory } = await import("./rur.mjs");
	try {
		return { summary: await renderStory(absolute(args), { service: true, env }) };
	} catch (error) {
		return { error: String((error && error.message) || error) };
	}
}

// `rur sequence` with the same options, as an object, through the service when it can be used
export async function sequence(args, { env = process.env } = {}) {
	const { sequence: runSequence } = await import("./rur.mjs");
	try {
		return { summary: await runSequence(absolute(args), { service: true, env }) };
	} catch (error) {
		return { error: String((error && error.message) || error) };
	}
}

// `rur stories --place all` through the service
export async function stories(project, options = {}) {
	const response = await request("GET", `/stories?project=${encodeURIComponent(path.resolve(project))}`, undefined, options);
	return response && response.status === 200 ? response.json.stories : null;
}

// { port, pid, root, stateDir, code, executable, renders } of this checkout's running service for env's state dir, or null
export async function status({ env = process.env } = {}) {
	if (serviceDisabled(env)) return null;
	const port = await locate(env, { startIfMissing: false });
	if (port === null) return null;
	const found = await probe(port);
	return found.json ? { port, ...found.json } : null;
}

// Stops this checkout's service for env's state dir (never another checkout's)
export async function stop({ env = process.env } = {}) {
	if (serviceDisabled(env)) return;
	const port = await locate(env, { startIfMissing: false });
	if (port === null) return;
	try {
		await call(port, "POST", "/shutdown", {}, 5000);
	} catch {
		// not running
	}
}
