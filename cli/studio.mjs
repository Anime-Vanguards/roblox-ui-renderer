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
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { withoutCookie } from "./assets.mjs";
import { PNG } from "pngjs";

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
export async function runLuau(code, { studio = DEFAULT_STUDIO, datamodel = "Edit" } = {}) {
	return session(async request => {
		const target = await studioId(request, studio);
		const call = async luau => {
			const result = await request("tools/call", { name: "execute_luau", arguments: { studio_id: target.id, datamodel_type: datamodel, code: luau } });
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
		const bytes = Buffer.from(parts.join(""), "base64");
		return { text: bytes.toString("utf8"), bytes, studio: target };
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

// Focus: rur never takes the keyboard focus. Studio renders at 60 fps in the background and 240 in front, but
// captures come out identical once the viewport is whole (three animated stories 100% unfocused, 2026-09-30), and
// a Studio window brought to the front pulls the user out of what they are doing. focusStudio / restoreFocus are
// kept for callers (--focus) and do nothing.
export function focusStudio() {
	if (process.env.RUR_FOCUS === "1" && !focusStudio.warned) {
		focusStudio.warned = true;
		process.stderr.write("rur: --focus / RUR_FOCUS is ignored: captures never bring Studio to the front (they render the same unfocused)\n");
	}
	return null;
}

export function restoreFocus() {}

// Watches the foreground while something that may raise Studio runs (a play session): a Studio window that takes
// the foreground is put back behind and the previous window gets it again (tools/studio/StudioLaunch.cs).
// Returns a stop function that ends the watch and resolves to its log.
export function guardStudioFocus(studioName, ms = 120000) {
	const title = studioName.replace(/\s*\(placeId:.*$/, "").replace(/\s*-\s*Roblox Studio$/, "");
	const lookup = spawnSync("powershell", ["-NoProfile", "-Command", `(Get-Process RobloxStudioBeta -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle.IndexOf('${title.replace(/'/g, "''")}', [StringComparison]::OrdinalIgnoreCase) -ge 0 } | Select-Object -First 1).Id`], { encoding: "utf8", env: withoutCookie() });
	const pid = Number(String(lookup.stdout || "").trim());
	if (!pid) return async () => "no Studio process found to guard";
	const child = spawn("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", path.join(ROOT, "tools", "studio", "launch.ps1"), "-GuardPid", String(pid), "-GuardMs", String(ms)], { env: withoutCookie(), windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
	let log = "";
	child.stdout.on("data", chunk => {
		log += chunk;
	});
	const done = new Promise(resolve => child.once("exit", resolve));
	return async () => {
		child.kill();
		await done;
		return log;
	};
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

// Studios started by tools/studio/launch.mjs: [{ pid, exe, version, place, scale, desktop, startedAt }], one file per
// machine; entries whose process has ended are dropped when read
export const STUDIO_DESKTOP = "rur-studio";
const LAUNCHED_FILE = path.join(os.tmpdir(), "rur", "studios.json");
const alive = pid => {
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		return error.code === "EPERM";
	}
};
export function launchedStudios() {
	try {
		return JSON.parse(fs.readFileSync(LAUNCHED_FILE, "utf8")).filter(studio => alive(studio.pid));
	} catch {
		return [];
	}
}
export function writeLaunchedStudios(studios) {
	fs.mkdirSync(path.dirname(LAUNCHED_FILE), { recursive: true });
	fs.writeFileSync(LAUNCHED_FILE, JSON.stringify(studios, null, 1));
}
// How a Studio named by `query` (part of its window title, i.e. its place file) is captured: "engine" when the
// launcher started it on its own desktop, else "window" (PrintWindow). RUR_STUDIO_CAPTURE overrides.
export function studioCaptureMethod(query) {
	if (process.env.RUR_STUDIO_CAPTURE) return process.env.RUR_STUDIO_CAPTURE;
	const hidden = launchedStudios().some(studio => studio.desktop && query && path.basename(studio.place).includes(query));
	return hidden ? "engine" : "window";
}

// Lossless capture through the engine, for a Studio started on a desktop of its own (tools/studio/launch.mjs):
// Windows does not compose windows there, so PrintWindow has nothing to read, but Studio renders and
// CaptureService:CaptureScreenshot returns the viewport at its physical size, read back through an EditableImage.
// The screenshot leaves CoreGui out, so what is captured is mounted under StarterGui (shown in edit mode).
// Returns { png, dpi } for the top-left width x height physical pixels.
const ENGINE_CAPTURE_LUAU = (width, height) => `
local CaptureService = game:GetService("CaptureService")
local AssetService = game:GetService("AssetService")
local Id = nil
CaptureService:CaptureScreenshot(function(ContentId)
	Id = ContentId
end)
local Started = os.clock()
while not Id do
	if os.clock() - Started > 20 then
		error("CaptureScreenshot did not answer in 20 s")
	end
	task.wait()
end
local Image = AssetService:CreateEditableImageAsync(Content.fromUri(Id))
local Size = Image.Size
local W, H = math.min(${width}, Size.X), math.min(${height}, Size.Y)
local Pixels = Image:ReadPixelsBuffer(Vector2.zero, Vector2.new(W, H))
Image:Destroy()
-- Runs of equal RGB: [count u16][r][g][b]
local Out = buffer.create(W * H * 5)
local Write = 0
local Count = W * H
local Index = 0
while Index < Count do
	local At = Index * 4
	local R, G, B = buffer.readu8(Pixels, At), buffer.readu8(Pixels, At + 1), buffer.readu8(Pixels, At + 2)
	local Run = 1
	while Index + Run < Count and Run < 65535 do
		local Next = (Index + Run) * 4
		if buffer.readu8(Pixels, Next) ~= R or buffer.readu8(Pixels, Next + 1) ~= G or buffer.readu8(Pixels, Next + 2) ~= B then
			break
		end
		Run += 1
	end
	buffer.writeu16(Out, Write, Run)
	buffer.writeu8(Out, Write + 2, R)
	buffer.writeu8(Out, Write + 3, G)
	buffer.writeu8(Out, Write + 4, B)
	Write += 5
	Index += Run
end
local Viewport = workspace.CurrentCamera.ViewportSize
return string.format("%d %d %d %d %.6f|", W, H, Size.X, Size.Y, Viewport.X) .. buffer.readstring(Out, 0, Write)
`;

export async function captureEngine(width, height, { studio = DEFAULT_STUDIO, datamodel = "Edit" } = {}) {
	const { bytes } = await runLuau(ENGINE_CAPTURE_LUAU(width, height), { studio, datamodel });
	const bar = bytes.indexOf(0x7c);
	const [w, h, fullWidth, , viewportWidth] = bytes.subarray(0, bar).toString("latin1").split(" ").map(Number);
	const png = new PNG({ width: w, height: h });
	let out = 0;
	for (let at = bar + 1; at + 5 <= bytes.length; at += 5) {
		const run = bytes.readUInt16LE(at);
		for (let k = 0; k < run; k++, out += 4) {
			png.data[out] = bytes[at + 2];
			png.data[out + 1] = bytes[at + 3];
			png.data[out + 2] = bytes[at + 4];
			png.data[out + 3] = 255;
		}
	}
	if (out !== w * h * 4) throw new Error(`engine capture decoded ${out / 4} of ${w * h} pixels`);
	return { png, dpi: Math.round((fullWidth / viewportWidth) * 100) / 100 };
}

// A Luau string literal of value's text: its UTF-8 as it is, with ", \ and the control characters escaped as Luau reads
// them (JSON's \uXXXX escapes are not Luau's, so a control character used to break the chunk it was written into)
export function luauString(value) {
	let literal = "\"";
	for (const char of String(value)) {
		const code = char.codePointAt(0);
		if (char === "\"" || char === "\\") literal += `\\${char}`;
		else if (char === "\n") literal += "\\n";
		else if (char === "\r") literal += "\\r";
		else if (char === "\t") literal += "\\t";
		else if (code < 0x20 || code === 0x7f) literal += `\\${String(code).padStart(3, "0")}`;
		// (a lone surrogate has no UTF-8: the replacement character stands in for it)
		else if (code >= 0xd800 && code <= 0xdfff) literal += "\ufffd";
		else literal += char;
	}
	return `${literal}"`;
}
