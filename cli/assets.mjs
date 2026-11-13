// Asset resolution + download cache.
//
// rbxasset://      → the local Roblox Studio install (content/, ExtraContent/, PlatformContent/pc/)
// rbxassetid://N   → assetdelivery.roblox.com (public route first, then the account cookie), then an open
//                    Studio for an image asset delivery refuses (cli/studio-images.mjs, when a bridge is set)
// rbxthumb://...   → thumbnails.roblox.com
//
// The account cookie comes from this repo's .env, the process environment, or the file RUR_COOKIE_ENV
// names (ROBLOX_ASSET_DELIVERY_COOKIE or ROBLOSECURITY), in that order. It is only
// ever placed in the Cookie header of requests to https://assetdelivery.roblox.com and is never
// printed, logged, cached or written to disk. CDN downloads are fetched without it.

import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import https from "node:https";
import zlib from "node:zlib";
import { spawnSync } from "node:child_process";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { stateDir } from "./paths.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// The state dir (cli/paths.mjs: RUR_STATE_DIR, alias RUR_CACHE_DIR, default <renderer>/.cache)
export const CACHE_DIR = stateDir();
const ASSET_DIR = path.join(CACHE_DIR, "assets");
const DELIVERY_HOST = "assetdelivery.roblox.com";
const USER_AGENT = "rur/0.1 (offline Roblox UI renderer)";

let cookieCache;

function cookieValue(raw) {
	let value = raw.trim();
	if (value.startsWith('"') || value.startsWith("'")) {
		const end = value.indexOf(value[0], 1);
		if (end < 0) return null;
		value = value.slice(1, end);
	} else {
		value = value.split("#", 1)[0].trim();
	}
	return value.replace(/^\.ROBLOSECURITY=/, "") || null;
}

function cookieFromFile(file) {
	let text;
	try {
		text = fs.readFileSync(file, "utf8");
	} catch {
		return null;
	}
	for (const line of text.split(/\r?\n/)) {
		const match = /^\s*(?:export\s+)?(ROBLOX_ASSET_DELIVERY_COOKIE|ROBLOSECURITY)\s*=\s*(.*)$/.exec(line);
		if (!match) continue;
		const value = cookieValue(match[2]);
		if (value) return value;
	}
	return null;
}

function loadCookie() {
	if (cookieCache !== undefined) return cookieCache;
	cookieCache = cookieFromFile(path.join(ROOT, ".env"));
	if (cookieCache) return cookieCache;
	for (const name of ["ROBLOX_ASSET_DELIVERY_COOKIE", "ROBLOSECURITY"]) {
		const value = cookieValue(process.env[name] || "");
		if (value) return cookieCache = value;
	}
	const candidates = [
		process.env.RUR_COOKIE_ENV,
	].filter(Boolean);
	for (const file of candidates) {
		const value = cookieFromFile(file);
		if (value) return cookieCache = value;
	}
	return cookieCache;
}

export function hasCookie() {
	return Boolean(loadCookie());
}

// The environment without the asset cookie variables (any case), for child processes that never download
// assets: cargo and its build scripts, git, PowerShell, browsers. The render service and the asset helper
// still inherit it, because they download.
export function withoutCookie(env = process.env) {
	const copy = { ...env };
	for (const key of Object.keys(copy)) if (/^(?:ROBLOX_ASSET_DELIVERY_COOKIE|ROBLOSECURITY)$/i.test(key)) delete copy[key];
	return copy;
}

function request(url, headers = {}, redirects = 5) {
	return new Promise((resolve, reject) => {
		const req = https.get(url, { headers: { "User-Agent": USER_AGENT, ...headers }, timeout: 30000 }, response => {
			const status = response.statusCode || 0;
			if (status >= 300 && status < 400 && response.headers.location && redirects > 0) {
				response.resume();
				// Never forward headers (and so never the cookie) across a redirect.
				resolve(request(new URL(response.headers.location, url).toString(), {}, redirects - 1));
				return;
			}
			const chunks = [];
			response.on("data", chunk => chunks.push(chunk));
			response.on("end", () => resolve({ status, headers: response.headers, body: Buffer.concat(chunks) }));
			response.on("error", reject);
		});
		req.on("timeout", () => req.destroy(new Error(`timeout fetching ${new URL(url).host}`)));
		req.on("error", reject);
	});
}

function deliveryHeaders(withCookie) {
	if (!withCookie) return {};
	const cookie = loadCookie();
	return cookie ? { Cookie: `.ROBLOSECURITY=${cookie}` } : null;
}

async function deliveryLocation(assetId) {
	const routes = [false, true];
	let lastStatus = 0;
	let lastError = null;
	let cookieTried = false;
	for (const withCookie of routes) {
		const headers = deliveryHeaders(withCookie);
		if (headers === null) continue;
		if (withCookie) cookieTried = true;
		lastError = null;
		const url = `https://${DELIVERY_HOST}/v1/assetId/${assetId}`;
		let response;
		try {
			response = await request(url, headers, 0);
		} catch (error) {
			lastStatus = -1;
			continue;
		}
		lastStatus = response.status;
		try {
			const payload = JSON.parse(response.body.toString("utf8"));
			if (response.status >= 200 && response.status < 300 && payload && typeof payload.location === "string") return payload.location;
			const first = payload && Array.isArray(payload.errors) ? payload.errors[0] : null;
			if (first && typeof first === "object") lastError = first;
		} catch {}
	}
	if (lastError) {
		const detail = [lastError.code, lastError.message].filter(part => part !== undefined && part !== "").join(": ");
		const hint = (lastError.code === 401 || lastError.code === 403) && !cookieTried ? "; private asset, no asset cookie is configured" : "";
		throw new Error(`asset ${assetId} unavailable from asset delivery (${detail || `HTTP ${lastStatus}`}${hint})`);
	}
	throw new Error(`asset ${assetId} unavailable from asset delivery (HTTP ${lastStatus})`);
}

function post(url, body, headers) {
	return new Promise((resolve, reject) => {
		const data = Buffer.from(body);
		const req = https.request(url, { method: "POST", headers: { "User-Agent": USER_AGENT, "Content-Type": "application/json", "Content-Length": data.length, ...headers }, timeout: 30000 }, response => {
			const chunks = [];
			response.on("data", chunk => chunks.push(chunk));
			response.on("end", () => resolve({ status: response.statusCode || 0, headers: response.headers, body: Buffer.concat(chunks) }));
			response.on("error", reject);
		});
		req.on("timeout", () => req.destroy(new Error(`timeout fetching ${new URL(url).host}`)));
		req.on("error", reject);
		req.end(data);
	});
}

// Where many assets are, from asset delivery's batch endpoint: up to 100 per request, with the cookie
// when there is one (so a private asset costs no second trip). Through deliveryLocation each asset costs
// a round trip (two for a private one) before its download, so a UI of many images waited on lookups.
// An id it does not place is left out; its download asks deliveryLocation, which reports why.
async function deliveryLocations(ids) {
	const locations = new Map();
	const headers = deliveryHeaders(true) || {};
	const chunks = [];
	for (let index = 0; index < ids.length; index += 100) chunks.push(ids.slice(index, index + 100));
	await Promise.all(chunks.map(async chunk => {
		// assetId as written: ids are digit strings, and some are past a double's integers
		const body = `[${chunk.map((id, index) => `{"requestId":"${index}","assetId":${id}}`).join(",")}]`;
		const url = `https://${DELIVERY_HOST}/v1/assets/batch`;
		try {
			let response = await post(url, body, headers);
			// A request with the cookie is answered once with the CSRF token it must carry
			const token = response.headers["x-csrf-token"];
			if (response.status === 403 && token) response = await post(url, body, { ...headers, "X-CSRF-TOKEN": token });
			if (response.status !== 200) return;
			const replies = JSON.parse(response.body.toString("utf8"));
			for (const [position, reply] of (Array.isArray(replies) ? replies : []).entries()) {
				const id = chunk[Number(reply?.requestId ?? position)];
				if (id && typeof reply.location === "string" && reply.location) locations.set(id, reply.location);
			}
		} catch {}
	}));
	return locations;
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

// Asset delivery serves some files gzip-wrapped without a Content-Encoding header (body starts 1f 8b)
function unwrap(buffer) {
	if (buffer.length >= 2 && buffer[0] === 0x1f && buffer[1] === 0x8b) {
		try {
			return zlib.gunzipSync(buffer);
		} catch {
			return buffer;
		}
	}
	return buffer;
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

// What rur downloaded under each cache name: assets/downloaded/<file name>.json, the bytes' sha256 and
// size, written with every download. A file under an asset's name is not that asset because it is
// there: the cache is a plain folder, and on 2026-09-30 agents filled it with stand-ins for assets this
// renderer could not download (thumbnails.roblox.com 700 x 700 and 420 x 420 renditions cropped to their
// alpha bounds, and render directories' copies of them), which every later render drew as the asset (a
// header plate 7% narrower for its height, its art out of place). A cached file without a matching
// record is downloaded again before it is used and replaced when its bytes differ; offline it is used as
// it is. native/src/resolve.rs checks the same record.
const RECORD_DIR = path.join(ASSET_DIR, "downloaded");
const sha256 = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
const recordFile = file => path.join(RECORD_DIR, `${path.basename(file)}.json`);

function recordDownload(file, body, extra = {}) {
	fs.mkdirSync(RECORD_DIR, { recursive: true });
	writeAtomic(recordFile(file), JSON.stringify({ sha256: sha256(body), size: body.length, ...extra }));
}

// An image Studio decoded for rur (cli/studio-images.mjs), stored as assets/<id>.png. Not a stand-in: its
// pixels are the asset's own, as Studio loads them, re-encoded losslessly. Its record says
// "source": "studio", so the cache can tell it from a download (`rur assets warm --refresh` reads it again).
export function storeStudioImage(assetId, png) {
	const id = String(assetId).replace(/\D/g, "");
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

// Cached font files, from the directory itself (the index can lag behind it)
function cachedFonts() {
	try {
		return fs.readdirSync(ASSET_DIR).filter(name => /^\d+\.(ttf|otf|ttc)$/i.test(name));
	} catch {
		return [];
	}
}

// The cloud-font index (.cache/assets/fonts.json) the runtime preloads. Several processes download at once (a
// parallel sweep), so it is merged with the current file and the directory right before writing and written
// through a temporary file + rename: a reader never sees a half-written file, and a writer never drops the
// entries another process added (a read of a half-written index once reset it to a single entry).
function recordFont(file) {
	const index = path.join(ASSET_DIR, "fonts.json");
	const name = path.basename(file);
	let list = [];
	try {
		list = JSON.parse(fs.readFileSync(index, "utf8"));
	} catch {}
	if (!Array.isArray(list)) {
		list = [];
	}
	if (list.includes(name)) {
		return;
	}
	const merged = [...new Set([...list, ...cachedFonts(), name])];
	const temporary = `${index}.${process.pid}.${Date.now()}.tmp`;
	try {
		fs.writeFileSync(temporary, JSON.stringify(merged, null, 2));
		fs.renameSync(temporary, index);
	} catch {
		fs.rmSync(temporary, { force: true });
	}
}

const inflight = new Map();

// Downloads (or reads from cache) an asset id. Returns { file, kind, mime } or throws. `location`: where
// deliveryLocations found it, tried before asking asset delivery again.
export async function fetchAssetId(assetId, { allowDecal = true, offline = false, location } = {}) {
	const id = String(assetId).replace(/\D/g, "");
	if (!id) throw new Error(`invalid asset id ${assetId}`);
	let cached = cachedFile(id);
	if (cached && cached.endsWith(".bin")) {
		// Cached before gzip bodies were unwrapped: store the real file under its own extension
		const body = unwrap(fs.readFileSync(cached));
		const info = sniff(body);
		if (info.ext !== ".bin") {
			const file = path.join(ASSET_DIR, `${id}${info.ext}`);
			writeAtomic(file, body);
			fs.rmSync(cached, { force: true });
			if (info.kind === "font") recordFont(file);
			cached = file;
		}
	}
	const cachedBytes = cached ? fs.readFileSync(cached) : null;
	if (cached && (offline || downloaded(cached, cachedBytes))) {
		const info = sniff(cachedBytes);
		// A cached font that dropped out of the index comes back
		if (info.kind === "font") recordFont(cached);
		if (info.kind === "model" && allowDecal) return resolveModelTexture(id, cached, offline);
		return { file: cached, ...info };
	}
	if (offline) throw new Error("offline and not cached");
	if (inflight.has(id)) return inflight.get(id);

	const work = (async () => {
		let body;
		try {
			let download = location && await request(location, {}).catch(() => null);
			if (!download || download.status < 200 || download.status >= 300) download = await request(await deliveryLocation(id), {});
			if (download.status < 200 || download.status >= 300) throw new Error(`asset ${id} download failed (HTTP ${download.status})`);
			body = unwrap(download.body);
		} catch (error) {
			if (cached) error.message += `; the cached ${path.basename(cached)} is not rur's download of it (no matching record in assets/downloaded) and is not used`;
			throw error;
		}
		const info = sniff(body);
		fs.mkdirSync(ASSET_DIR, { recursive: true });
		const file = path.join(ASSET_DIR, `${id}${info.ext}`);
		// The same bytes keep their file (and render directories' links to it); other bytes replace it
		if (cached !== file || sha256(cachedBytes) !== sha256(body)) writeAtomic(file, body);
		if (cached && cached !== file) fs.rmSync(cached, { force: true });
		recordDownload(file, body);
		if (info.kind === "font") recordFont(file);
		if (info.kind === "model" && allowDecal) return resolveModelTexture(id, file);
		return { file, ...info };
	})();
	inflight.set(id, work);
	try {
		return await work;
	} finally {
		inflight.delete(id);
	}
}

// A decal/texture model asset (e.g. a Decal id used as an image) → the image it references.
async function resolveModelTexture(id, file, offline = false) {
	const text = fs.readFileSync(file).toString("latin1");
	const match = /(?:rbxassetid:\/\/|[?&]id=)(\d+)/i.exec(text);
	if (!match || match[1] === id) throw new Error(`asset ${id} is a model without an image texture`);
	const resolved = await fetchAssetId(match[1], { allowDecal: false, offline });
	return { ...resolved, via: id };
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

export function resolveRbxasset(uri) {
	const relative = uri.replace(/^rbxasset:\/\//i, "");
	for (const root of studioContentRoots()) {
		for (const base of ["content", "ExtraContent", path.join("PlatformContent", "pc")]) {
			const file = path.join(root, base, relative);
			if (fs.existsSync(file)) return file;
		}
	}
	return null;
}

// WebP -> PNG (straight RGBA, lossless) through Python's Pillow (libwebp), which the glyph rasterizer already
// needs; null when that fails. The renderer decodes PNGs itself (exact texels, colour under transparent texels).
function webpToPng(webp, png) {
	const python = process.env.RUR_PYTHON || (process.platform === "win32" ? "python" : "python3");
	const script = "import sys\nfrom PIL import Image\nImage.open(sys.argv[1]).convert('RGBA').save(sys.argv[2])";
	// Converted beside the target and renamed over it (writeAtomic)
	const temporary = `${png}.${process.pid}.${Date.now()}.tmp.png`;
	const result = spawnSync(python, ["-c", script, webp, temporary], { encoding: "utf8" });
	if (result.status !== 0 || !fs.existsSync(temporary)) {
		fs.rmSync(temporary, { force: true });
		return null;
	}
	fs.renameSync(temporary, png);
	return png;
}

// Measured (fixtures/image-probe-thumb-blocked.luau, out/compare/image-probe-thumb-blocked, notes/images.md round 4):
// for a user thumbnail the service reports as "Blocked" (its imageUrl is the UnapprovedImage icon, which Studio's
// log reports as "HttpError: InvalidUrl") Studio draws a placeholder silhouette instead: AvatarHeadShot a 420 x 420
// grey head in a circle (the w=60 / 150 / 420 cells all show this one texture, drawn Pixelated: 100.0 / 99.99 /
// 99.96% of their pixels within 2), Avatar a 720 x 720 black R6 figure. Neither is in the Studio install nor
// served by the thumbnail API (every returnPolicy, format and isCircular gives the UnapprovedImage URL); Studio
// downloads them, so they are taken from its HTTP cache (%LOCALAPPDATA%/Roblox/rbx-storage, entries "RBXH" +
// header + body, body length at byte 25), identified by the body's MD5, and kept in .cache/assets. The "Error"
// state (no imageUrl) draws nothing, like Studio.
const BLOCKED_PLACEHOLDERS = {
	AvatarHeadShot: { md5: "2f598ef27b1dddc3c4556aba8c2101c1", length: 11315 },
	Avatar: { md5: "5c2d7bd09200921254a731f35c6594e1", length: 41357 },
};

// The UnapprovedImage icon (the PNG rendition the service's Blocked imageUrl serves)
const UNAPPROVED_IMAGE_MD5 = "4cbf45bd6a776b83a681b35f937d5641";

function blockedPlaceholder(type) {
	const known = BLOCKED_PLACEHOLDERS[type];
	if (!known) return null;
	const file = path.join(ASSET_DIR, `placeholder-${known.md5}.png`);
	if (fs.existsSync(file)) return fs.readFileSync(file);
	const storage = path.join(process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local"), "Roblox", "rbx-storage");
	let folders = [];
	try {
		folders = fs.readdirSync(storage);
	} catch {
		return null;
	}
	for (const folder of folders) {
		let names = [];
		try {
			names = fs.readdirSync(path.join(storage, folder));
		} catch {
			continue;
		}
		for (const name of names) {
			const entry = path.join(storage, folder, name);
			try {
				if (fs.statSync(entry).size !== known.length + 37) continue;
				const bytes = fs.readFileSync(entry);
				if (bytes.toString("latin1", 0, 4) !== "RBXH" || bytes.readUInt32LE(25) !== known.length) continue;
				const body = bytes.subarray(37);
				if (crypto.createHash("md5").update(body).digest("hex") !== known.md5) continue;
				fs.mkdirSync(ASSET_DIR, { recursive: true });
				writeAtomic(file, body);
				return body;
			} catch {
				continue;
			}
		}
	}
	return null;
}

async function fetchThumbnail(uri, { offline = false } = {}) {
	const params = new URLSearchParams(uri.replace(/^rbxthumb:\/\//i, ""));
	const type = params.get("type") || "Asset";
	const id = params.get("id");
	const w = params.get("w") || "150";
	const h = params.get("h") || "150";
	// Measured (fixtures/image-probe-thumb.luau, tools/analysis/image_thumb.py): Studio draws the thumbnail
	// service's WebP rendition (a 60 x 60 headshot's texels: 100% identical to the decoded WebP, 36% to the PNG
	// rendition, which is another encode); cached as PNG under a "-webp" key (older PNG-rendition files stay
	// unused)
	const key = `thumb-${type}-${id}-${w}x${h}-webp`.replace(/[^\w-]/g, "_");
	const cached = cachedFile(key);
	if (cached) {
		const bytes = fs.readFileSync(cached);
		// A cache entry written before the placeholder rule holds the UnapprovedImage icon the service returns
		// for a Blocked thumbnail; Studio draws the placeholder there (17 such entries were found in one cache;
		// fixtures/image-probe-thumb-blocked.luau Blocked150)
		if (crypto.createHash("md5").update(bytes).digest("hex") === UNAPPROVED_IMAGE_MD5) {
			const placeholder = blockedPlaceholder(type);
			if (placeholder) return { file: path.join(ASSET_DIR, `placeholder-${BLOCKED_PLACEHOLDERS[type].md5}.png`), ...sniff(placeholder) };
		}
		return { file: cached, ...sniff(bytes) };
	}
	if (offline) throw new Error("offline and not cached");

	const size = `${w}x${h}`;
	const endpoints = {
		Asset: `https://thumbnails.roblox.com/v1/assets?assetIds=${id}&size=${size}`,
		Avatar: `https://thumbnails.roblox.com/v1/users/avatar?userIds=${id}&size=${size}`,
		AvatarHeadShot: `https://thumbnails.roblox.com/v1/users/avatar-headshot?userIds=${id}&size=${size}`,
		AvatarBust: `https://thumbnails.roblox.com/v1/users/avatar-bust?userIds=${id}&size=${size}`,
		BadgeIcon: `https://thumbnails.roblox.com/v1/badges/icons?badgeIds=${id}&size=${size}`,
		GamePass: `https://thumbnails.roblox.com/v1/game-passes?gamePassIds=${id}&size=${size}`,
		GroupIcon: `https://thumbnails.roblox.com/v1/groups/icons?groupIds=${id}&size=${size}`,
		BundleThumbnail: `https://thumbnails.roblox.com/v1/bundles/thumbnails?bundleIds=${id}&size=${size}`,
		DeveloperProduct: `https://thumbnails.roblox.com/v1/developer-products/icons?developerProductIds=${id}&size=${size}`,
	};
	const endpoint = endpoints[type] || endpoints.Asset;
	const download = async format => {
		const meta = await request(`${endpoint}&format=${format}`, {});
		if (meta.status < 200 || meta.status >= 300) throw new Error(`thumbnail metadata failed (HTTP ${meta.status})`);
		const payload = JSON.parse(meta.body.toString("utf8"));
		const state = payload && payload.data && payload.data[0] && payload.data[0].state;
		if (state === "Blocked") {
			const placeholder = blockedPlaceholder(type);
			if (placeholder) return placeholder;
		}
		const imageUrl = payload && payload.data && payload.data[0] && payload.data[0].imageUrl;
		if (!imageUrl) throw new Error(`no thumbnail for ${type} ${id}`);
		const image = await request(imageUrl, {});
		if (image.status < 200 || image.status >= 300) throw new Error(`thumbnail download failed (HTTP ${image.status})`);
		const body = unwrap(image.body);
		if (sniff(body).kind !== "image") throw new Error(`invalid thumbnail image for ${type} ${id}`);
		return body;
	};
	fs.mkdirSync(ASSET_DIR, { recursive: true });
	// Some thumbnail endpoints reject WebP; that must still reach the PNG fallback.
	const webp = await download("Webp").catch(() => null);
	if (webp && sniff(webp).ext === ".webp") {
		const temporary = path.join(ASSET_DIR, `${key}.${process.pid}.${Date.now()}.webp`);
		fs.writeFileSync(temporary, webp);
		const png = webpToPng(temporary, path.join(ASSET_DIR, `${key}.png`));
		fs.rmSync(temporary, { force: true });
		if (png) return { file: png, ...sniff(fs.readFileSync(png)) };
	}
	// No WebP or no converter: the PNG rendition
	const body = await download("Png");
	const info = sniff(body);
	const file = path.join(ASSET_DIR, `${key}${info.ext}`);
	writeAtomic(file, body);
	return { file, ...info };
}

export function assetIdFromUri(uri) {
	const trimmed = String(uri).trim();
	let match = /^rbxassetid:\/\/(\d+)/i.exec(trimmed);
	if (match) return match[1];
	match = /roblox\.com\/asset\/?\?.*?\bid=(\d+)/i.exec(trimmed);
	if (match) return match[1];
	match = /^(\d+)$/.exec(trimmed);
	if (match) return match[1];
	return null;
}

// Resolves any content URI to a local file. Returns { file, mime } or { error }.
// A file:// URI's path (file:///C:/x and file://C:/x, %xx escapes decoded): the PNG snapshot the engine
// emulator writes of an EditableImage a script shows (resolve.rs file_path)
export function filePathFromUri(uri) {
	let rest = uri.slice("file://".length);
	if (/^\/[A-Za-z]:/.test(rest)) rest = rest.slice(1);
	return rest.replace(/%([0-9A-Fa-f]{2})/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)));
}

export async function resolveUri(uri, { offline = false, locations } = {}) {
	try {
		if (/^file:\/\//i.test(uri)) {
			const file = filePathFromUri(uri);
			return fs.statSync(file, { throwIfNoEntry: false })?.isFile() ? { file, ...sniff(fs.readFileSync(file)) } : { error: `no such file: ${uri}` };
		}
		if (/^rbxasset:\/\//i.test(uri)) {
			const file = resolveRbxasset(uri);
			return file ? { file, ...sniff(fs.readFileSync(file)) } : { error: `not in the local Studio install: ${uri}` };
		}
		if (/^rbxthumb:\/\//i.test(uri)) {
			return await fetchThumbnail(uri, { offline });
		}
		const id = assetIdFromUri(uri);
		if (id) {
			const cached = cachedFile(id);
			if (offline && !cached) return { error: "offline and not cached" };
			return await fetchAssetId(id, { offline, location: locations?.get(id) });
		}
		return { error: `unsupported content uri: ${uri}` };
	} catch (error) {
		return { error: String(error && error.message ? error.message : error) };
	}
}

// Whether fetchAssetId would download `id`: nothing cached under it, or a file rur did not download
function needsDownload(id) {
	const cached = cachedFile(id);
	return !cached || !downloaded(cached, fs.readFileSync(cached));
}

// Where the usable cache file of `id` came from: "studio" (storeStudioImage), "download", or null when
// there is none (nothing cached, or a file without its matching record)
export function cachedSource(assetId) {
	const id = String(assetId).replace(/\D/g, "");
	if (needsDownload(id)) return null;
	try {
		return JSON.parse(fs.readFileSync(recordFile(cachedFile(id)), "utf8")).source === "studio" ? "studio" : "download";
	} catch {
		return null;
	}
}

// Resolves many URIs with bounded concurrency; the asset ids to download are located in batches first.
// 16 at a time, as ror downloads: located, a download is one CDN round trip, and 40 images took 2.7 s
// 8 at a time, 1.7 s 16 at a time.
export async function resolveAll(uris, options = {}) {
	const results = new Map();
	const queue = [...new Set(uris)];
	const ids = options.offline ? [] : [...new Set(queue.map(assetIdFromUri).filter(id => id && needsDownload(id)))];
	const locations = ids.length > 1 ? await deliveryLocations(ids) : undefined;
	const workers = Array.from({ length: Math.min(16, queue.length) }, async () => {
		while (queue.length) {
			const uri = queue.shift();
			results.set(uri, await resolveUri(uri, { ...options, locations }));
		}
	});
	await Promise.all(workers);
	if (!options.offline) await readRefusedFromStudio(results);
	return results;
}

// Whether refused images are read from Studio: a Studio MCP bridge is configured (RUR_STUDIO_MCP_URL, or the
// TCP bridge's port) and RUR_STUDIO_IMAGES is not "0"
export function studioImagesEnabled(env = process.env) {
	return env.RUR_STUDIO_IMAGES !== "0" && Boolean(env.RUR_STUDIO_MCP_URL || Number(env.RUR_STUDIO_MCP_PORT));
}

// The asset ids asset delivery would not give (a private image), read from an open Studio when a bridge is
// configured (cli/studio-images.mjs); an id Studio cannot read either keeps its error, with Studio's reason
export async function readRefusedFromStudio(results, env = process.env) {
	const refused = [...results].filter(([uri, result]) => result.error && assetIdFromUri(uri) && !/without an image texture/.test(result.error));
	if (!refused.length || !studioImagesEnabled(env)) return;
	let readStudioImages;
	try {
		({ readStudioImages } = await import("./studio-images.mjs"));
	} catch (error) {
		// Studio's reader needs rur's npm packages: without them each image keeps asset delivery's error, with
		// the command that installs them, rather than Node's "Cannot find package"
		const { packageError } = await import("./packages.mjs");
		const reason = packageError(error);
		if (reason === error) throw error;
		for (const [uri, result] of refused) results.set(uri, { error: `${result.error}; not read from Studio: ${reason.message}` });
		return;
	}
	const { images, errors } = await readStudioImages([...new Set(refused.map(([uri]) => assetIdFromUri(uri)))], env);
	for (const [uri, result] of refused) {
		const id = assetIdFromUri(uri);
		if (images.has(id)) results.set(uri, storeStudioImage(id, images.get(id)));
		else if (errors.has(id)) results.set(uri, { error: `${result.error}; ${errors.get(id)}` });
	}
}
