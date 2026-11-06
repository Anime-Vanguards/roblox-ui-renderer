// The native binary every host runs: `rur-native render` (rur render without the service, the sweep,
// rur compare), `rur-native serve` (the render service, cli/client.mjs), `rur-native engine` (the
// engine CLI, native/src/engine/cli.rs: golden checks, story controls, tests) and the desktop app.
//
// The binary is the native app's build (tools/native-app/build.mjs, located by findNativeApp): a
// build is current only while its fingerprint matches native/ and the build scripts, so an edit to
// the engine sources rebuilds it on the next run (cargo; a few seconds to minutes). Without a Rust
// toolchain the build fails and the host stops with a message saying so (docs/setup.md "Requirements").
//
// RUR_NATIVE_ENGINE=<exe> names an already-resolved binary (a parent process, e.g. the sweep, passes
// it on to its children so they do not re-check the build).
//
// --isolated (RUR_ISOLATED=1, cli/paths.mjs): the binary the state dir's .cache/native-app/manifest.json
// names (checksum verified) is used even when the sources changed since it was built (said in one
// line), so a run is not rebuilt under another checkout's or agent's edits; without a build there,
// the normal build runs.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { APP_DIRECTORIES, CRATE_DIRECTORY, describeNativeApp, findNativeApp, nativeAppStatus } from "../tools/native-app/config.mjs";
import { emulatorLink, emulatorSourceFiles } from "../tools/emulator.mjs";
import { isolated } from "./paths.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// findNativeApp hashes every source file and the binary; hosts ask often (every live session, every
// standby), so the answer is kept while none of those files changed (size + mtime).
let cached = null;
let warnedGiven = false;

function stampFiles() {
	const files = [...APP_DIRECTORIES.map(directory => path.join(directory, "manifest.json")), path.join(CRATE_DIRECTORY, "Cargo.toml"), path.join(CRATE_DIRECTORY, "Cargo.lock"),
		path.join(ROOT, "tools/native-app/config.mjs"), path.join(ROOT, "tools/native-app/build.mjs"), path.join(ROOT, "tools/emulator.mjs"), path.join(ROOT, ".cargo/config.toml")];
	// A linked engine checkout (tools/emulator.mjs link) counts as sources too
	const link = emulatorLink();
	for (const relative of emulatorSourceFiles(link)) files.push(path.join(link, relative));
	const nativeHost = path.join(ROOT, "tools/native-host");
	for (const name of fs.existsSync(nativeHost) ? fs.readdirSync(nativeHost) : []) files.push(path.join(nativeHost, name));
	const walk = dir => {
		for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
			const full = path.join(dir, entry.name);
			if (entry.isDirectory()) walk(full);
			else files.push(full);
		}
	};
	walk(path.join(CRATE_DIRECTORY, "src"));
	return files.map(file => {
		try {
			const stat = fs.statSync(file);
			return `${file}:${stat.size}:${stat.mtimeMs}`;
		} catch {
			return `${file}:-`;
		}
	}).join("\n");
}

// --isolated: the manifest's binary, current or not (null: no usable build, the normal path builds one)
let isolatedChoice;
function isolatedEngine() {
	if (isolatedChoice && fs.existsSync(isolatedChoice)) return isolatedChoice;
	let status = null;
	try {
		status = nativeAppStatus();
	} catch {
		// no usable build
	}
	isolatedChoice = status && (status.state === "current" || status.state === "stale") ? status.executable : null;
	if (status && status.state === "stale") {
		const changed = status.changed?.length ? `, changed: ${status.changed.slice(0, 3).join(", ")}${status.changed.length > 3 ? ", ..." : ""}` : "";
		process.stderr.write(`rur: --isolated: using the engine build in ${status.directory} (built for ${String(status.built).slice(0, 12)}; the sources now fingerprint ${status.fingerprint.slice(0, 12)}${changed})\n`);
	} else if (!isolatedChoice) {
		process.stderr.write(`rur: --isolated: ${describeNativeApp(status)}; building it\n`);
	}
	return isolatedChoice;
}

// The current engine binary, or null when there is no build for the current sources
export function findEngine() {
	const given = process.env.RUR_NATIVE_ENGINE;
	if (given && fs.existsSync(given)) return given;
	if (given && !warnedGiven) {
		warnedGiven = true;
		process.stderr.write(`rur: RUR_NATIVE_ENGINE=${given} does not exist; using the build for the current sources\n`);
	}
	// None yet: the build of ensureEngine makes one, and findEngine after it finds it current
	if (isolated() && isolatedEngine()) return isolatedChoice;
	const stamp = stampFiles();
	if (cached && cached.stamp === stamp && fs.existsSync(cached.executable)) return cached.executable;
	const executable = findNativeApp();
	cached = executable ? { stamp, executable } : null;
	return executable;
}

let building = null;

// The current engine binary, building it first when the sources changed since the last build.
// Throws with an explanation when it cannot be built.
export async function ensureEngine() {
	const found = findEngine();
	if (found) return found;
	if (!building) {
		building = (async () => {
			// Say what is there and why it is not used (a binary built from other sources: both
			// fingerprints and the changed files) before a build that can take minutes
			let status = null;
			try {
				status = nativeAppStatus();
			} catch {
				// described as unknown
			}
			process.stderr.write(`rur: ${describeNativeApp(status)}\n`);
			process.stderr.write(`rur: building the native engine into ${APP_DIRECTORIES[0]} (node tools/native-app/build.mjs)\n`);
			const { buildNativeApp } = await import("../tools/native-app/build.mjs");
			try {
				await buildNativeApp({ stdio: ["ignore", 2, 2] });
			} catch (error) {
				throw new Error(`the native engine could not be built: ${error && error.message ? error.message : error}\n`
					+ "rur runs stories with its native engine (native/, Rust). Install the Rust toolchain (https://rustup.rs, "
					+ "see docs/setup.md), then run `node tools/native-app/build.mjs` (from PowerShell on Windows).");
			}
			const executable = findEngine();
			if (!executable) throw new Error("the native engine was built but its build does not match the current sources (edited during the build?); run again");
			return executable;
		})().finally(() => {
			building = null;
		});
	}
	return building;
}

// `rur-native engine` arguments common to every run: the renderer checkout (data/, the asset cache)
export const engineBaseArgs = () => ["engine", "--repo", ROOT];
