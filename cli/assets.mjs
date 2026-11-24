// The asset cache, as the Node CLI sees it. Downloads, the account (cookie or account broker), the Open
// Cloud API key and the cache rules are the native engine's (native/src/download.rs): `rur-native assets
// fetch` resolves URIs for the CLI's tools, as a render does, and `rur-native assets account` says how
// private assets are asked for. What stays here: the state dir, the environment for child processes that
// download nothing, the cache's records (which file was rur's download, which was read from Studio) and
// the images an open Studio reads for a render (cli/studio-images.mjs, storeStudioImage).
//
// rbxasset://      → the local Roblox Studio install (content/, ExtraContent/, PlatformContent/pc/)
// rbxassetid://N   → assetdelivery.roblox.com (public route first; for a private asset Open Cloud's asset
//                    delivery with the API key, then the account), then an open Studio for an image
//                    asset delivery refuses (cli/studio-images.mjs, when a bridge is set). A pinned version,
//                    rbxassetid://N&version=V, is cached apart as N-vV (parseAssetUri, assetKey).
// rbxthumb://...   → thumbnails.roblox.com

import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import crypto from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { stateDir } from "./paths.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// The state dir (cli/paths.mjs: RUR_STATE_DIR, alias RUR_CACHE_DIR, default <renderer>/.cache)
export const CACHE_DIR = stateDir();
const ASSET_DIR = path.join(CACHE_DIR, "assets");

// The environment without the asset cookie variables, the broker token, the Open Cloud API key and the switch
// that reads Roblox Studio's account (ROR_STUDIO_COOKIE; any case), for child processes that never download
// assets: cargo and its build scripts, git, rustc, PowerShell, browsers, the sim, the selftest's checks. The
// render service and `rur-native assets` still inherit them, because they download.
export function withoutCookie(env = process.env) {
	const copy = { ...env };
	for (const key of Object.keys(copy)) if (/^(?:ROBLOX_ASSET_DELIVERY_COOKIE|ROBLOSECURITY|ROBLOX_BROKER_TOKEN|ROBLOX_ASSET_DELIVERY_API_KEY|ROR_STUDIO_COOKIE)$/i.test(key)) delete copy[key];
	return copy;
}

// `rur-native assets <args>` with `input` on stdin: its stdout, or an error with its last stderr line.
// `engine`: the binary to run, else the current build (cli/engine.mjs ensureEngine, which builds it).
async function nativeAssets(args, { input = "", engine } = {}) {
	if (!engine) {
		const { ensureEngine } = await import("./engine.mjs");
		engine = await ensureEngine();
	}
	return new Promise((resolve, reject) => {
		const child = spawn(engine, ["assets", ...args, "--repo", ROOT], { cwd: ROOT, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
		const out = [], err = [];
		child.stdout.on("data", chunk => out.push(chunk));
		child.stderr.on("data", chunk => err.push(chunk));
		child.on("error", reject);
		child.on("close", status => {
			const stdout = Buffer.concat(out).toString("utf8");
			// 0: everything resolved, 1: some did not (each says why)
			if (status === 0 || status === 1) return resolve(stdout);
			const stderr = Buffer.concat(err).toString("utf8").trim().split(/\r?\n/).pop();
			reject(new Error(`rur-native assets ${args[0]} failed (exit ${status})${stderr ? `: ${stderr}` : ""}`));
		});
		child.stdin.end(input);
	});
}

// Resolves content URIs to local files as a render does (the native engine's resolver): Map<uri, { file,
// kind, mime } | { error }>. Downloads what is missing unless `offline`; with `studio` (the default) the
// images asset delivery refuses are read from an open Studio when a bridge is configured. `fonts`: the
// URIs are cloud font asset ids (a run's missingFontAssets).
export async function resolveAll(uris, { offline = false, fonts = false, studio = true, engine } = {}) {
	const unique = [...new Set(uris.map(String))];
	const results = new Map();
	if (!unique.length) return results;
	const args = ["fetch", "--json", "--stdin", ...(offline ? ["--offline"] : []), ...(fonts ? ["--fonts"] : []), ...(studio ? [] : ["--no-studio"])];
	const rows = JSON.parse(await nativeAssets(args, { input: unique.join("\n"), engine }));
	for (const { uri, ...result } of rows) results.set(uri, result);
	return results;
}

// How private assets are asked for: { access, apiKey }. access: "broker" (which comes before any cookie),
// "cookie" or null (no account); apiKey: whether an Open Cloud API key is asked first. Undefined when there
// is no native build to ask (`rur doctor` before the first build). Never a value.
export async function accountAccess() {
	const { findEngine } = await import("./engine.mjs");
	const engine = findEngine();
	if (!engine) return undefined;
	const result = spawnSync(engine, ["assets", "account", "--json", "--repo", ROOT], { cwd: ROOT, encoding: "utf8", windowsHide: true });
	try {
		const { access, apiKey, source, studio } = JSON.parse(result.stdout);
		return { access: access ?? null, apiKey: apiKey === true, source: source ?? null, studio: studio ?? null };
	} catch {
		return undefined;
	}
}

export function sniff(buffer) {
	if (buffer.length >= 8 && buffer.readUInt32BE(0) === 0x89504e47) return { ext: ".png", kind: "image", mime: "image/png" };
	if (buffer.length >= 3 && buffer[0] === 0xff && buffer[1] === 0xd8 && buffer[2] === 0xff) return { ext: ".jpg", kind: "image", mime: "image/jpeg" };
	if (buffer.length >= 6 && buffer.subarray(0, 6).toString("ascii").startsWith("GIF8")) return { ext: ".gif", kind: "image", mime: "image/gif" };
	if (buffer.length >= 12 && buffer.subarray(0, 4).toString("ascii") === "RIFF" && buffer.subarray(8, 12).toString("ascii") === "WEBP") return { ext: ".webp", kind: "image", mime: "image/webp" };
	if (buffer.length >= 2 && buffer.subarray(0, 2).toString("ascii") === "BM") return { ext: ".bmp", kind: "image", mime: "image/bmp" };
	if (buffer.length >= 4) {
		const tag = buffer.subarray(0, 4).toString("latin1");
		if (tag === "\x00\x01\x00\x00" || tag === "true") return { ext: ".ttf", kind: "font", mime: "font/ttf" };
		if (tag === "OTTO") return { ext: ".otf", kind: "font", mime: "font/otf" };
		if (tag === "ttcf") return { ext: ".ttc", kind: "font", mime: "font/collection" };
	}
	const head = buffer.subarray(0, 64).toString("utf8").trimStart();
	if (head.startsWith("{") || head.startsWith("[")) return { ext: ".json", kind: "json", mime: "application/json" };
	if (head.startsWith("<roblox!")) return { ext: ".rbxm", kind: "model", mime: "application/octet-stream" };
	if (head.startsWith("<roblox") || head.startsWith("<?xml")) return { ext: ".rbxmx", kind: "model", mime: "application/xml" };
	return { ext: ".bin", kind: "unknown", mime: "application/octet-stream" };
}

// A cache file is replaced, never rewritten in place: render directories hard-link cache files
// (native/src/story_render.rs link_or_copy), and a link must keep the content it was made with.
function writeAtomic(file, body) {
	const temporary = `${file}.${process.pid}.${Date.now()}.tmp`;
	try {
		fs.writeFileSync(temporary, body);
		fs.renameSync(temporary, file);
	} catch (error) {
		fs.rmSync(temporary, { force: true });
		throw error;
	}
}

// What rur stored under each cache name: assets/downloaded/<file name>.json, the bytes' sha256 and size.
// A cached file without a matching record is downloaded again before it is used (native/src/resolve.rs
// downloaded, which says why).
const RECORD_DIR = path.join(ASSET_DIR, "downloaded");
const sha256 = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
const recordFile = file => path.join(RECORD_DIR, `${path.basename(file)}.json`);

function recordDownload(file, body, extra = {}) {
	fs.mkdirSync(RECORD_DIR, { recursive: true });
	writeAtomic(recordFile(file), JSON.stringify({ sha256: sha256(body), size: body.length, ...extra }));
}

// An image Studio decoded for rur (cli/studio-images.mjs), stored as assets/<key>.png (the id, or `<id>-v<version>`
// for a pinned version). Not a stand-in: its pixels are the asset's own, as Studio loads them, re-encoded
// losslessly. Its record says "source": "studio", so the cache can tell it from a download (`rur assets warm
// --refresh` reads it again).
export function storeStudioImage(assetId, png) {
	const id = keyOf(assetId);
	if (!id) throw new Error(`not an asset id: ${assetId}`);
	const cached = cachedFile(id);
	fs.mkdirSync(ASSET_DIR, { recursive: true });
	const file = path.join(ASSET_DIR, `${id}.png`);
	if (cached !== file || sha256(fs.readFileSync(cached)) !== sha256(png)) writeAtomic(file, png);
	if (cached && cached !== file) fs.rmSync(cached, { force: true });
	recordDownload(file, png, { source: "studio" });
	return { file, ...sniff(png) };
}

// Files that matched their record, by the size and modification time they had then
const verified = new Map();

function downloaded(file, bytes) {
	const stat = fs.statSync(file);
	const stamp = `${stat.size}:${stat.mtimeMs}`;
	if (verified.get(file) === stamp) return true;
	let record;
	try {
		record = JSON.parse(fs.readFileSync(recordFile(file), "utf8"));
	} catch {
		return false;
	}
	if (!record || record.size !== bytes.length || record.sha256 !== sha256(bytes)) return false;
	verified.set(file, stamp);
	return true;
}

function cachedFile(assetId) {
	if (!fs.existsSync(ASSET_DIR)) return null;
	for (const ext of [".png", ".jpg", ".gif", ".webp", ".bmp", ".ttf", ".otf", ".ttc", ".json", ".rbxm", ".rbxmx", ".bin"]) {
		const file = path.join(ASSET_DIR, `${assetId}${ext}`);
		if (fs.existsSync(file)) return file;
	}
	return null;
}

export function studioContentRoots() {
	const versions = path.join(process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local"), "Roblox", "Versions");
	const roots = [];
	if (process.env.RUR_ROBLOX_CONTENT) roots.push(path.dirname(process.env.RUR_ROBLOX_CONTENT));
	try {
		const entries = fs.readdirSync(versions)
			.map(name => path.join(versions, name))
			.filter(dir => fs.existsSync(path.join(dir, "RobloxStudioBeta.exe")))
			.sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs);
		roots.push(...entries);
	} catch {}
	return roots;
}

// The asset delivery asset a content URI names, as native/src/resolve.rs asset_ref reads it (both are checked
// against native/tests/fixtures/content-ids.json): null for text that is no asset id, { error } for a spelling a
// Roblox client loads nothing for, else { id, version } (digit strings; version null for the latest).
// `rbxassetid://N&version=V` is how Roblox saves a pinned version (docs/assets.md "Pinned versions").
export function parseAssetUri(uri) {
	const SCHEME = "rbxassetid://";
	const text = String(uri);
	const loadsNothing = why => ({ error: `a Roblox client loads nothing for this content id: ${why}` });
	if (text.startsWith(SCHEME)) {
		const rest = text.slice(SCHEME.length);
		if (rest.includes("#")) return loadsNothing("it holds a #");
		const amp = rest.indexOf("&");
		if (amp >= 0) return pinnedAsset(rest.slice(0, amp), rest.slice(amp + 1));
		const id = /^\d*/.exec(rest)[0];
		if (!id) return null;
		const after = rest[id.length];
		if (after === "?" || after === "/" || after === "%") return loadsNothing(`a ${after} follows its id`);
		return { id, version: null };
	}
	const trimmed = rustTrim(text);
	const lower = asciiLower(trimmed);
	if (lower.startsWith(SCHEME)) return loadsNothing(text.startsWith(trimmed) ? "its scheme is not in lower case" : "white space comes before its scheme");
	// /roblox\.com\/asset\/?\?.*?\bid=(\d+)/i
	const at = lower.indexOf("roblox.com/asset");
	if (at >= 0) {
		let rest = lower.slice(at + "roblox.com/asset".length);
		if (rest.startsWith("/")) rest = rest.slice(1);
		if (!rest.startsWith("?")) return null;
		const query = rest.slice(1);
		for (let search = 0, start; (start = query.indexOf("id=", search)) >= 0; search = start + 3) {
			const boundary = start === 0 || !/[A-Za-z0-9_]/.test(query[start - 1]);
			const id = /^\d*/.exec(query.slice(start + 3))[0];
			if (boundary && id) {
				const version = versionReader();
				for (const [key, value] of queryPairs(query)) if (asciiLower(key) === "version") version.read(value);
				return { id, version: version.version };
			}
		}
		return null;
	}
	return /^\d+$/.test(trimmed) ? { id: trimmed, version: null } : null;
}

// rbxassetid://N&<query> (resolve.rs pinned): the id with white space around it at most; `version` in any case
// (versionReader), a lowercase `id` holding an id names that asset, `assetversionid` in any case is refused
function pinnedAsset(idText, query) {
	let id = canonicalId(rustTrim(idText));
	if (!id) return { error: "a Roblox client loads nothing for this content id: no asset id comes before its &" };
	const version = versionReader();
	for (const [key, value] of queryPairs(query)) {
		const lower = asciiLower(key);
		if (lower === "assetversionid") return { error: "an assetversionid names an asset version by its own id, which rur does not fetch" };
		if (key === "id") id = (value !== null && canonicalId(value)) || id;
		else if (lower === "version") version.read(value);
	}
	return { id, version: version.version };
}

// The `key=value` pairs of a query in order, form-decoded (`+` a space, then %xx) and trimmed; no `=`: value null
function queryPairs(query) {
	const decode = part => rustTrim(percentDecode(part.replace(/\+/g, " ")));
	return query.split("&").filter(Boolean).map(pair => {
		const equals = pair.indexOf("=");
		return equals < 0 ? [decode(pair), null] : [decode(pair.slice(0, equals)), decode(pair.slice(equals + 1))];
	});
}

// resolve.rs percent_decode: %xx escapes of the UTF-8 bytes; a % without two hex digits stays
function percentDecode(text) {
	const bytes = Buffer.from(text, "utf8");
	const out = [];
	for (let index = 0; index < bytes.length; index++) {
		const hex = bytes[index] === 0x25 && index + 2 < bytes.length ? /^[0-9a-fA-F]{2}$/.exec(bytes.subarray(index + 1, index + 3).toString("latin1")) : null;
		if (hex) {
			out.push(parseInt(hex[0], 16));
			index += 2;
		} else out.push(bytes[index]);
	}
	return Buffer.from(out).toString("utf8");
}

// 1 to 20 digits, a u64 other than 0, without leading zeros (resolve.rs canonical_id)
function canonicalId(text) {
	if (!/^\d{1,20}$/.test(text)) return null;
	const id = BigInt(text);
	return id > 0n && id < 2n ** 64n ? String(id) : null;
}

// resolve.rs VersionReader: each `version` value that is an integer (a sign, then decimal digits or 0x and hex,
// within i64) replaces the one before, 0 or less the latest; the first that is none ends the reading
function versionReader() {
	const reader = {
		version: null,
		done: false,
		read(value) {
			if (reader.done) return;
			const match = value === null ? null : /^([+-]?)(?:0[xX]([0-9a-fA-F]+)|(\d+))$/.exec(value);
			if (!match) {
				reader.done = true;
				return;
			}
			const magnitude = match[2] !== undefined ? BigInt(`0x${match[2]}`) : BigInt(match[3]);
			const number = match[1] === "-" ? -magnitude : magnitude;
			if (number < -(2n ** 63n) || number > 2n ** 63n - 1n) reader.done = true;
			else reader.version = number > 0n ? String(number) : null;
		},
	};
	return reader;
}

// Rust's str::trim: Unicode White_Space at both ends (JavaScript's trim also takes the byte order mark)
const rustTrim = text => text.replace(/^[\p{White_Space}]+|[\p{White_Space}]+$/gu, "");
const asciiLower = text => text.replace(/[A-Z]/g, letter => letter.toLowerCase());

// An asset's name in the cache (resolve.rs AssetRef key): the id for the latest version, `<id>-v<version>` for a
// pinned one; and back to a content id that names it
export const assetKey = ({ id, version }) => (version ? `${id}-v${version}` : id);
export const isAssetKey = key => /^\d+(?:-v\d+)?$/.test(String(key));
export function assetKeyFromUri(uri) {
	const asset = parseAssetUri(uri);
	return asset && !asset.error ? assetKey(asset) : null;
}
export function assetKeyUri(key) {
	const [id, version] = String(key).split("-v");
	return version ? `rbxassetid://${id}&version=${version}` : `rbxassetid://${id}`;
}
// A cache key from a key, or from other text holding an id (its digits, as before pinned versions)
const keyOf = assetId => (isAssetKey(assetId) ? String(assetId) : String(assetId).replace(/\D/g, ""));

// Where the usable cache file of `key` (an id, or `<id>-v<version>`) came from: "studio" (storeStudioImage),
// "download", or null when there is none (nothing cached, or a file without its matching record)
export function cachedSource(assetId) {
	const id = keyOf(assetId);
	if (!id) return null;
	const cached = cachedFile(id);
	if (!cached || !downloaded(cached, fs.readFileSync(cached))) return null;
	try {
		return JSON.parse(fs.readFileSync(recordFile(cached), "utf8")).source === "studio" ? "studio" : "download";
	} catch {
		return null;
	}
}

// Whether refused images are read from Studio: a Studio MCP bridge is configured (RUR_STUDIO_MCP_URL, or the
// TCP bridge's port) and RUR_STUDIO_IMAGES is not "0" (native/src/download.rs studio_images_enabled)
export function studioImagesEnabled(env = process.env) {
	return env.RUR_STUDIO_IMAGES !== "0" && Boolean(env.RUR_STUDIO_MCP_URL || Number(env.RUR_STUDIO_MCP_PORT));
}

// Reads `ids` from an open Studio (cli/studio-images.mjs) and stores each image it gives in the cache:
// { files: Map<id, { file, kind, mime }>, errors: Map<id, reason> }. Without rur's npm packages every id's
// reason is the command that installs them, rather than Node's "Cannot find package".
export async function storeFromStudio(ids, env = process.env) {
	const files = new Map();
	let readStudioImages;
	try {
		({ readStudioImages } = await import("./studio-images.mjs"));
		const { images, errors } = await readStudioImages(ids, env);
		for (const [id, png] of images) files.set(id, storeStudioImage(id, png));
		return { files, errors };
	} catch (error) {
		const { packageError } = await import("./packages.mjs");
		const reason = packageError(error);
		if (reason === error) throw error;
		return { files, errors: new Map(ids.map(id => [id, `not read from Studio: ${reason.message}`])) };
	}
}

// The assets asset delivery would not give (a private image), read from an open Studio when a bridge is
// configured, by cache key (a pinned version as `<id>-v<version>`); one Studio cannot read either keeps its
// error, with Studio's reason. `results`: resolveAll's.
export async function readRefusedFromStudio(results, env = process.env) {
	const refused = [...results].filter(([uri, result]) => result.error && assetKeyFromUri(uri) && !/without an image texture/.test(result.error));
	if (!refused.length || !studioImagesEnabled(env)) return;
	const { files, errors } = await storeFromStudio([...new Set(refused.map(([uri]) => assetKeyFromUri(uri)))], env);
	for (const [uri, result] of refused) {
		const key = assetKeyFromUri(uri);
		if (files.has(key)) results.set(uri, files.get(key));
		else if (errors.has(key)) results.set(uri, { error: `${result.error}; ${errors.get(key)}` });
	}
}
