// Images read from Roblox Studio: the asset ids asset delivery refuses (an image private to the
// game's creator, with no cookie or another account's) are read from an open Studio, which has the
// owner's access, through a Studio MCP bridge. Studio decodes each image (AssetService
// CreateEditableImageAsync) and hands back its pixels; rur stores them as a PNG in the asset cache
// (cli/assets.mjs storeStudioImage), recorded as read from Studio. No credential is involved and
// nothing leaves the machine: the bridge is on 127.0.0.1, and the Luau only reads (the EditableImage
// is destroyed, nothing is mounted or saved).
//
// Bridges (docs/assets.md):
//   RUR_STUDIO_MCP_PORT   the TCP bridge `rur compare` uses (cli/studio.mjs, docs/studio.md)
//   RUR_STUDIO_MCP_URL    an HTTP bridge with GET /instances and POST /mcp/execute_luau (robloxstudio-mcp
//                         --http-only, e.g. http://127.0.0.1:58741)
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

// HTTP bridge: each edit session (RUR_STUDIO narrows them), and a call that runs Luau in one
async function httpBridge(url, wanted) {
	const base = url.replace(/\/+$/, "");
	const call = async (route, body) => {
		const response = await fetch(`${base}${route}`, {
			method: body === undefined ? "GET" : "POST",
			headers: { "Content-Type": "application/json" },
			body: body === undefined ? undefined : JSON.stringify(body),
			signal: AbortSignal.timeout(TIMEOUT_MS),
		});
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
	if (env.RUR_STUDIO_MCP_URL) return httpBridge(env.RUR_STUDIO_MCP_URL, env.RUR_STUDIO || "");
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
