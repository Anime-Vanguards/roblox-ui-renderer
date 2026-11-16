// Images read from Roblox Studio: the asset ids asset delivery refuses (an image private to the
// game's creator, with no cookie or another account's) are read from an open Studio, which has the
// owner's access, through a Studio MCP bridge. Studio decodes each image (AssetService
// CreateEditableImageAsync) and hands back its pixels; rur stores them as a PNG in the asset cache
// (cli/assets.mjs storeStudioImage), recorded as read from Studio. Nothing leaves the machine: an HTTP
// bridge must be on it (127.0.0.0/8, ::1 or localhost; any other host is refused before a request) and a
// redirect is refused, never followed. The Luau only reads (the EditableImage is destroyed, nothing is
// mounted or saved). No Roblox credential is involved; robloxstudio-mcp 3 takes a token of its own,
// RUR_STUDIO_MCP_TOKEN: read only from the environment, sent only to that bridge in the header it
// expects, and never printed or written anywhere (error text that could quote it has it taken out).
//
// Bridges (docs/assets.md):
//   RUR_STUDIO_MCP_PORT   the TCP bridge `rur compare` uses (cli/studio.mjs, docs/studio.md)
//   RUR_STUDIO_MCP_URL    an HTTP bridge on this machine, e.g. http://127.0.0.1:58741. Its GET /health picks
//                         the API: robloxstudio-mcp 3 or later (v3Bridge), else GET /instances and POST
//                         /mcp/execute_luau (robloxstudio-mcp --http-only)
//   RUR_STUDIO_MCP_TOKEN  robloxstudio-mcp 3's auth token
// RUR_STUDIO picks the Studio (an id or part of its name; default: every edit session, in order).
// RUR_STUDIO_IMAGES=0 turns this off with a bridge configured (cli/assets.mjs studioImagesEnabled).

import zlib from "node:zlib";
import { PNG } from "pngjs";

const TIMEOUT_MS = 120000;
// Base64 characters one call returns at most (the image that crosses it is the call's last); the rest
// of the ids go in the next call
const CALL_BUDGET = 2_000_000;
// Pixels travel zstd-compressed (EncodingService:CompressBuffer) when this Node can decompress them; icons
// are mostly transparent, so a call carries many
const ZSTD = typeof zlib.zstdDecompressSync === "function";

// Images Studio loads at once (each load waits on its own download)
const WAVE = 12;

// Many images in one call: a JSON list, one entry per id it reached, in order: { width, height, encoding
// ("zstd" | "raw"), pixels (base64 RGBA8) } or { error }. Ids load WAVE at a time; the call stops once
// CALL_BUDGET characters are used (a wave's unreported loads are dropped and asked for again next call).
function imagesLuau(ids) {
	return `
local AssetService = game:GetService("AssetService")
local EncodingService = game:GetService("EncodingService")
local HttpService = game:GetService("HttpService")
local Ids = { ${ids.join(", ")} }
local Results = {}
local Used = 0

local function Encode(Image)
	local Size = Image.Size
	local Pixels = Image:ReadPixelsBuffer(Vector2.zero, Size)
	local Encoding = "raw"
	if ${ZSTD} then
		local Packed, Compressed = pcall(EncodingService.CompressBuffer, EncodingService, Pixels, Enum.CompressionAlgorithm.Zstd, 1)
		if Packed then
			Pixels = Compressed
			Encoding = "zstd"
		end
	end
	return { width = Size.X, height = Size.Y, encoding = Encoding, pixels = buffer.tostring(EncodingService:Base64Encode(Pixels)) }
end

for First = 1, #Ids, ${WAVE} do
	local Last = math.min(First + ${WAVE} - 1, #Ids)
	local Loads = {}
	local Waiting = Last - First + 1
	local Thread = coroutine.running()
	for Index = First, Last do
		task.spawn(function()
			Loads[Index] = table.pack(pcall(AssetService.CreateEditableImageAsync, AssetService, Content.fromAssetId(Ids[Index])))
			Waiting -= 1
			if Waiting == 0 then
				task.spawn(Thread)
			end
		end)
	end
	if Waiting > 0 then
		coroutine.yield()
	end
	for Index = First, Last do
		local Ok, Image = Loads[Index][1], Loads[Index][2]
		if Used >= ${CALL_BUDGET} then
			if Ok then
				Image:Destroy()
			end
		elseif not Ok then
			table.insert(Results, { error = tostring(Image) })
		else
			local Entry = Encode(Image)
			Image:Destroy()
			Used += #Entry.pixels
			table.insert(Results, Entry)
		end
	end
	if Used >= ${CALL_BUDGET} then
		break
	end
end
return HttpService:JSONEncode(Results)
`;
}

// The HTTP bridge's address, refused unless it is on this machine: 127.0.0.0/8, ::1 or localhost. The URL
// parser has normalized the host (127.1, 0x7f.0.0.1 and 2130706433 are all 127.0.0.1).
function loopbackBridge(url) {
	let parsed = null;
	try {
		parsed = new URL(url);
	} catch {
		// refused below
	}
	const host = parsed?.hostname;
	if (!parsed || !/^https?:$/.test(parsed.protocol) || !(host === "localhost" || host === "[::1]" || /^127(?:\.\d{1,3}){3}$/.test(host))) {
		throw new Error(`RUR_STUDIO_MCP_URL must be an http:// or https:// address on this machine (127.0.0.1, ::1 or localhost)${parsed ? `, not ${parsed.protocol}//${parsed.host}` : ""}`);
	}
	return url.replace(/\/+$/, "");
}

// One request to the HTTP bridge. A redirect is refused, not followed: it could lead off the machine.
async function bridgeRequest(base, route, body, headers = {}) {
	const response = await fetch(`${base}${route}`, {
		method: body === undefined ? "GET" : "POST",
		headers: { "Content-Type": "application/json", ...headers },
		body: body === undefined ? undefined : JSON.stringify(body),
		redirect: "manual",
		signal: AbortSignal.timeout(TIMEOUT_MS),
	});
	if ((response.status >= 300 && response.status < 400) || response.type === "opaqueredirect") {
		throw Object.assign(new Error(`Studio bridge ${route} answered with a redirect (HTTP ${response.status}), which is not followed`), { redirect: true });
	}
	return response;
}

// The version the bridge reports on its unauthenticated GET /health, or "" when it reports none (an older
// bridge, or nothing listening: the older API then reports the failure)
async function bridgeVersion(base) {
	try {
		const response = await bridgeRequest(base, "/health");
		if (!response.ok) return "";
		const health = await response.json();
		return String(health?.version ?? health?.serverVersion ?? "").slice(0, 40);
	} catch (error) {
		if (error.redirect) throw error;
		return "";
	}
}

// HTTP bridge: robloxstudio-mcp 3 or later by its /health, else the older API
async function httpBridge(url, wanted, token) {
	const base = loopbackBridge(url);
	const version = await bridgeVersion(base);
	return Number(/^v?(\d+)(?:\.|$)/.exec(version)?.[1] || 0) >= 3 ? v3Bridge(base, wanted, token, version) : legacyBridge(base, wanted);
}

// robloxstudio-mcp 3's API, as read at v3.5.0 (github.com/chrrxs/robloxstudio-mcp, commit dd73269): each tool
// is POST /mcp/<tool>, answered with the tool's result, and needs the server's token in X-MCP-Auth (401
// without it). get_connected_instances lists the sessions, { instances: [{ id, placeName, peers: { edit, ... }
// }] } (3.0: [{ id, name, roles }]); execute_luau { code, instance_id, target } runs Luau in one and answers
// { success, returnValue, error }. Server text in an error has the token taken out.
async function v3Bridge(base, wanted, token, version) {
	if (!token) throw new Error(`the Studio bridge is robloxstudio-mcp ${version}, which needs its auth token: set RUR_STUDIO_MCP_TOKEN`);
	// What a header can carry (fetch's own error would quote the value)
	if (!/^[\x21-\x7e]+$/.test(token)) throw new Error("RUR_STUDIO_MCP_TOKEN holds characters an HTTP header cannot carry");
	const hide = text => String(text).split(token).join("<RUR_STUDIO_MCP_TOKEN>");
	const call = async (tool, body) => {
		const response = await bridgeRequest(base, `/mcp/${tool}`, body, { "X-MCP-Auth": token });
		if (response.status === 401) throw new Error(`the Studio bridge refused RUR_STUDIO_MCP_TOKEN (HTTP 401): set it to robloxstudio-mcp's token`);
		const reply = await response.json().catch(() => null);
		if (!response.ok) {
			const detail = reply?.message || reply?.error;
			throw new Error(`Studio bridge /mcp/${tool}: HTTP ${response.status}${detail ? ` (${hide(detail).slice(0, 200)})` : ""}`);
		}
		return reply?.structuredContent ?? reply ?? {};
	};
	const { instances = [] } = await call("get_connected_instances", {});
	const sessions = instances.filter(instance => instance.peers?.edit || instance.roles?.includes("edit"))
		.filter(instance => !wanted || instance.id === wanted || String(instance.placeName ?? instance.name ?? "").includes(wanted));
	return sessions.map(session => ({
		name: session.placeName || session.name || session.id,
		run: async code => {
			const reply = await call("execute_luau", { code, instance_id: session.id, target: "edit" });
			if (!reply.success) throw new Error(hide(reply.error || "the Luau failed").slice(0, 300));
			return String(reply.returnValue ?? "");
		},
	}));
}

// The older API (robloxstudio-mcp --http-only): each edit session (RUR_STUDIO narrows them), and a call that
// runs Luau in one
async function legacyBridge(base, wanted) {
	const call = async (route, body) => {
		const response = await bridgeRequest(base, route, body);
		if (!response.ok) throw new Error(`Studio bridge ${route}: HTTP ${response.status}`);
		return response.json();
	};
	const { instances = [] } = await call("/instances");
	const sessions = instances.filter(instance => instance.role === "edit")
		.filter(instance => !wanted || instance.instanceId === wanted || String(instance.placeName || "").includes(wanted));
	return sessions.map(session => ({
		name: session.placeName || session.instanceId,
		run: async code => {
			const payload = await call("/mcp/execute_luau", { code, sessionId: session.instanceId });
			const reply = JSON.parse(payload.content?.[0]?.text || "{}");
			if (!reply.success) throw new Error(String(reply.error || "the Luau failed").slice(0, 300));
			return String(reply.returnValue ?? "");
		},
	}));
}

// TCP bridge: cli/studio.mjs (the Studio RUR_STUDIO names, else the only one open)
async function tcpBridge() {
	const { runLuau, DEFAULT_STUDIO } = await import("./studio.mjs");
	return [{ name: DEFAULT_STUDIO || "Studio", run: async code => (await runLuau(code, { studio: DEFAULT_STUDIO })).text }];
}

async function studios(env) {
	if (env.RUR_STUDIO_MCP_URL) return httpBridge(env.RUR_STUDIO_MCP_URL, env.RUR_STUDIO || "", (env.RUR_STUDIO_MCP_TOKEN || "").trim());
	return tcpBridge();
}

function encodePng(width, height, rgba) {
	const image = new PNG({ width, height });
	rgba.copy(image.data);
	return PNG.sync.write(image);
}

// Reads `ids` (digit strings) from Studio. Returns { images: Map<id, png Buffer>, errors: Map<id, reason> };
// with no Studio reachable every id's reason says so.
export async function readStudioImages(ids, env = process.env) {
	const images = new Map();
	const errors = new Map();
	let sessions;
	try {
		sessions = await studios(env);
		if (!sessions.length) throw new Error("no Studio edit session is connected");
	} catch (error) {
		const reason = `no Studio to read it from (${String(error?.message || error).slice(0, 200)})`;
		for (const id of ids) errors.set(id, reason);
		return { images, errors };
	}
	// Each session reads what the ones before it could not, as few calls as the budget allows
	let pending = [...ids];
	for (const session of sessions) {
		const failed = [];
		const fail = (id, error) => {
			failed.push(id);
			errors.set(id, `Studio (${session.name}) could not read it: ${String(error?.message || error).slice(0, 200)}`);
		};
		while (pending.length) {
			let replies;
			try {
				replies = JSON.parse(await session.run(imagesLuau(pending)));
				if (!Array.isArray(replies) || !replies.length) throw new Error("it returned no images");
			} catch (error) {
				for (const id of pending) fail(id, error);
				break;
			}
			for (const [index, reply] of replies.entries()) {
				const id = pending[index];
				try {
					if (reply.error) throw new Error(reply.error);
					const packed = Buffer.from(reply.pixels, "base64");
					const rgba = reply.encoding === "zstd" ? zlib.zstdDecompressSync(packed) : packed;
					if (rgba.length !== reply.width * reply.height * 4) throw new Error(`${rgba.length} bytes of pixels for ${reply.width} x ${reply.height}`);
					images.set(id, encodePng(reply.width, reply.height, rgba));
					errors.delete(id);
				} catch (error) {
					fail(id, error);
				}
			}
			pending = pending.slice(replies.length);
		}
		pending = failed;
		if (!pending.length) break;
	}
	return { images, errors };
}

// Every image id a place's GUI names: ImageLabel / ImageButton / ScrollingFrame image properties. Attributes
// and StringValues are not read: on a live place every one holding an asset URI was an AnimationId. Read-only.
const SCAN_LUAU = `
local HttpService = game:GetService("HttpService")
local Properties = {
	ImageLabel = { "Image" },
	ImageButton = { "Image", "HoverImage", "PressedImage" },
	ScrollingFrame = { "TopImage", "MidImage", "BottomImage" },
}
local Seen = {}
local Ids = {}
local function Add(Value)
	if type(Value) ~= "string" then
		return
	end
	for _, Pattern in { "rbxassetid://(%d+)", "roblox%.com/asset/?%?.-id=(%d+)" } do
		for Id in string.gmatch(Value, Pattern) do
			if not Seen[Id] then
				Seen[Id] = true
				table.insert(Ids, Id)
			end
		end
	end
end
for _, Service in game:GetChildren() do
	local Ok, Descendants = pcall(Service.GetDescendants, Service)
	if Ok then
		for _, Item in Descendants do
			pcall(function()
				for _, Name in Properties[Item.ClassName] or {} do
					Add(Item[Name])
				end
			end)
		end
	end
end
return HttpService:JSONEncode(Ids)
`;

// The image ids of every connected Studio's place (RUR_STUDIO narrows them), and the Studios' names
export async function studioImageIds(env = process.env) {
	const sessions = await studios(env);
	if (!sessions.length) throw new Error("no Studio edit session is connected");
	const ids = new Set();
	for (const session of sessions) for (const id of JSON.parse(await session.run(SCAN_LUAU))) ids.add(String(id));
	return { ids: [...ids], studios: sessions.map(session => session.name) };
}
