// `rur assets warm`: the asset cache filled ahead of renders with every image an open Studio's place uses.
// The place is scanned for image ids (cli/studio-images.mjs studioImageIds); ids asset delivery gives are
// downloaded (`rur-native assets fetch`, cli/assets.mjs resolveAll) and the ones it refuses are read from
// Studio (readRefusedFromStudio), as a render would, so later renders find them all cached and need
// neither Studio nor the network. --refresh reads the ids cached from Studio again (an image re-uploaded
// under the same id, or a cache to rebuild).
import { assetKeyFromUri, assetKeyUri, cachedSource, readRefusedFromStudio, resolveAll, storeFromStudio, studioImagesEnabled } from "./assets.mjs";
import { studioImageIds } from "./studio-images.mjs";

// `resolve`: how the ids are downloaded (resolveAll's signature; tests give their own)
export async function warmAssets({ refresh = false, env = process.env, resolve = resolveAll } = {}) {
	if (!studioImagesEnabled(env)) {
		throw new Error("rur assets warm reads the place from Studio: set RUR_STUDIO_MCP_URL or RUR_STUDIO_MCP_PORT to a Studio MCP bridge (docs/assets.md), and leave RUR_STUDIO_IMAGES unset");
	}
	const started = Date.now();
	const { ids, studios } = await studioImageIds(env);
	const before = new Map(ids.map(id => [id, cachedSource(id)]));
	const refreshed = [];
	if (refresh) {
		const stale = ids.filter(id => before.get(id) === "studio");
		const { files } = await storeFromStudio(stale, env);
		refreshed.push(...files.keys());
	}
	// Studio is read here, with this environment, for what asset delivery refused (ids are cache keys: a pinned
	// version is asked for as rbxassetid://<id>&version=<version>)
	const results = await resolve(ids.map(assetKeyUri), { offline: false, studio: false });
	await readRefusedFromStudio(results, env);
	const counts = { cached: 0, downloaded: 0, fromStudio: 0, refreshed: refreshed.length };
	const missing = [];
	for (const [uri, result] of results) {
		const id = assetKeyFromUri(uri);
		if (result.error) missing.push({ id, error: result.error });
		else if (refreshed.includes(id)) continue;
		else if (before.get(id)) counts.cached++;
		else if (cachedSource(id) === "studio") counts.fromStudio++;
		else counts.downloaded++;
	}
	return { studios, images: ids.length, ...counts, missing, seconds: Math.round((Date.now() - started) / 100) / 10 };
}
