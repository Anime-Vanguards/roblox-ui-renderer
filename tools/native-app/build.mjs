import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { digest, prepareNativeSource, runNativeCommand } from "../native-host/source.mjs";
import { acquireBuildLock } from "../native-host/build-lock.mjs";
import { APP_DIRECTORY, APP_MANIFEST, SOURCE_DIRECTORY, TARGET_DIRECTORY, CRATE_DIRECTORY, appFingerprint, appSourceHashes, nativeAppStatus } from "./config.mjs";
import { emulatorLink } from "../emulator.mjs";
import { withoutCookie } from "../../cli/assets.mjs";
import { isMain } from "../../cli/entry.mjs";

// Cargo.lock is followed exactly, except while a local engine checkout is linked (tools/emulator.mjs):
// cargo then rewrites the lock's emulator entry to the path package
const locked = () => (emulatorLink() ? [] : ["--locked"]);

// Another build holding the lock is waited for (up to RUR_BUILD_WAIT seconds, default 20 minutes)
// instead of failing: it is usually building these same sources, and its result is then reused.
async function waitForBuildLock(filename, log) {
	const deadline = Date.now() + (Number(process.env.RUR_BUILD_WAIT) || 1200) * 1000;
	let noted = false;
	for (;;) {
		try {
			return await acquireBuildLock(filename);
		} catch (error) {
			if (!/Another native build owns|metadata .* is incomplete/.test(String(error.message)) || Date.now() > deadline) throw error;
			if (!noted) log("rur: waiting for another native build to finish");
			noted = true;
			await new Promise(resolve => setTimeout(resolve, 500));
		}
	}
}

// `stdio`: the child processes' stdio ("inherit"; cli/engine.mjs passes ["ignore", 2, 2] so a host's
// stdout stays its own)
export async function buildNativeApp({ debug = false, prepareOnly = false, verifyOnly = false, stdio = "inherit" } = {}) {
	const log = stdio === "inherit" ? console.log : console.error;
	fs.mkdirSync(APP_DIRECTORY, { recursive: true });
	const releaseLock = await waitForBuildLock(path.join(APP_DIRECTORY, "build.lock"), log);
	try {
		const profile = debug || verifyOnly ? "debug" : "release";
		// A build that finished while this one waited for the lock may already be current
		if (!prepareOnly && !verifyOnly) {
			const status = nativeAppStatus({ profile });
			if (status.state === "current") return status.executable;
		}
		// Sources edited during a build (other work in the same tree) build again, up to 3 times: with
		// incremental release builds a rebuild costs seconds
		for (let attempt = 1; ; attempt++) {
			try {
				return await buildOnce({ profile, prepareOnly, verifyOnly, stdio, log });
			} catch (error) {
				if (!(error instanceof SourcesChanged) || attempt >= 3) throw error;
				log(`rur: ${error.message}; building again`);
			}
		}
	} finally {
		await releaseLock();
	}
}

class SourcesChanged extends Error {}

// The FreeType source native/build.rs compiles: the state dir's extraction (build.rs alone looks in the
// checkout's own .cache, which a RUR_STATE_DIR elsewhere leaves empty), unless RUR_FREETYPE_SRC names one
const freetypeSource = () => process.env.RUR_FREETYPE_SRC || path.join(SOURCE_DIRECTORY, "freetype-2.14.2");

async function buildOnce({ profile, prepareOnly, verifyOnly, stdio, log }) {
	{
		// Per-file hashes go into the manifest, so a stale build can say what changed (describeNativeApp)
		const hashes = appSourceHashes();
		const fingerprint = appFingerprint(profile, hashes);
		await prepareNativeSource(SOURCE_DIRECTORY, { stdio });
		if (appFingerprint(profile) !== fingerprint) throw new SourcesChanged("native sources changed during preparation");
		if (prepareOnly) return null;
		const env = { ...withoutCookie(), CARGO_TARGET_DIR: TARGET_DIRECTORY, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS || String(os.availableParallelism()), RUR_FREETYPE_SRC: freetypeSource() };
		const profileArgs = profile === "debug" ? [] : ["--release"];
		if (verifyOnly) {
			runNativeCommand("cargo", ["test", ...locked()], CRATE_DIRECTORY, env);
			runNativeCommand(process.execPath, [path.join(CRATE_DIRECTORY, "../tools/tests/native-assets.mjs")], CRATE_DIRECTORY, env);
			if (appFingerprint(profile) !== fingerprint) throw new SourcesChanged("native sources changed during the tests");
			return null;
		}
		runNativeCommand("cargo", ["build", ...locked(), ...profileArgs], CRATE_DIRECTORY, env, stdio);
		if (appFingerprint(profile) !== fingerprint) throw new SourcesChanged("native sources changed during the build");
		const built = path.join(TARGET_DIRECTORY, profile, process.platform === "win32" ? "rur-native.exe" : "rur-native");
		const binary = fs.readFileSync(built);
		const sha256 = digest(binary);
		const executable = `rur-native-${sha256}${process.platform === "win32" ? ".exe" : ""}`;
		const installed = path.join(APP_DIRECTORY, executable);
		if (!fs.existsSync(installed)) fs.writeFileSync(installed, binary, { mode: 0o755 });
		if (digest(fs.readFileSync(installed)) !== sha256) throw new Error("Native app executable checksum mismatch");
		runNativeCommand(installed, ["--version"], CRATE_DIRECTORY, process.env, stdio);
		if (appFingerprint(profile) !== fingerprint) throw new SourcesChanged("native sources changed before publishing");
		const manifest = { schema: 1, platform: process.platform, arch: process.arch, profile, executable, sha256, buildFingerprint: fingerprint, files: hashes };
		const temporary = `${APP_MANIFEST}.${process.pid}.tmp`;
		fs.writeFileSync(temporary, JSON.stringify(manifest, null, "\t") + "\n");
		fs.renameSync(temporary, APP_MANIFEST);
		log(`Built native app: ${installed}`);
		return installed;
	}
}

// The browser module (native/src/web.rs, `--wasm`): the same crate for wasm32-unknown-emscripten with
// feature "web" at the `web` profile, built with the Emscripten SDK (EMSDK, else an `emsdk` directory
// beside this checkout or one of its parents; `rustup target add wasm32-unknown-emscripten`). Writes
// <state dir>/native-app/web/{rur-web.js, rur_web.wasm, manifest.json}; tools/web/bundle.mjs packs a
// story with it.
export const WEB_DIRECTORY = path.join(APP_DIRECTORY, "web");

export function findEmsdk() {
	const candidates = [];
	if (process.env.EMSDK) candidates.push(process.env.EMSDK);
	for (let dir = CRATE_DIRECTORY; ; dir = path.dirname(dir)) {
		candidates.push(path.join(dir, "emsdk"));
		if (path.dirname(dir) === dir) break;
	}
	return candidates.find(dir => fs.existsSync(path.join(dir, "upstream", "emscripten"))) || null;
}

function emsdkEnvironment(emsdk) {
	const windows = process.platform === "win32";
	const exe = name => (windows ? `${name}.exe` : name);
	const env = { ...withoutCookie(), EMSDK: emsdk, EM_CONFIG: path.join(emsdk, ".emscripten") };
	const pythonRoot = path.join(emsdk, "python");
	if (fs.existsSync(pythonRoot)) {
		const version = fs.readdirSync(pythonRoot).find(name => fs.existsSync(path.join(pythonRoot, name, exe("python"))));
		if (version) env.EMSDK_PYTHON = path.join(pythonRoot, version, exe("python"));
	}
	const pathKey = Object.keys(env).find(key => key.toLowerCase() === "path") || "PATH";
	env[pathKey] = [emsdk, path.join(emsdk, "upstream", "emscripten"), env[pathKey]].join(path.delimiter);
	// Emscripten 5+ ships .exe launchers on Windows (cc-rs and rustc default to emcc.bat); em++ links
	// Luau's C++ (libc++ / libc++abi with exceptions)
	Object.assign(env, {
		CC_wasm32_unknown_emscripten: exe("emcc"),
		CXX_wasm32_unknown_emscripten: exe("em++"),
		AR_wasm32_unknown_emscripten: exe("emar"),
		CARGO_TARGET_WASM32_UNKNOWN_EMSCRIPTEN_LINKER: exe("em++"),
		CFLAGS_wasm32_unknown_emscripten: webCFlags("CFLAGS"),
		CXXFLAGS_wasm32_unknown_emscripten: webCFlags("CXXFLAGS"),
	});
	return env;
}

// What every C/C++ object of the module is compiled with, through cc-rs in each build script: Rust's
// WebAssembly exceptions (native/build.rs links with them). Luau's own -fexceptions alone would pick
// Emscripten's JavaScript exceptions, which do not mix with them; FreeType's setjmp follows the same
// model (WebAssembly setjmp/longjmp).
const WEB_C_FLAGS = "-fwasm-exceptions";
const webCFlags = name => [process.env[`${name}_wasm32_unknown_emscripten`], WEB_C_FLAGS].filter(Boolean).join(" ");

// Cargo does not rebuild C objects when these flags change: mlua-sys's build script (Luau) reruns
// only when its own files change. A target directory built with other flags (the JavaScript-exception
// Luau of earlier builds) is removed once, so no object of another model is linked.
function dropObjectsOfOtherFlags(env, log) {
	const directory = path.join(TARGET_DIRECTORY, "wasm32-unknown-emscripten");
	const marker = path.join(directory, "c-flags.json");
	const flags = JSON.stringify({ CFLAGS: env.CFLAGS_wasm32_unknown_emscripten, CXXFLAGS: env.CXXFLAGS_wasm32_unknown_emscripten });
	let built = null;
	try {
		built = fs.readFileSync(marker, "utf8").trim();
	} catch {
		// no marker: a target directory from before it, or none
	}
	if (built === flags) return;
	if (fs.existsSync(directory)) {
		log(`rur: the web build's C/C++ flags changed (${flags}); rebuilding its objects`);
		fs.rmSync(directory, { recursive: true, force: true });
	}
	fs.mkdirSync(directory, { recursive: true });
	fs.writeFileSync(marker, flags + "\n");
}

export async function buildWebModule({ stdio = "inherit" } = {}) {
	const log = stdio === "inherit" ? console.log : console.error;
	const emsdk = findEmsdk();
	if (!emsdk) throw new Error("The web build needs the Emscripten SDK: set EMSDK (https://emscripten.org/docs/getting_started/downloads.html)");
	fs.mkdirSync(WEB_DIRECTORY, { recursive: true });
	const releaseLock = await waitForBuildLock(path.join(APP_DIRECTORY, "build.lock"), log);
	try {
		const hashes = appSourceHashes();
		const fingerprint = appFingerprint("web", hashes);
		await prepareNativeSource(SOURCE_DIRECTORY, { stdio });
		const env = { ...emsdkEnvironment(emsdk), CARGO_TARGET_DIR: TARGET_DIRECTORY, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS || String(os.availableParallelism()), RUR_FREETYPE_SRC: freetypeSource() };
		dropObjectsOfOtherFlags(env, log);
		runNativeCommand("cargo", ["build", ...locked(), "--target", "wasm32-unknown-emscripten", "--no-default-features", "--features", "web", "--profile", "web", "--bin", "rur-web"], CRATE_DIRECTORY, env, stdio);
		const built = path.join(TARGET_DIRECTORY, "wasm32-unknown-emscripten", "web");
		const files = {};
		for (const name of ["rur-web.js", "rur_web.wasm"]) {
			const bytes = fs.readFileSync(path.join(built, name));
			fs.writeFileSync(path.join(WEB_DIRECTORY, name), bytes);
			files[name] = { bytes: bytes.length, sha256: digest(bytes) };
		}
		const manifest = { schema: 1, profile: "web", buildFingerprint: fingerprint, emscripten: fs.readFileSync(path.join(emsdk, "upstream", "emscripten", "emscripten-version.txt"), "utf8").trim().replace(/"/g, ""), files, sources: hashes };
		fs.writeFileSync(path.join(WEB_DIRECTORY, "manifest.json"), JSON.stringify(manifest, null, "\t") + "\n");
		log(`Built web module: ${WEB_DIRECTORY} (rur_web.wasm ${(files["rur_web.wasm"].bytes / 1048576).toFixed(1)} MB)`);
		return WEB_DIRECTORY;
	} finally {
		await releaseLock();
	}
}

// The web module's state: "current" (built from these sources), "stale" or "missing"
export function webModuleStatus() {
	try {
		const manifest = JSON.parse(fs.readFileSync(path.join(WEB_DIRECTORY, "manifest.json"), "utf8"));
		const present = Object.entries(manifest.files).every(([name, file]) => fs.existsSync(path.join(WEB_DIRECTORY, name)) && fs.statSync(path.join(WEB_DIRECTORY, name)).size === file.bytes);
		if (!present) return { state: "missing", directory: WEB_DIRECTORY };
		return { state: manifest.buildFingerprint === appFingerprint("web") ? "current" : "stale", directory: WEB_DIRECTORY, manifest };
	} catch {
		return { state: "missing", directory: WEB_DIRECTORY };
	}
}

if (isMain(import.meta.url)) {
	const args = process.argv.slice(2);
	if (args.some(arg => !["--debug", "--prepare", "--check", "--wasm"].includes(arg))) throw new Error("Usage: node tools/native-app/build.mjs [--debug] [--prepare | --check | --wasm]");
	if (args.includes("--prepare") && args.includes("--check")) throw new Error("--prepare and --check are mutually exclusive");
	if (args.includes("--wasm")) await buildWebModule();
	else await buildNativeApp({ debug: args.includes("--debug"), prepareOnly: args.includes("--prepare"), verifyOnly: args.includes("--check") });
}
