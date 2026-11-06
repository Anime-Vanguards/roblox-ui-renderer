// Leftovers in the state dir (cli/paths.mjs) that nothing reads at run time. `rur doctor` reports them,
// `rur doctor --fix` removes them:
//
//   native-app/rur-native-<hash>.exe      engine binaries the manifest no longer names (a build's new
//                                         binary is kept until its manifest is written; a binary in use
//                                         is skipped)
//   freetype/                             the retired Python glyph pipeline's FreeType build (glyphs and
//                                         hinted metrics are rasterized in the engine since 2026-09-30)
//   hinted/*.json of version 1            hinted metrics the Python generator wrote (the engine writes
//                                         version 2, keyed by its compiled-in FreeType)
//   native-host/                          the retired Lune host's build
//   storybook-*.log, serve.log.*          logs of the retired browser storybook
//
// Kept: native-app/target (cargo's incremental build: a rebuild after an edit takes seconds instead of
// minutes), native-app/source (the pinned sources the build compiles) and every cache (assets, fonts,
// hinted metrics, Studio snapshots).

import fs from "node:fs";
import path from "node:path";
import { stateDir } from "./paths.mjs";

// Bytes and files under `file`
function measure(file) {
	let bytes = 0;
	let files = 0;
	const walk = entry => {
		let stat;
		try {
			stat = fs.lstatSync(entry);
		} catch {
			return;
		}
		if (stat.isDirectory()) {
			for (const name of fs.readdirSync(entry)) walk(path.join(entry, name));
		} else {
			bytes += stat.size;
			files++;
		}
	};
	walk(file);
	return { bytes, files };
}

const list = directory => {
	try {
		return fs.readdirSync(directory, { withFileTypes: true });
	} catch {
		return [];
	}
};

// Engine binaries in `directory` (<state dir>/native-app) its manifest does not name
function nativeAppLeftovers(directory, { minAgeMs = 10 * 60 * 1000 } = {}) {
	let manifest, manifestTime;
	try {
		const file = path.join(directory, "manifest.json");
		manifest = JSON.parse(fs.readFileSync(file, "utf8"));
		manifestTime = fs.statSync(file).mtimeMs;
	} catch {
		return [];
	}
	const found = [];
	for (const entry of list(directory)) {
		if (!entry.isFile() || !/^rur-native-[0-9a-f]{64}(\.exe)?$/.test(entry.name) || entry.name === manifest.executable) continue;
		const file = path.join(directory, entry.name);
		const time = fs.statSync(file).mtimeMs;
		// A build writes its binary before its manifest: only binaries older than both the manifest and a margin
		if (time < manifestTime && Date.now() - time > minAgeMs) found.push({ path: file, what: "superseded engine binary" });
	}
	return found;
}

// Hinted metric files of the Python generator (version 1)
function hintedLeftovers(directory) {
	const found = [];
	for (const entry of list(directory)) {
		if (!entry.isFile() || !entry.name.endsWith(".json")) continue;
		const file = path.join(directory, entry.name);
		try {
			const fd = fs.openSync(file, "r");
			const head = Buffer.alloc(16);
			fs.readSync(fd, head, 0, 16, 0);
			fs.closeSync(fd);
			if (head.toString("latin1").startsWith('{"version":1,')) found.push({ path: file, what: "hinted metrics of the retired Python generator" });
		} catch {
			// unreadable: left alone
		}
	}
	return found;
}

// [{ path, what, bytes, files }] of the state dir's leftovers
export function stateLeftovers(dir = stateDir()) {
	const found = [...nativeAppLeftovers(path.join(dir, "native-app")), ...hintedLeftovers(path.join(dir, "hinted"))];
	if (fs.existsSync(path.join(dir, "freetype"))) found.push({ path: path.join(dir, "freetype"), what: "retired Python FreeType build" });
	if (fs.existsSync(path.join(dir, "native-host"))) found.push({ path: path.join(dir, "native-host"), what: "retired Lune host build" });
	for (const entry of list(dir)) {
		if (entry.isFile() && /^storybook-.*\.log$/.test(entry.name)) found.push({ path: path.join(dir, entry.name), what: "retired storybook log" });
	}
	return found.map(item => ({ ...item, ...measure(item.path) }));
}

// Removes them: { removed: [items], failed: [{ ...item, error }], bytes, files } (bytes / files removed)
export function removeLeftovers(items) {
	const result = { removed: [], failed: [], bytes: 0, files: 0 };
	for (const item of items) {
		try {
			fs.rmSync(item.path, { recursive: true, force: true, maxRetries: 2 });
			result.removed.push(item);
			result.bytes += item.bytes || 0;
			result.files += item.files || 0;
		} catch (error) {
			// e.g. a binary a running engine uses (Windows)
			result.failed.push({ ...item, error: error.code || error.message });
		}
	}
	return result;
}

export const megabytes = bytes => `${(bytes / 1024 / 1024).toFixed(1)} MB`;
