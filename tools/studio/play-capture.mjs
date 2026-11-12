#!/usr/bin/env node
// Play-mode comparison of a fixture that builds its own ScreenGuis (fixtures/screen-layers.luau): `rur compare`
// mounts in the edit datamodel, where Players.LocalPlayer is nil and one ScreenGui under CoreGui is captured.
// This starts a play session in the named Studio, runs the fixture in the client datamodel, captures the
// composed viewport and compares it with `rur render --all-layers --mode play`.
//
//   node tools/studio/play-capture.mjs --fixture fixtures/screen-layers.luau --studio <name> [--viewport 400x300]
//       [--background 20,20,26] [--time 1] [--out out/compare/<fixture>-play] [--reuse]
//
// In the play session the script hides the core interface (chat, player list, backpack and the rest) and puts one
// more ScreenGui under the PlayerGui, DisplayOrder -1000000 and covering the screen with the background colour:
// the world and the core interface would otherwise show between the layers. The compared area is the
// viewport's top-left --viewport logical pixels. Roblox keeps its two menu buttons on screen whatever a script
// asks: the viewport is captured once with the backdrop alone, in a colour of its own (255, 0, 255: the
// buttons are dark and translucent, and their rims would vanish on a dark background), and the pixels that are
// not that colour (the buttons) are left out of the comparison and counted as "masked". The session is stopped again
// whatever happens. --reuse compares the saved captures without starting Studio.
//
// Use a Studio session of your own (tools/studio/launch.mjs): a play session takes its window over. Starting play
// can raise Studio's window, so the foreground is guarded for the whole session (cli/studio.mjs guardStudioFocus).
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { withStudioLock } from "../../cli/lock.mjs";
import { captureViewport, guardStudioFocus, listStudios, studioCaptureMethod } from "../../cli/studio.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const args = {};
for (let i = 2; i < process.argv.length; i++) {
	const key = process.argv[i].replace(/^--/, "");
	args[key] = process.argv[i + 1] && !process.argv[i + 1].startsWith("--") ? process.argv[++i] : true;
}
if (!args.fixture || (!args.studio && !args.reuse)) {
	console.error("usage: play-capture.mjs --fixture <file> --studio <name> [--viewport WxH] [--background r,g,b] [--time s] [--out dir] [--reuse]");
	process.exit(2);
}
const fixture = path.resolve(args.fixture);
const name = path.basename(fixture, ".luau");
const [width, height] = String(args.viewport || "400x300").split("x").map(Number);
const background = String(args.background || "20,20,26").split(",").map(Number);
const seconds = Number(args.time || 1);
const out = path.resolve(args.out || path.join(ROOT, "out", "compare", `${name}-play`));
fs.mkdirSync(out, { recursive: true });

const bridge = (...parameters) => {
	const result = spawnSync(process.execPath, [path.join(ROOT, "tools", "studio-mcp.cjs"), ...parameters], { encoding: "utf8" });
	if (result.status !== 0) throw new Error(`studio-mcp ${parameters[0]} failed: ${(result.stderr || result.stdout).trim()}`);
	return result.stdout.trim();
};
const callTool = (tool, parameters) => {
	const file = path.join(out, "call.json");
	fs.writeFileSync(file, JSON.stringify(parameters));
	try {
		return bridge("call", tool, `@${file}`);
	} finally {
		fs.rmSync(file, { force: true });
	}
};
const clientLuau = (code, studio) => {
	const file = path.join(out, "client.luau");
	fs.writeFileSync(file, code);
	try {
		return bridge("luau", file, "--studio", studio, "--datamodel", "Client");
	} finally {
		fs.rmSync(file, { force: true });
	}
};
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

// PNG reading and writing for 8-bit RGB / RGBA, non-interlaced: what the capture and the renderer write
function readPng(file) {
	const bytes = fs.readFileSync(file);
	let offset = 8, header = null;
	const data = [];
	while (offset < bytes.length) {
		const length = bytes.readUInt32BE(offset), type = bytes.toString("latin1", offset + 4, offset + 8);
		if (type === "IHDR") header = { width: bytes.readUInt32BE(offset + 8), height: bytes.readUInt32BE(offset + 12), depth: bytes[offset + 16], color: bytes[offset + 17] };
		else if (type === "IDAT") data.push(bytes.subarray(offset + 8, offset + 8 + length));
		offset += 12 + length;
	}
	if (!header || header.depth !== 8 || (header.color !== 2 && header.color !== 6)) throw new Error(`${file}: not an 8-bit RGB or RGBA PNG`);
	const channels = header.color === 6 ? 4 : 3, stride = header.width * channels;
	const raw = zlib.inflateSync(Buffer.concat(data));
	const rgb = Buffer.alloc(header.width * header.height * 3);
	let previous = Buffer.alloc(stride);
	for (let y = 0; y < header.height; y++) {
		const filter = raw[y * (stride + 1)];
		const line = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
		for (let x = 0; x < stride; x++) {
			const a = x >= channels ? line[x - channels] : 0, b = previous[x], c = x >= channels ? previous[x - channels] : 0;
			let predicted = 0;
			if (filter === 1) predicted = a;
			else if (filter === 2) predicted = b;
			else if (filter === 3) predicted = (a + b) >> 1;
			else if (filter === 4) {
				const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
				predicted = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
			}
			line[x] = (line[x] + predicted) & 255;
		}
		for (let x = 0; x < header.width; x++) line.copy(rgb, (y * header.width + x) * 3, x * channels, x * channels + 3);
		previous = line;
	}
	return { width: header.width, height: header.height, rgb };
}
function writePng(file, width, height, rgb) {
	const table = new Int32Array(256).map((_, n) => {
		let c = n;
		for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
		return c;
	});
	const chunk = (type, body) => {
		const head = Buffer.concat([Buffer.from(type, "latin1"), body]);
		let crc = -1;
		for (const byte of head) crc = table[(crc ^ byte) & 255] ^ (crc >>> 8);
		const length = Buffer.alloc(4), sum = Buffer.alloc(4);
		length.writeUInt32BE(body.length);
		sum.writeUInt32BE((crc ^ -1) >>> 0);
		return Buffer.concat([length, head, sum]);
	};
	const header = Buffer.alloc(13);
	header.writeUInt32BE(width, 0);
	header.writeUInt32BE(height, 4);
	header.set([8, 2, 0, 0, 0], 8);
	const raw = Buffer.alloc((width * 3 + 1) * height);
	for (let y = 0; y < height; y++) rgb.copy(raw, y * (width * 3 + 1) + 1, y * width * 3, (y + 1) * width * 3);
	fs.writeFileSync(file, Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header), chunk("IDAT", zlib.deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]));
}

async function capture() {
	const studios = await listStudios();
	const studio = studios.find(entry => entry.id === args.studio) || studios.find(entry => typeof entry.name === "string" && entry.name.includes(args.studio));
	if (!studio) throw new Error(`no Studio instance matches "${args.studio}"`);
	// A Studio on a desktop of its own (tools/studio/launch.mjs) is captured through the engine, but a play client
	// refuses to read its screenshot back (CreateEditableImageAsync: "cannot currently create editable image from
	// temporary texture id") and play mode has no edit datamodel to do it from
	if (studioCaptureMethod(args.studio) === "engine") {
		throw new Error("play-capture needs a Studio on the user's desktop (tools/studio/launch.mjs --user-desktop): a play session of a Studio on its own desktop cannot be captured losslessly");
	}
	return withStudioLock(`play-capture ${name}`, async () => {
		const stopGuard = guardStudioFocus(studio.name, 300000);
		callTool("start_stop_play", { studio_id: studio.id, is_start: true });
		try {
			// The client datamodel appears a moment after the session starts
			let ready = "";
			for (let attempt = 0; attempt < 40 && ready !== "ready"; attempt++) {
				await sleep(1500);
				try {
					ready = clientLuau(`local P = game:GetService("Players").LocalPlayer
return if P and P:FindFirstChildOfClass("PlayerGui") and game:IsLoaded() then "ready" else "waiting"`, studio.id);
				} catch {
					ready = "";
				}
			}
			if (ready !== "ready") throw new Error("the play session's client did not load");
			const source = fs.readFileSync(fixture, "utf8");
			const size = clientLuau(`local Players = game:GetService("Players")
local StarterGui = game:GetService("StarterGui")
for Attempt = 1, 50 do
	if pcall(function() StarterGui:SetCore("TopbarEnabled", false) end) then break end
	task.wait(0.1)
end
StarterGui:SetCoreGuiEnabled(Enum.CoreGuiType.All, false)
local Backdrop = Instance.new("ScreenGui")
Backdrop.Name = "RURBackdrop"
Backdrop.DisplayOrder = -1000000
Backdrop.IgnoreGuiInset = true
Backdrop.ResetOnSpawn = false
local Fill = Instance.new("Frame")
Fill.Size = UDim2.fromScale(1, 1)
Fill.BorderSizePixel = 0
Fill.Name = "Fill"
Fill.BackgroundColor3 = Color3.fromRGB(255, 0, 255)
Fill.Parent = Backdrop
Backdrop.Parent = Players.LocalPlayer.PlayerGui
task.wait(0.5)
local V = workspace.CurrentCamera.ViewportSize
return tostring(V.X) .. "," .. tostring(V.Y)`, studio.id);
			const viewport = size.split(/\s+/).pop().split(",").map(Number);
			if (!(viewport[0] >= width && viewport[1] >= height)) throw new Error(`the play viewport is ${size}, smaller than ${width}x${height}`);
			const scale = captureViewport(studio.name, Math.round(viewport[0]), Math.round(viewport[1]), path.join(out, "studio-chrome.png"), [1.5, 1, 1.25, 2, 1.75]);
			if (!scale) throw new Error("no Studio viewport of the play session's size was found to capture");
			clientLuau(`local Players = game:GetService("Players")
Players.LocalPlayer.PlayerGui.RURBackdrop.Fill.BackgroundColor3 = Color3.fromRGB(${background.join(", ")})
local Target = Instance.new("ScreenGui")
Target.Name = "RURStory"
Target.IgnoreGuiInset = true
Target.ResetOnSpawn = false
Target.Parent = game:GetService("CoreGui")
local Mount = (function()
${source}
end)()
Mount(Target)
task.wait(${seconds})
return "mounted"`, studio.id);
			captureViewport(studio.name, Math.round(viewport[0] * scale), Math.round(viewport[1] * scale), path.join(out, "studio-viewport.png"));
			fs.writeFileSync(path.join(out, "studio-capture.json"), JSON.stringify({ viewport, scale, fixture: path.relative(ROOT, fixture), background, time: seconds }, null, "\t"));
			return scale;
		} finally {
			callTool("start_stop_play", { studio_id: studio.id, is_start: false });
			await sleep(1500);
			const guard = await stopGuard();
			if (/FOREGROUND TAKEN|ON SCREEN/.test(guard)) console.error(`play-capture: Studio raised a window during the play session (parked, foreground handed back):
${guard.trim()}`);
		}
	});
}

const scale = args.reuse ? JSON.parse(fs.readFileSync(path.join(out, "studio-capture.json"), "utf8")).scale : await capture();
const rendered = path.join(out, "rur");
const render = spawnSync(process.execPath, [path.join(ROOT, "cli", "rur.mjs"), "render", "--fixture", fixture, "--all-layers", "--mode", "play", "--viewport", `${width}x${height}`, "--scale", String(scale),
	"--time", String(seconds), "--background", background.join(","), "--out", rendered], { encoding: "utf8" });
if (render.status !== 0) throw new Error(`rur render failed: ${render.stderr || render.stdout}`);
const studio = readPng(path.join(out, "studio-viewport.png")), chrome = readPng(path.join(out, "studio-chrome.png")), ours = readPng(path.join(rendered, "render.png"));
const w = Math.round(width * scale), h = Math.round(height * scale);
if (ours.width !== w || ours.height !== h) throw new Error(`the render is ${ours.width}x${ours.height}, expected ${w}x${h}`);
const crop = Buffer.alloc(w * h * 3), diff = Buffer.alloc(w * h * 3);
let over = 0, largest = 0, exact = 0, masked = 0;
for (let y = 0; y < h; y++) {
	for (let x = 0; x < w; x++) {
		let worst = 0, button = false;
		for (let c = 0; c < 3; c++) button ||= chrome.rgb[(y * chrome.width + x) * 3 + c] !== [255, 0, 255][c];
		for (let c = 0; c < 3; c++) {
			const a = studio.rgb[(y * studio.width + x) * 3 + c], b = ours.rgb[(y * w + x) * 3 + c];
			crop[(y * w + x) * 3 + c] = a;
			worst = Math.max(worst, Math.abs(a - b));
		}
		if (button) {
			masked++;
			diff.set([0, 0, 96], (y * w + x) * 3);
			continue;
		}
		if (worst === 0) exact++;
		if (worst > 2) over++;
		largest = Math.max(largest, worst);
		diff.fill(Math.min(255, worst * 8), (y * w + x) * 3, (y * w + x) * 3 + 3);
	}
}
writePng(path.join(out, "studio.png"), w, h, crop);
writePng(path.join(out, "diff.png"), w, h, diff);
const report = { fixture: path.relative(ROOT, fixture), width: w, height: h, scale, masked, exactPercent: +(100 * exact / (w * h - masked)).toFixed(4), over2: over, largest };
fs.writeFileSync(path.join(out, "report.json"), JSON.stringify(report, null, "\t"));
console.log(`${name} (play) ${w}x${h} over2=${over} largest=${largest} exact=${report.exactPercent}% masked=${masked}`);
