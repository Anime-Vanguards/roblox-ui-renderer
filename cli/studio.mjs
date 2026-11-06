// Roblox Studio bridge for parity checks: Studio MCP to run Luau, and lossless viewport capture
// (PrintWindow, physical pixels) through tools/wincap.
//
// The MCP side is a local TCP port speaking newline-delimited JSON-RPC (MCP) with the tools
// `list_roblox_studios` and `execute_luau` (docs/studio.md).
// RUR_STUDIO_MCP_PORT names the port. Runs use the Studio --studio names (an id or part of its
// window title), else RUR_STUDIO, else the only Studio open.
//
// Everything mounted in Studio lives under CoreGui (never saved with the place) and is removed
// after each comparison.

import net from "node:net";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { withoutCookie } from "./assets.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PORT = Number(process.env.RUR_STUDIO_MCP_PORT || 0);
// The Studio a run uses when it names none: an id or a substring of its window name ("": the only one open)
export const DEFAULT_STUDIO = process.env.RUR_STUDIO || "";

async function session(fn) {
	if (!PORT) throw new Error("Studio comparisons need a Studio MCP bridge: set RUR_STUDIO_MCP_PORT to its local port (docs/studio.md)");
	const socket = await new Promise((resolve, reject) => {
		const connection = net.connect(PORT, "127.0.0.1");
		connection.once("connect", () => resolve(connection));
		connection.once("error", reject);
	});
	let buffer = "";
	let nextId = 1;
	const waiters = new Map();
	socket.on("data", chunk => {
		buffer += chunk.toString("utf8");
		let index;
		while ((index = buffer.indexOf("\n")) >= 0) {
			const line = buffer.slice(0, index).trim();
			buffer = buffer.slice(index + 1);
			if (!line) continue;
			let message;
			try {
				message = JSON.parse(line);
			} catch {
				continue;
			}
			if (message.id !== undefined && waiters.has(message.id)) {
				const waiter = waiters.get(message.id);
				waiters.delete(message.id);
				waiter(message);
			}
		}
	});
	const request = (method, params, timeout = 300000) => new Promise((resolve, reject) => {
		const id = nextId++;
		const timer = setTimeout(() => {
			waiters.delete(id);
			reject(new Error(`Studio MCP timeout (${method})`));
		}, timeout);
		waiters.set(id, message => {
			clearTimeout(timer);
			if (message.error) reject(new Error(JSON.stringify(message.error)));
			else resolve(message.result);
		});
		socket.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
	});
	try {
		await request("initialize", { protocolVersion: "2024-11-05", clientInfo: { name: "rur", version: "0.1.0" }, capabilities: {} });
		socket.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
		return await fn(request);
	} finally {
		socket.destroy();
	}
}

function resultText(result) {
	return (result.content || []).map(item => item.text || "").join("");
}

export async function listStudios() {
	return session(async request => JSON.parse(resultText(await request("tools/call", { name: "list_roblox_studios", arguments: {} }))).studios || []);
}

async function studioId(request, query) {
	const studios = JSON.parse(resultText(await request("tools/call", { name: "list_roblox_studios", arguments: {} }))).studios || [];
	if (!query && studios.length !== 1) throw new Error(`${studios.length ? "several Studio instances are open" : "no Studio is open"} (open: ${studios.map(studio => studio.name ?? "(opening)").join(", ") || "none"}): name one with --studio or RUR_STUDIO`);
	const match = studios.find(studio => studio.id === query) || studios.find(studio => typeof studio.name === "string" && studio.name.includes(query)); // a Studio still opening has no name yet
	if (!match) throw new Error(`no Studio instance matches "${query}" (open: ${studios.map(studio => studio.name ?? "(opening)").join(", ")})`);
	return match;
}

// Runs Luau in Studio (Edit datamodel). Large results: the chunk returns a string, paged in base64.
export async function runLuau(code, { studio = DEFAULT_STUDIO } = {}) {
	return session(async request => {
		const target = await studioId(request, studio);
		const call = async luau => {
			const result = await request("tools/call", { name: "execute_luau", arguments: { studio_id: target.id, datamodel_type: "Edit", code: luau } });
			const text = resultText(result);
			if (result.isError) throw new Error(text);
			return text;
		};
		const wrapped = `local __rur_result = (function()\n${code}\nend)()
if type(__rur_result) ~= "string" then __rur_result = tostring(__rur_result) end
local __rur_alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
local __rur_bytes = buffer.fromstring(__rur_result)
local __rur_length = buffer.len(__rur_bytes)
local __rur_out = buffer.create(math.ceil(__rur_length / 3) * 4)
local __rur_write = 0
for __rur_i = 0, __rur_length - 1, 3 do
	local a = buffer.readu8(__rur_bytes, __rur_i)
	local b = if __rur_i + 1 < __rur_length then buffer.readu8(__rur_bytes, __rur_i + 1) else 0
	local c = if __rur_i + 2 < __rur_length then buffer.readu8(__rur_bytes, __rur_i + 2) else 0
	local n = a * 65536 + b * 256 + c
	buffer.writeu8(__rur_out, __rur_write, string.byte(__rur_alphabet, bit32.rshift(n, 18) + 1))
	buffer.writeu8(__rur_out, __rur_write + 1, string.byte(__rur_alphabet, bit32.band(bit32.rshift(n, 12), 63) + 1))
	buffer.writeu8(__rur_out, __rur_write + 2, if __rur_i + 1 < __rur_length then string.byte(__rur_alphabet, bit32.band(bit32.rshift(n, 6), 63) + 1) else 61)
	buffer.writeu8(__rur_out, __rur_write + 3, if __rur_i + 2 < __rur_length then string.byte(__rur_alphabet, bit32.band(n, 63) + 1) else 61)
	__rur_write += 4
end
_G.__RUR_RESULT = buffer.tostring(__rur_out)
return #_G.__RUR_RESULT`;
		const length = Number(await call(wrapped));
		const parts = [];
		for (let start = 1; start <= length; start += 90000) {
			parts.push(await call(`return string.sub(_G.__RUR_RESULT, ${start}, ${start + 89999})`));
		}
		await call("_G.__RUR_RESULT = nil return 0");
		return { text: Buffer.from(parts.join(""), "base64").toString("utf8"), studio: target };
	});
}

// Studio's viewport must have a whole-number logical width and height: a fractional one stretches its
// projection and blurs every capture (measured 2026-09-28/30: a story 98.45% at 1965.33 x 1150, 100%
// at 1966 x 1150, focused or not). ensureWholeViewport grows the Studio window by a physical pixel or two
// until both are whole (tools/wincap/resize.ps1, without activating it); returns the viewport.
export async function ensureWholeViewport({ studio = DEFAULT_STUDIO } = {}) {
	const read = async () => {
		const result = await runLuau("local V = workspace.CurrentCamera.ViewportSize return tostring(V.X) .. ',' .. tostring(V.Y)", { studio });
		return { size: String(result.text).split(",").map(Number), name: result.studio.name };
	};
	const fractional = value => Math.abs(value - Math.round(value)) > 0.01;
	let { size, name } = await read();
	for (let attempt = 0; attempt < 4 && (fractional(size[0]) || fractional(size[1])); attempt++) {
		const title = name.replace(/\s*\(placeId:.*$/, "").replace(/\s*-\s*Roblox Studio$/, "");
		const script = path.join(ROOT, "tools", "wincap", "resize.ps1");
		spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, "-Title", title, "-DeltaWidth", fractional(size[0]) ? "1" : "0", "-DeltaHeight", fractional(size[1]) ? "1" : "0"], { encoding: "utf8", env: withoutCookie() });
		await new Promise(resolve => setTimeout(resolve, 600));
		({ size, name } = await read());
	}
	if (fractional(size[0]) || fractional(size[1])) {
		process.stderr.write(`rur: Studio's viewport is still ${size.map(v => +v.toFixed(2)).join("x")} logical px after resizing; captures may be blurred
`);
	}
	return size;
}

// Focus: Studio renders at 60 fps in the background and 240 in front, but captures come out identical
// once the viewport is whole (three animated stories 100% unfocused, 2026-09-30), so
// captures leave focus alone unless asked (--focus / RUR_FOCUS=1): no pulling the user out of a game.
// focusStudio returns the window that had focus, for restoreFocus; RUR_STUDIO_FOCUSED=1 tells a child
// the parent already focused it.
export function focusStudio(studioName) {
	if (process.env.RUR_FOCUS !== "1" || process.env.RUR_STUDIO_FOCUSED === "1") return null;
	const title = studioName.replace(/\s*\(placeId:.*$/, "").replace(/\s*-\s*Roblox Studio$/, "");
	const script = path.join(ROOT, "tools", "wincap", "focus.ps1");
	const result = spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, "-Title", title], { encoding: "utf8", env: withoutCookie() });
	const previous = /previous=(\d+)/.exec(result.stdout || "");
	if (result.status !== 0 || !/focused=True/.test(result.stdout || "")) {
		process.stderr.write(`rur: could not focus Studio for the capture (it renders differently unfocused): ${(result.stderr || result.stdout || "").trim().split(/\r?\n/)[0]}\n`);
	}
	return previous ? previous[1] : null;
}

export function restoreFocus(previous) {
	if (!previous || previous === "0") return;
	const script = path.join(ROOT, "tools", "wincap", "focus.ps1");
	spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, "-Restore", String(previous)], { encoding: "utf8", env: withoutCookie() });
}

// Lossless capture of the Studio viewport (physical pixels) to `outFile`. With `scales` (display scales
// to try, in order) the size is the viewport's logical size and the matching scale is returned;
// without, `outFile`.
export function captureViewport(studioName, width, height, outFile, scales) {
	const title = studioName.replace(/\s*\(placeId:.*$/, "").replace(/\s*-\s*Roblox Studio$/, "");
	const script = path.join(ROOT, "tools", "wincap", "capture.ps1");
	const scaleArgs = scales ? ["-Scales", scales.join(",")] : [];
	const result = spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, "-Title", title, "-Width", String(width), "-Height", String(height), "-Out", outFile, ...scaleArgs], { encoding: "utf8", env: withoutCookie() });
	if (result.status !== 0 || !fs.existsSync(outFile)) throw new Error(`viewport capture failed: ${result.stderr || result.stdout}`);
	if (!scales) return outFile;
	const matched = /scale=([\d.]+)/.exec(result.stdout || "");
	return matched ? Number(matched[1]) : undefined;
}

export function luauString(value) {
	return JSON.stringify(String(value));
}
