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

import { PNG } from "pngjs";

const TIMEOUT_MS = 120000;

// The pixels of one image as JSON { width, height, pixels (base64 RGBA8) } or { error }
function imageLuau(id) {
	return `
local AssetService = game:GetService("AssetService")
local EncodingService = game:GetService("EncodingService")
local HttpService = game:GetService("HttpService")
local Ok, Image = pcall(AssetService.CreateEditableImageAsync, AssetService, Content.fromAssetId(${id}))
if not Ok then
	return HttpService:JSONEncode({ error = tostring(Image) })
end
local Size = Image.Size
local Pixels = Image:ReadPixelsBuffer(Vector2.zero, Size)
Image:Destroy()
return HttpService:JSONEncode({ width = Size.X, height = Size.Y, pixels = buffer.tostring(EncodingService:Base64Encode(Pixels)) })
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
	for (const id of ids) {
		for (const session of sessions) {
			try {
				const reply = JSON.parse(await session.run(imageLuau(id)));
				if (reply.error) throw new Error(reply.error);
				const rgba = Buffer.from(reply.pixels, "base64");
				if (rgba.length !== reply.width * reply.height * 4) throw new Error(`${rgba.length} bytes of pixels for ${reply.width} x ${reply.height}`);
				images.set(id, encodePng(reply.width, reply.height, rgba));
				errors.delete(id);
				break;
			} catch (error) {
				errors.set(id, `Studio (${session.name}) could not read it: ${String(error?.message || error).slice(0, 200)}`);
			}
		}
	}
	return { images, errors };
}
