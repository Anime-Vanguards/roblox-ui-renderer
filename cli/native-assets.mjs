// Native asset worker: only missing image URIs and cloud-font requests cross this persistent pipe,
// never frame JSON. The native app's asset store (native/src/assets.rs) and the render pipeline's
// resolver (native/src/resolve.rs) start it when a URI needs a download or cloud fonts are missing;
// glyphs are rasterized natively (native/src/glyphs.rs). Authentication, downloads and the asset cache
// stay with cli/assets.mjs.
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";
import { fileURLToPath } from "node:url";
import { fetchAssetId, resolveAll } from "./assets.mjs";
import { ViewportWorker } from "./viewports.mjs";

// Downloads the cloud font assets a run reported missing (diagnostics.missingFontAssets) into the asset
// cache, where the engine finds them on its next start. Whether any arrived: a font face, or a family's
// JSON (its faces are asked for on the next run).
export async function fetchMissingFonts(diagnostics) {
	let fetched = 0;
	for (const id of Array.isArray(diagnostics && diagnostics.missingFontAssets) ? diagnostics.missingFontAssets : []) {
		try {
			const result = await fetchAssetId(id, { allowDecal: false });
			if (result.kind === "font" || result.kind === "json") fetched++;
		} catch {
			// unavailable: the run keeps its fallback metrics and reports the font as missing
		}
	}
	return fetched > 0;
}

export class NativeAssetWorker {
	constructor({ offline = false } = {}) {
		this.offline = offline;
		this.viewports = new ViewportWorker({ offline });
	}
	close() { this.viewports.close(); }

	// { id, images: [uri...] } -> { id, images: [{ uri, file } | { uri, error }] };
	// { id, command: "fonts", diagnostics } -> { id, fontsChanged, images: [] }
	async prepare(request) {
		if (request.command === "model") {
			return { id: request.id, ...await this.viewports.model(request.model) };
		}
		if (request.command === "fonts") {
			return { id: request.id, fontsChanged: !this.offline && await fetchMissingFonts(request.diagnostics), images: [] };
		}
		const uris = Array.isArray(request.images) ? request.images : [];
		if (Array.isArray(request.viewportSlots)) this.viewports.retain(request.viewportSlots);
		const images = await resolveAll(uris.filter(uri => !uri.startsWith("rur-viewport://")), { offline: this.offline });
		for (const uri of uris.filter(uri => uri.startsWith("rur-viewport://"))) {
			try { images.set(uri, await this.viewports.render(uri, request.viewports?.[uri], { persist: request.persistViewports === true })); }
			catch (error) { images.set(uri, { error: String(error?.message || "Viewport render failed") }); }
		}
		return { id: request.id, images: [...images].map(([uri, result]) => ({ uri, ...result })) };
	}
}

async function main() {
	const worker = new NativeAssetWorker({ offline: process.argv.includes("--offline") });
	process.on("exit", () => worker.close());
	const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
	for await (const line of input) {
		let request;
		try {
			request = JSON.parse(line);
			if (request.command === "quit") break;
			process.stdout.write(JSON.stringify(await worker.prepare(request)) + "\n");
		} catch (error) {
			process.stdout.write(JSON.stringify({ id: request?.id, error: String(error?.message || "Asset preparation failed") }) + "\n");
		}
	}
}

// Whether `node cli/native-assets.mjs` started this file, as opposed to another module importing it.
function startedDirectly() {
	if (!process.argv[1]) return false;
	const entry = path.resolve(process.argv[1]);
	const moduleFile = fileURLToPath(import.meta.url);
	if (entry === moduleFile) return true;
	// Node resolves the links in the module's path but leaves the command line's entry path as
	// written, so a launch through a linked folder differs in text only
	try {
		return fs.realpathSync(entry) === fs.realpathSync(moduleFile);
	} catch {
		return false;
	}
}

if (startedDirectly()) {
	// Exit at "quit" or the end of stdin even when a download left a socket or timer behind (the native
	// owner waits for this process)
	main().then(() => process.exit(0), () => process.exit(1));
}
