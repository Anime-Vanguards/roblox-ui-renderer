#!/usr/bin/env node
"use strict";

// Minimal MCP client for a Roblox Studio MCP bridge on a local TCP port (newline-delimited JSON-RPC; docs/studio.md).
// RUR_STUDIO_MCP_PORT names the port; runs use the Studio --studio, STUDIO_MCP_STUDIO or RUR_STUDIO
// names (an id or part of its window title), else the only Studio open.
//
//   node tools/studio-mcp.cjs list
//   node tools/studio-mcp.cjs call <tool> '<json args>'
//   node tools/studio-mcp.cjs call <tool> @args.json
//   node tools/studio-mcp.cjs luau <file.luau> [--studio <id or name substring>] [--datamodel Edit|Client|Server]
//   node tools/studio-mcp.cjs luau-big <file.luau> <out.txt> [--studio …]   (chunk returns a big string)
//   node tools/studio-mcp.cjs capture [<studio id or name substring>]   (screen_capture; images go to STUDIO_MCP_IMAGE_DIR)

const net = require("net");
const fs = require("fs");

const PORT = Number(process.env.RUR_STUDIO_MCP_PORT || 0);
const DEFAULT_STUDIO = process.env.STUDIO_MCP_STUDIO || process.env.RUR_STUDIO || "";
const HOST = "127.0.0.1";
const TIMEOUT_MS = Number(process.env.STUDIO_MCP_TIMEOUT_MS || 120000);

function connect() {
	if (!PORT) return Promise.reject(new Error("set RUR_STUDIO_MCP_PORT to the local port of your Studio MCP bridge (docs/studio.md)"));
	return new Promise((resolve, reject) => {
		const socket = net.connect(PORT, HOST);
		socket.once("connect", () => resolve(socket));
		socket.once("error", reject);
	});
}

async function session(fn) {
	const socket = await connect();
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
			} catch (_) {
				continue;
			}
			if (message.id !== undefined && waiters.has(message.id)) {
				const waiter = waiters.get(message.id);
				waiters.delete(message.id);
				waiter(message);
			}
		}
	});
	const request = (method, params) => new Promise((resolve, reject) => {
		const id = nextId++;
		const timer = setTimeout(() => {
			waiters.delete(id);
			reject(new Error(`timeout waiting for ${method}`));
		}, TIMEOUT_MS);
		waiters.set(id, message => {
			clearTimeout(timer);
			if (message.error) reject(new Error(JSON.stringify(message.error)));
			else resolve(message.result);
		});
		socket.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
	});
	try {
		await request("initialize", {
			protocolVersion: "2024-11-05",
			clientInfo: { name: "roblox-ui-renderer", version: "0.1.0" },
			capabilities: {},
		});
		socket.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
		return await fn(request);
	} finally {
		socket.destroy();
	}
}

let imageCounter = 0;

function saveImage(item) {
	const dir = process.env.STUDIO_MCP_IMAGE_DIR || process.cwd();
	const extension = (item.mimeType || "image/png").split("/")[1] || "png";
	const base = process.env.STUDIO_MCP_IMAGE_NAME || `studio-capture-${Date.now()}`;
	const file = require("path").join(dir, `${base}${imageCounter++ === 0 ? "" : `-${imageCounter}`}.${extension}`);
	fs.writeFileSync(file, Buffer.from(item.data, "base64"));
	return file;
}

function printResult(result) {
	if (result && Array.isArray(result.content)) {
		for (const item of result.content) {
			if (item.type === "text") process.stdout.write(item.text + "\n");
			else if (item.type === "image" && item.data) process.stdout.write(`[image saved: ${saveImage(item)}]\n`);
			else process.stdout.write(JSON.stringify(item).slice(0, 2000) + "\n");
		}
		if (result.isError) process.exitCode = 1;
	} else {
		process.stdout.write(JSON.stringify(result, null, 2) + "\n");
	}
}

async function resolveStudio(request, query) {
	const result = await request("tools/call", { name: "list_roblox_studios", arguments: {} });
	const text = (result.content || []).map(item => item.text || "").join("");
	const studios = JSON.parse(text).studios || [];
	if (!query && studios.length !== 1) throw new Error(`name a Studio with --studio or RUR_STUDIO: ${text}`);
	const match = studios.find(studio => studio.id === query) || studios.find(studio => studio.name.includes(query));
	if (!match) throw new Error(`no Studio matches ${query}: ${text}`);
	return match.id;
}

function parseArgs(raw) {
	if (!raw) return {};
	if (raw.startsWith("@")) return JSON.parse(fs.readFileSync(raw.slice(1), "utf8"));
	return JSON.parse(raw);
}

async function main() {
	const [command, ...rest] = process.argv.slice(2);
	if (command === "list") {
		await session(async request => {
			const result = await request("tools/list", {});
			for (const tool of result.tools || []) {
				console.log(`## ${tool.name}\n${tool.description || ""}\nschema: ${JSON.stringify(tool.inputSchema)}\n`);
			}
		});
	} else if (command === "call") {
		const [name, raw] = rest;
		await session(async request => printResult(await request("tools/call", { name, arguments: parseArgs(raw) })));
	} else if (command === "luau") {
		const [file, ...flags] = rest;
		const code = fs.readFileSync(file, "utf8");
		const studioFlag = flags.indexOf("--studio");
		const studioQuery = studioFlag >= 0 ? flags[studioFlag + 1] : DEFAULT_STUDIO;
		const modelFlag = flags.indexOf("--datamodel");
		const datamodel = modelFlag >= 0 ? flags[modelFlag + 1] : "Edit";
		await session(async request => {
			const studioId = await resolveStudio(request, studioQuery);
			printResult(await request("tools/call", {
				name: "execute_luau",
				arguments: { studio_id: studioId, datamodel_type: datamodel, code },
			}));
		});
	} else if (command === "luau-big") {
		// Runs a chunk that returns a (possibly huge) string, then pages it out of Studio's _G.
		const [file, outFile, ...flags] = rest;
		const code = fs.readFileSync(file, "utf8");
		const studioFlag = flags.indexOf("--studio");
		const studioQuery = studioFlag >= 0 ? flags[studioFlag + 1] : DEFAULT_STUDIO;
		const chunkSize = Number(process.env.STUDIO_MCP_CHUNK || 90000);
		await session(async request => {
			const studioId = await resolveStudio(request, studioQuery);
			const run = async luau => {
				const result = await request("tools/call", {
					name: "execute_luau",
					arguments: { studio_id: studioId, datamodel_type: "Edit", code: luau },
				});
				const text = (result.content || []).map(item => item.text || "").join("");
				if (result.isError) throw new Error(text);
				return text;
			};
			// Base64 on the Studio side keeps paging byte-exact for non-ASCII payloads.
			const wrapped = `local __rur_result = (function()
${code}
end)()
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
			const length = Number(await run(wrapped));
			if (!Number.isFinite(length)) throw new Error("chunk did not return a string");
			const parts = [];
			for (let start = 1; start <= length; start += chunkSize) {
				parts.push(await run(`return string.sub(_G.__RUR_RESULT, ${start}, ${start + chunkSize - 1})`));
			}
			await run("_G.__RUR_RESULT = nil return 0");
			const encoded = parts.join("");
			if (encoded.length !== length) throw new Error(`paged ${encoded.length} chars but Studio reported ${length}; lower STUDIO_MCP_CHUNK`);
			const text = Buffer.from(encoded, "base64").toString("utf8");
			fs.writeFileSync(outFile, text);
			console.log(`wrote ${text.length} chars to ${outFile}`);
		});
	} else if (command === "capture") {
		const [studioQuery = DEFAULT_STUDIO] = rest;
		await session(async request => {
			const studioId = await resolveStudio(request, studioQuery);
			printResult(await request("tools/call", {
				name: "screen_capture",
				arguments: { studio_id: studioId, capture_id: `ScreenCapture_${Date.now()}` },
			}));
		});
	} else {
		console.error("usage: studio-mcp.cjs list | call <tool> '<json>' | luau <file>");
		process.exitCode = 2;
	}
}

main().catch(error => {
	console.error(error.stack || String(error));
	process.exitCode = 1;
});
