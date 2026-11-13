// Packs stories for the browser build (native/src/web.rs, native/web/host.js): every file the engine
// reads, at virtual paths of the module's in-memory file system, the images the stories can show, the
// engine configuration per story, and the module itself.
//
//   node tools/web/bundle.mjs --story <file> [--story <file> ...] [--out <dir>] [--project <dir>]
//       [--place <name>] [--viewport 1200x900] [--scale 1.5] [--time 1] [--margin 0] [--controls <json>] [--no-snapshots]
//       [--explore '{"*": [{"time": 3, "actions": [{"t": 1.2, "type": "click", "node": "Tabs.Audio"}]}]}']
//
// The file set comes from running each story natively (`rur-native engine ... --dependencies`): the
// modules it required get their contents; the rest of the project tree is listed as empty files (the
// engine mounts the whole tree as instances and reads a script only when it is required, so another
// tab's module that was never required stays empty: the bundle covers what the story requires when it
// mounts and runs for --time, plus every file the scan below finds). Eagerly read files (.meta.json,
// models, project files, snapshots, data/) are included whole. Fonts: every Studio font, the cloud
// fonts the scenes use and the cloud fallback, with the hinted-metric cache entries of all of them.
// Images: the scenes', plus every rbxassetid / rbxasset literal of the required sources (what other
// tabs and states show), resolved offline as the native app's asset helper resolves them
// (cli/assets.mjs), recorded in an image map the module reads (RUR_WEB_IMAGES).
//
// The bundle holds Roblox Studio's installed fonts, which belong to Roblox or its licensors: a bundle
// is for your own machine and must never be committed, published or shared.
//
// Output (<out>, required, outside this checkout):
//   index.html          a plain test page (tools/web/serve.mjs serves the directory)
//   rur-host.js         the page-side host (native/web/host.js)
//   rur-web.js          the Emscripten loader
//   rur-web.gz.wasm     the module, gzipped inside a WebAssembly custom section (gzipWasm; the page
//                       unwraps it and inflates it with DecompressionStream)
//   bundle.json         the file index and the story configurations
//   bundle.<n>.gz.wasm  the packed files: parts of at most 12 MB before compression, wrapped the same way
//   bundle-summary.json sizes, what was left out and why (for the caller, not the page)
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { isMain } from "../../cli/entry.mjs";

import { placeOf, placeRoots, readPlaces } from "../../cli/places.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");
const PART_BYTES = 12 * 1024 * 1024;
// engine/text/fonts.rs FALLBACK_FILES / CLOUD_FALLBACK_FILES (the registry loads the whole chain)
const FALLBACK_FONTS = ["SourceSansPro-Regular.ttf", "Arimo-Regular.ttf", "RobloxEmoji.ttf", "TwemojiMozilla.ttf", "NotoSansThaiUI-Regular.ttf", "NotoNaskhArabicUI-Regular.ttf", "NotoSansDevanagariUI-Regular.ttf"];
const CLOUD_FALLBACK_IDS = ["123924484074402"];
// engine/text/hinted.rs: GENERATOR_VERSION, the compiled-in FreeType, the default size range
const HINTED_GENERATOR = 1;
const FREETYPE_VERSION = "2.14.2";
const HINTED_MAXIMUM = 2048;

const sha256 = data => crypto.createHash("sha256").update(data).digest("hex");

// Gzipped bytes as a valid WebAssembly file: the header and one custom section ("rur.gz") holding them.
// Some static hosts serve .wasm but no archive type; native/web/host.js unwraps and inflates it.
export function gzipWasm(bytes) {
	const gz = zlib.gzipSync(bytes, { level: 9 });
	const leb = value => {
		const out = [];
		do {
			let byte = value & 0x7f;
			value = Math.floor(value / 128);
			if (value) byte |= 0x80;
			out.push(byte);
		} while (value);
		return Buffer.from(out);
	};
	const name = Buffer.from("rur.gz");
	const payload = Buffer.concat([leb(name.length), name, gz]);
	return Buffer.concat([Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x00]), leb(payload.length), payload]);
}
const posix = file => file.split(path.sep).join("/");

// The directories the page makes before it writes any file (host.js mkdirTree): the project's own
// (empty folders included, the tree lists them) and the parent of every file it writes. A file outside
// every base sits under /ext/<hash of the file>/, a directory no mapped directory produces (a directory
// outside the bases hashes its own path), so only the file itself gives it.
export function directoryList(directories, files) {
	const parents = files.map(file => file.slice(0, file.lastIndexOf("/")) || "/");
	return [...new Set([...directories.filter(directory => !directory.startsWith("/ext/")), ...parents])].sort();
}
// A scene list as `rur-native engine --out` writes it: an empty one is `{}` (cli.rs empty_arrays_as_objects)
const list = value => (Array.isArray(value) ? value : []);

export function defaultProject() {
	return process.env.RUR_PROJECT ? path.resolve(process.env.RUR_PROJECT) : path.resolve(REPO, "examples", "project");
}

function parseArgs(argv) {
	const args = { stories: [], project: defaultProject(), viewport: "1200x900", scale: 1.5, time: 1, margin: 0, snapshots: true, controls: {} };
	for (let i = 0; i < argv.length; i++) {
		const key = argv[i];
		const value = () => {
			if (i + 1 >= argv.length) throw new Error(`${key} needs a value`);
			return argv[++i];
		};
		if (key === "--story") args.stories.push(value());
		else if (key === "--project") args.project = path.resolve(value());
		else if (key === "--place") args.place = value();
		else if (key === "--out") args.out = path.resolve(value());
		else if (key === "--viewport") args.viewport = value();
		else if (key === "--scale") args.scale = Number(value());
		else if (key === "--time") args.time = Number(value());
		else if (key === "--margin") args.margin = Number(value());
		else if (key === "--controls") args.controls = JSON.parse(value());
		else if (key === "--no-snapshots") args.snapshots = false;
		else if (key === "--cloud-fallback") args.cloudFallback = true;
		else if (key === "--explore") {
			const text = value();
			args.explore = JSON.parse(text.trim().startsWith("{") ? text : fs.readFileSync(text, "utf8"));
		}
		else if (key === "--engine") args.engine = path.resolve(value());
		else if (key === "--web") args.web = path.resolve(value());
		else throw new Error(`unknown argument ${key}`);
	}
	if (!args.stories.length) throw new Error("usage: node tools/web/bundle.mjs --story <file> [--story <file> ...] [--out <dir>]");
	return args;
}

// Bundles `stories` (paths, absolute or relative to the project) into `out`. Returns the summary.
// `place`: a place of the project (its source roots, as `rur render --place`; cli/places.mjs), one for
// every story or an array of one per story; a story without one runs in the place whose roots hold it.
// A story whose native run fails to mount stops the bundle with the engine's message.
// `explore`: {<story index> | "*": [{actions: [...], time}]}: extra native runs whose required modules
// and scene images join the bundle (e.g. a click on every tab), since a module another tab requires
// later would otherwise be an empty file in the page.
export async function bundle({ stories, project = defaultProject(), out, place, viewport = "1200x900", scale = 1.5, time = 1, margin = 0, controls = {}, snapshots = true, explore = {}, cloudFallback = false, engine, web, log = message => console.error(message) }) {
	const { stateDir } = await import(pathToFileURL(path.join(REPO, "cli", "paths.mjs")).href);
	const { resolveAll, studioContentRoots } = await import(pathToFileURL(path.join(REPO, "cli", "assets.mjs")).href);
	const { findNativeApp } = await import(pathToFileURL(path.join(REPO, "tools", "native-app", "config.mjs")).href);
	const { WEB_DIRECTORY, webModuleStatus } = await import(pathToFileURL(path.join(REPO, "tools", "native-app", "build.mjs")).href);
	const state = stateDir();
	project = path.resolve(project);
	engine ||= findNativeApp();
	if (!engine) throw new Error("no current native build: run `node tools/native-app/build.mjs` (PowerShell on Windows)");
	web ||= WEB_DIRECTORY;
	if (!fs.existsSync(path.join(web, "rur_web.wasm"))) throw new Error(`no web module in ${web}: run \`node tools/native-app/build.mjs --wasm\``);
	const webStatus = webModuleStatus();
	if (web === WEB_DIRECTORY && webStatus.state !== "current") log(`rur: the web module in ${web} was built from other sources (${webStatus.state}); rebuild with \`node tools/native-app/build.mjs --wasm\``);
	const studio = studioContentRoots().find(root => fs.existsSync(path.join(root, "content", "fonts")));
	if (!studio) throw new Error("no Roblox Studio install with content/fonts (fonts and rbxasset:// images come from it)");
	const files = stories.map(story => path.resolve(fs.existsSync(story) ? story : path.join(project, story)));
	for (const file of files) if (!fs.existsSync(file)) throw new Error(`no such story: ${file}`);
	// Never inside this checkout: the bundle holds Roblox Studio's fonts, which must not be committed or published
	if (!out) throw new Error("--out <dir> is required: a bundle holds Roblox Studio's fonts, so write it outside this checkout");
	out = path.resolve(out);
	const inside = path.relative(REPO, out);
	if (!inside || (!inside.startsWith("..") && !path.isAbsolute(inside))) throw new Error(`${out} is inside this checkout: a bundle holds Roblox Studio's fonts, which must not be committed or published; pick a directory outside it`);
	log("rur: the bundle holds Roblox Studio's installed fonts (Roblox's or licensed to Roblox): keep it on this machine, never commit, publish or share it");
	const [vw, vh] = viewport.split("x").map(Number);

	// Virtual paths: /project (the project), /rur (this checkout: data/), /state (the state dir: asset
	// cache, hinted metrics, snapshots), /studio (the Studio version root)
	const bases = [[state, "/state"], [project, "/project"], [REPO, "/rur"], [path.resolve(studio), "/studio"]];
	const virtual = file => {
		const absolute = path.resolve(file);
		for (const [base, target] of bases) {
			const relative = path.relative(base, absolute);
			if (!relative.startsWith("..") && !path.isAbsolute(relative)) return relative ? `${target}/${posix(relative)}` : target;
		}
		return `/ext/${sha256(absolute).slice(0, 12)}/${path.basename(absolute)}`;
	};

	const contents = new Set(); // absolute files packed with their bytes
	const listed = new Set(); // absolute files that exist in the tree (empty unless in contents)
	const dirs = new Set();
	const include = file => {
		if (file && fs.existsSync(file) && fs.statSync(file).isFile()) {
			contents.add(path.resolve(file));
			dirs.add(path.dirname(path.resolve(file)));
		}
	};
	const listTree = (directory, wantContent) => {
		let entries;
		try {
			entries = fs.readdirSync(directory, { withFileTypes: true });
		} catch {
			return;
		}
		dirs.add(path.resolve(directory));
		for (const entry of entries) {
			const full = path.join(directory, entry.name);
			if (entry.isDirectory()) listTree(full, wantContent);
			else if (entry.isFile()) {
				listed.add(path.resolve(full));
				if (wantContent(full, entry.name)) include(full);
			}
		}
	};

	// 1. Native runs: scenes (fonts, images) and required sources
	fs.mkdirSync(out, { recursive: true });
	const configs = [];
	const required = new Set();
	const scenes = [];
	const allRoots = new Set();
	let snapshotDir = null;
	for (const [index, story] of files.entries()) {
		const storyPlace = (Array.isArray(place) ? place[index] : place) || placeOf(project, story);
		if (!readPlaces(project).some(item => item.name === storyPlace)) throw new Error(`the project has no place ${storyPlace} (cli/places.mjs)`);
		const roots = placeRoots(project, storyPlace).filter(dir => fs.existsSync(dir));
		roots.forEach(root => allRoots.add(root));
		// No implicit Studio snapshots (runtime.rs launch)
		const snapshot = null;
		if (snapshot) snapshotDir = snapshot;
		// The mounted story, then each exploration timeline (`explore`: input that reaches other tabs and
		// states, whose modules and images the page needs too)
		const runs = [{ time, actions: null }, ...(explore[index] || explore["*"] || [])];
		for (const [runIndex, run] of runs.entries()) {
			const scenePath = path.join(out, `native-scene-${index}-${runIndex}.json`);
			const depsPath = path.join(out, `native-deps-${index}-${runIndex}.json`);
			const args = ["engine", "--story", story, "--roots", roots.join(";"), "--viewport", viewport, "--scale", String(scale), "--time", String(run.time ?? time), "--out", scenePath, "--dependencies", depsPath, "--repo", REPO];
			if (margin) args.push("--margin", String(margin));
			if (Object.keys(controls).length) args.push("--controls", JSON.stringify(controls));
			if (run.actions && run.actions.length) args.push("--actions", JSON.stringify(run.actions));
			if (snapshot) args.push("--snapshot-dir", snapshot);
			else args.push("--no-snapshots");
			const started = Date.now();
			const result = spawnSync(engine, args, { cwd: REPO, encoding: "utf8", maxBuffer: 1 << 28, windowsHide: true });
			if (!fs.existsSync(depsPath)) {
				if (runIndex === 0) throw new Error(`native run of ${path.basename(story)} failed (${result.status}): ${(result.stderr || "").slice(-2000)}`);
				log(`${path.basename(story)}: exploration run ${runIndex} failed, left out: ${(result.stderr || "").trim().split("\n").pop()}`);
				continue;
			}
			log(`${path.basename(story)}: native run${runIndex ? ` (exploration ${runIndex})` : ""} ${((Date.now() - started) / 1000).toFixed(1)} s`);
			const scene = fs.existsSync(scenePath) ? JSON.parse(fs.readFileSync(scenePath, "utf8")) : null;
			if (scene && scene.status === "error") {
				const failure = `${path.basename(story)} (${storyPlace}) failed to mount${runIndex ? ` in exploration run ${runIndex}` : ""}: ${scene.failure || "no message"}`;
				if (!runIndex) throw new Error(failure);
				log(failure);
			}
			if (scene) scenes.push(scene);
			for (const file of JSON.parse(fs.readFileSync(depsPath, "utf8")).files) required.add(path.resolve(file));
			fs.rmSync(scenePath, { force: true });
			fs.rmSync(depsPath, { force: true });
		}
		configs.push({
			name: path.basename(story).replace(/\.story\.luau$|\.luau$/, ""),
			path: posix(path.relative(project, story)),
			config: { story: virtual(story), roots: roots.map(virtual), viewport: [vw, vh], scale, time, margin, snapshotDir: snapshot ? virtual(snapshot) : null, repo: "/rur", controls },
		});
	}

	// 2. Project tree: required scripts and eagerly read files, the rest listed
	const eager = (full, name) => {
		const lower = name.toLowerCase();
		return required.has(path.resolve(full)) || lower.endsWith(".meta.json") || lower.endsWith(".rbxm") || lower.endsWith(".rbxmx");
	};
	for (const root of allRoots) listTree(root, eager);
	for (const file of ["rur.json", "default.project.json"]) include(path.join(project, file));
	listTree(path.join(project, "external"), eager);
	for (const file of required) include(file);
	if (snapshotDir) listTree(snapshotDir, () => true);
	listTree(path.join(REPO, "data"), () => true);

	// 3. Fonts: every Studio font and family file; cloud fonts the scenes use, the cloud fallback and
	// the cloud font index; hinted metrics for all of them
	const cloudFonts = new Set();
	for (const scene of scenes) {
		for (const font of list(scene.fonts)) {
			if (font && font.file && path.resolve(font.file).startsWith(path.resolve(state))) cloudFonts.add(path.resolve(font.file));
		}
	}
	listTree(path.join(studio, "content", "fonts"), () => true);
	listTree(path.join(studio, "ExtraContent", "LuaPackages", "Packages", "_Index", "BuilderIcons"), (full, name) => /\.(ttf|otf|ttc|json)$/i.test(name));
	const assetDir = path.join(state, "assets");
	dirs.add(assetDir);
	for (const entry of fs.existsSync(assetDir) ? fs.readdirSync(assetDir) : []) {
		if (!/\.(ttf|otf|ttc)$/i.test(entry) && entry !== "fonts.json") continue;
		const full = path.join(assetDir, entry);
		listed.add(full);
		// The cloud fallback (a 16 MB CJK face) only when a scene draws with it or `cloudFallback`
		if (entry === "fonts.json" || cloudFonts.has(full) || cloudFallback && CLOUD_FALLBACK_IDS.some(id => entry.startsWith(`${id}.`))) include(full);
	}
	const fingerprint = sha256(JSON.stringify(["rur-hinted", HINTED_GENERATOR, FREETYPE_VERSION, process.env.FREETYPE_PROPERTIES || ""]));
	const hintedMissing = [];
	for (const font of [...contents].filter(file => /\.(ttf|otf|ttc)$/i.test(file))) {
		const key = sha256(`2:${fingerprint}:${HINTED_MAXIMUM}:${sha256(fs.readFileSync(font))}`);
		const cached = path.join(state, "hinted", `${key}.json`);
		if (fs.existsSync(cached)) include(cached);
		else hintedMissing.push(path.basename(font));
	}

	// 4. Images: the scenes', then every literal of the required sources (other tabs and states)
	const uris = new Set();
	for (const scene of scenes) {
		for (const image of list(scene.images)) if (typeof image === "string") uris.add(image);
		for (const node of list(scene.nodes)) {
			if (node.image && node.image.uri) uris.add(node.image.uri);
			for (const key of ["topImage", "midImage", "bottomImage"]) if (node.scroll && node.scroll[key]) uris.add(node.scroll[key]);
		}
	}
	const sceneUris = new Set(uris);
	for (const file of required) {
		let text;
		try {
			text = fs.readFileSync(file, "utf8");
		} catch {
			continue;
		}
		for (const match of text.matchAll(/rbxassetid:\/\/\d+|rbxasset:\/\/[\w\-./@]+\.(?:png|jpg|jpeg)/gi)) uris.add(match[0]);
		for (const match of text.matchAll(/"(\d{8,})"|\b(\d{9,})\b/g)) uris.add(`rbxassetid://${match[1] || match[2]}`);
	}
	const resolved = await resolveAll([...uris], { offline: true });
	const imageMap = {};
	const sceneMissing = [];
	for (const [uri, result] of resolved) {
		if (result.file && /^image\//.test(result.mime || "image/")) {
			include(result.file);
			imageMap[uri] = virtual(result.file);
			// export.rs probes @2x / @3x variants of rbxasset images
			if (/^rbxasset:\/\//i.test(uri)) for (const suffix of ["@2x", "@3x"]) include(result.file.replace(/(\.\w+)$/, `${suffix}$1`));
		} else if (sceneUris.has(uri)) {
			imageMap[uri] = { error: result.error || "not an image" };
			sceneMissing.push(uri);
		}
		// literal-scan hits that are no images (numbers) are left out
	}

	// 5. Pack
	const entries = [];
	const chunks = [];
	let offset = 0;
	for (const file of [...contents].sort()) {
		const bytes = fs.readFileSync(file);
		entries.push([virtual(file), offset, bytes.length]);
		chunks.push(bytes);
		offset += bytes.length;
	}
	const mapBytes = Buffer.from(JSON.stringify(imageMap));
	entries.push(["/rur/web-images.json", offset, mapBytes.length]);
	chunks.push(mapBytes);
	offset += mapBytes.length;
	const empty = [...listed].filter(file => !contents.has(file)).map(virtual).sort();
	const dirList = directoryList([...dirs].map(virtual), [...entries.map(([file]) => file), ...empty]);
	const packed = Buffer.concat(chunks);
	for (const stale of fs.readdirSync(out)) if (/^bundle\.\d+\.(bin|gz)(\.gz|\.wasm)?$|^rur-web\.(wasm\.gz|gz\.wasm)$/.test(stale)) fs.rmSync(path.join(out, stale));
	const parts = [];
	for (let at = 0, part = 0; at < packed.length || part === 0; at += PART_BYTES, part++) {
		const name = `bundle.${part}.gz.wasm`;
		fs.writeFileSync(path.join(out, name), gzipWasm(packed.subarray(at, Math.min(packed.length, at + PART_BYTES))));
		parts.push(name);
	}
	const wasm = fs.readFileSync(path.join(web, "rur_web.wasm"));
	const module = "rur-web.gz.wasm";
	fs.writeFileSync(path.join(out, module), gzipWasm(wasm));
	const env = { RUR_STATE_DIR: "/state", RUR_ROBLOX_CONTENT: "/studio/content", RUR_WEB_IMAGES: "/rur/web-images.json" };
	// `bytes`: what the page downloads (compressed), for its progress
	const bytes = [...parts, module].reduce((sum, name) => sum + fs.statSync(path.join(out, name)).size, 0);
	const index = { schema: 1, stories: configs, env, bytes, module, dirs: dirList, files: entries, empty, parts };
	fs.writeFileSync(path.join(out, "bundle.json"), JSON.stringify(index));
	fs.copyFileSync(path.join(web, "rur-web.js"), path.join(out, "rur-web.js"));
	fs.copyFileSync(path.join(REPO, "native", "web", "host.js"), path.join(out, "rur-host.js"));
	fs.copyFileSync(path.join(REPO, "native", "web", "index.html"), path.join(out, "index.html"));
	const size = name => fs.statSync(path.join(out, name)).size;
	const summary = {
		out,
		stories: configs.map(entry => entry.path),
		files: entries.length,
		emptyFiles: empty.length,
		requiredModules: required.size,
		images: Object.values(imageMap).filter(value => typeof value === "string").length,
		sceneImagesMissing: sceneMissing,
		hintedMissing,
		bytes: {
			packed: packed.length,
			parts: parts.map(name => [name, size(name)]),
			index: size("bundle.json"),
			wasm: wasm.length,
			wasmGz: size(module),
			loader: size("rur-web.js"),
			host: size("rur-host.js"),
		},
	};
	summary.bytes.total = summary.bytes.parts.reduce((sum, [, bytes]) => sum + bytes, 0) + summary.bytes.index + summary.bytes.wasmGz + summary.bytes.loader + summary.bytes.host;
	fs.writeFileSync(path.join(out, "bundle-summary.json"), JSON.stringify(summary, null, "\t"));
	return summary;
}

if (isMain(import.meta.url)) {
	const args = parseArgs(process.argv.slice(2));
	const summary = await bundle(args);
	console.log(JSON.stringify(summary, null, "\t"));
}
