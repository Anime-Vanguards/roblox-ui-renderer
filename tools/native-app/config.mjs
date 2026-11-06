import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { ROOT, HOST_FILES, normalizedSource } from "../native-host/config.mjs";
import { emulatorLink, emulatorSourceFiles } from "../emulator.mjs";
import { DEFAULT_STATE_DIR, isDefaultStateDir, stateDir } from "../../cli/paths.mjs";

export { ROOT };
// Builds go to the state dir (cli/paths.mjs: RUR_STATE_DIR, default .cache); a state dir elsewhere
// also uses the checkout's own .cache/native-app build when that one matches the sources.
export const APP_DIRECTORY = path.join(stateDir(), "native-app");
export const APP_MANIFEST = path.join(APP_DIRECTORY, "manifest.json");
export const SOURCE_DIRECTORY = path.join(APP_DIRECTORY, "source");
export const TARGET_DIRECTORY = path.join(APP_DIRECTORY, "target");
export const CRATE_DIRECTORY = path.join(ROOT, "native");
export const APP_DIRECTORIES = isDefaultStateDir() ? [APP_DIRECTORY] : [APP_DIRECTORY, path.join(DEFAULT_STATE_DIR, "native-app")];

function files(directory) {
	return fs.readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
		const name = path.join(directory, entry.name);
		return entry.isDirectory() ? files(name) : [name];
	}).sort();
}

// sha256 of files by size and modification time, kept in the state dir (hash-cache.json), so a
// command that checks the build (every CLI start) hashes only what changed since the last check
const HASH_CACHE = path.join(APP_DIRECTORY, "hash-cache.json");
let hashCache = null;
let hashCacheDirty = false;

function cachedHash(file, kind, compute) {
	if (!hashCache) {
		try {
			hashCache = JSON.parse(fs.readFileSync(HASH_CACHE, "utf8"));
		} catch {
			hashCache = {};
		}
	}
	const stat = fs.statSync(file);
	const key = `${kind}:${file}`;
	const known = hashCache[key];
	if (known && known[0] === stat.size && known[1] === stat.mtimeMs) return known[2];
	const hash = compute(fs.readFileSync(file));
	hashCache[key] = [stat.size, stat.mtimeMs, hash];
	hashCacheDirty = true;
	return hash;
}

function saveHashCache() {
	if (!hashCacheDirty) return;
	hashCacheDirty = false;
	try {
		fs.mkdirSync(APP_DIRECTORY, { recursive: true });
		const temporary = `${HASH_CACHE}.${process.pid}.tmp`;
		fs.writeFileSync(temporary, JSON.stringify(hashCache));
		fs.renameSync(temporary, HASH_CACHE);
	} catch {
		// a cache only
	}
}

// { "<repo-relative path>": sha256 } of every file the build depends on, line endings normalized
// (normalizedSource), so the same sources hash the same under core.autocrlf=true, in a copy and on Linux.
// The engine is the commit native/Cargo.toml and Cargo.lock pin; with a local checkout linked
// (tools/emulator.mjs link), that checkout's sources count as `emulator/<path>`.
export function appSourceHashes() {
	const names = ["native/Cargo.toml", "native/Cargo.lock", "native/.cargo/config.toml", "native/build.rs", "native/web/egl_shim.c", "tools/native-app/config.mjs", "tools/native-app/build.mjs",
		"tools/emulator.mjs", ...HOST_FILES.map(name => `tools/native-host/${name}`)];
	const hash = absolute => cachedHash(absolute, "source", bytes => createHash("sha256").update(normalizedSource(bytes)).digest("hex"));
	const result = {};
	for (const absolute of [...names.map(name => path.join(ROOT, name)), ...files(path.join(CRATE_DIRECTORY, "src"))]) {
		result[path.relative(ROOT, absolute).replaceAll("\\", "/")] = hash(absolute);
	}
	const link = emulatorLink();
	for (const relative of emulatorSourceFiles(link)) {
		result[`emulator/${relative}`] = hash(path.join(link, relative));
	}
	saveHashCache();
	return result;
}

export function appFingerprint(profile = "release", hashes = appSourceHashes()) {
	const hash = createHash("sha256").update("rur-native-app/2\0").update(profile).update("\0");
	for (const name of Object.keys(hashes).sort()) hash.update(name).update("\0").update(hashes[name]).update("\n");
	return hash.digest("hex");
}

// The files whose hashes differ between a build's manifest and the sources now
function changedFiles(before, now) {
	if (!before) return null;
	const names = new Set([...Object.keys(before), ...Object.keys(now)]);
	return [...names].filter(name => before[name] !== now[name]).sort();
}

// What is built against what the sources are now:
//   { state: "current", executable, directory, fingerprint }
//   { state: "stale", executable (present, not current), directory, built, fingerprint, changed: [files] | null }
//   { state: "missing" | "invalid", directory, reason }
export function nativeAppStatus({ profile } = {}) {
	let hashes = null;
	let best = null;
	for (const directory of APP_DIRECTORIES) {
		let manifest;
		try {
			manifest = JSON.parse(fs.readFileSync(path.join(directory, "manifest.json"), "utf8"));
		} catch {
			best ||= { state: "missing", directory, reason: "no build" };
			continue;
		}
		const invalid = reason => ({ state: "invalid", directory, reason });
		let status;
		if (manifest.schema !== 1 || manifest.platform !== process.platform || manifest.arch !== process.arch) status = invalid(`built for ${manifest.platform}/${manifest.arch}`);
		else if (profile && manifest.profile !== profile) status = invalid(`a ${manifest.profile} build (needs ${profile})`);
		else if (path.basename(String(manifest.executable)) !== manifest.executable) status = invalid("bad manifest");
		else {
			const executable = path.join(directory, manifest.executable);
			let digest = null;
			try {
				digest = cachedHash(executable, "binary", bytes => createHash("sha256").update(bytes).digest("hex"));
				saveHashCache();
			} catch {
				// removed
			}
			if (digest !== manifest.sha256) status = invalid(digest ? "binary checksum differs from its manifest" : "binary missing");
			else {
				hashes ||= appSourceHashes();
				const fingerprint = appFingerprint(manifest.profile, hashes);
				if (manifest.buildFingerprint === fingerprint) return { state: "current", executable, directory, fingerprint };
				status = { state: "stale", executable, directory, built: manifest.buildFingerprint, fingerprint, changed: changedFiles(manifest.files, hashes) };
			}
		}
		// Report the most useful miss: a present-but-stale binary over an invalid one over none
		const rank = { stale: 3, invalid: 2, missing: 1 };
		if (!best || rank[status.state] > rank[best.state]) best = status;
	}
	return best;
}

// One line about a status that is not current, e.g. for a notice before a rebuild
export function describeNativeApp(status) {
	if (!status) return "native engine: unknown";
	if (status.state === "current") return `native engine: current (${status.fingerprint.slice(0, 12)}) ${status.executable}`;
	if (status.state === "stale") {
		const changed = status.changed
			? status.changed.length ? `; changed: ${status.changed.slice(0, 4).join(", ")}${status.changed.length > 4 ? ` (+${status.changed.length - 4} more)` : ""}` : ""
			: "; the build predates per-file hashes";
		return `native engine: binary present (built for ${String(status.built).slice(0, 12)}) but the sources fingerprint ${status.fingerprint.slice(0, 12)}${changed}. `
			+ `To use it anyway: RUR_NATIVE_ENGINE=${status.executable}`;
	}
	return `native engine: ${status.reason} in ${status.directory}`;
}

export function findNativeApp({ profile } = {}) {
	try {
		const status = nativeAppStatus({ profile });
		return status && status.state === "current" ? status.executable : null;
	} catch {
		return null;
	}
}
