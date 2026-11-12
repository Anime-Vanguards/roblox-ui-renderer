// Where rur keeps what it writes, so a checkout can be used without writing into it.
//
//   state dir   RUR_STATE_DIR (alias: RUR_CACHE_DIR), or `--state-dir <dir>` on any cli/ or tools/ entry
//               script; default <renderer>/.cache. Asset and font caches, hinted metrics, the native
//               app build (native-app/), Studio snapshots, serve.log. (The Studio lock is machine-wide:
//               cli/lock.mjs.)
//   out dir     RUR_OUT_DIR, default <renderer>/out: where commands write when no --out is given.
//   isolated    RUR_ISOLATED=1, or `--isolated` on any entry script: never start or contact a render
//               service (cli/client.mjs) and run the engine this state dir's native-app manifest names
//               even when the sources changed since (cli/engine.mjs). `--no-service` / RUR_NO_SERVICE=1
//               is the service part alone.
//
// Built things a fresh state dir does not have yet (Studio snapshots, the native app) are read from the
// checkout's own .cache when present (sharedStatePath); nothing is written there.
// The native engine (native/src/lib.rs state_dir) follows the same variables.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const DEFAULT_STATE_DIR = path.join(ROOT, ".cache");
export const DEFAULT_OUT_DIR = path.join(ROOT, "out");

// `--state-dir <dir>` / `--state-dir=<dir>`: set before any module resolves a path (every module that
// does imports this one first), passed to child processes through the environment, and removed from
// process.argv so the command's own option parsing never sees it.
(function stateDirFlag() {
	const main = process.argv[1] ? path.resolve(process.argv[1]) : "";
	const inside = dir => main.toLowerCase().startsWith((path.join(ROOT, dir) + path.sep).toLowerCase());
	if (!inside("cli") && !inside("tools")) return;
	for (let i = 2; i < process.argv.length; i++) {
		const arg = process.argv[i];
		if (arg === "--") break;
		let value;
		if (arg === "--state-dir" && process.argv[i + 1] !== undefined) {
			value = process.argv[i + 1];
			process.argv.splice(i, 2);
		} else if (arg.startsWith("--state-dir=")) {
			value = arg.slice("--state-dir=".length);
			process.argv.splice(i, 1);
		} else continue;
		process.env.RUR_STATE_DIR = path.resolve(value);
		delete process.env.RUR_CACHE_DIR;
		break;
	}
	// `--isolated` / `--no-service`: flags of every command, passed on to child processes the same way
	for (let i = 2; i < process.argv.length; i++) {
		const arg = process.argv[i];
		if (arg === "--") break;
		if (arg === "--isolated") process.env.RUR_ISOLATED = "1";
		else if (arg === "--no-service") process.env.RUR_NO_SERVICE = "1";
		else continue;
		process.argv.splice(i--, 1);
	}
	// `--text-subpixel on|off` / `--text-subpixel=on`: the text mode (RUR_TEXT_SUBPIXEL, docs/text.md), the same way
	for (let i = 2; i < process.argv.length; i++) {
		const arg = process.argv[i];
		if (arg === "--") break;
		let value;
		if (arg === "--text-subpixel" && process.argv[i + 1] !== undefined) {
			value = process.argv[i + 1];
			process.argv.splice(i, 2);
		} else if (arg.startsWith("--text-subpixel=")) {
			value = arg.slice("--text-subpixel=".length);
			process.argv.splice(i, 1);
		} else continue;
		if (!["on", "off"].includes(value)) {
			console.error(`--text-subpixel takes on or off, not ${JSON.stringify(value)} (docs/text.md, Text modes)`);
			process.exit(2);
		}
		if (value === "on") process.env.RUR_TEXT_SUBPIXEL = "on";
		else delete process.env.RUR_TEXT_SUBPIXEL;
		break;
	}
})();

// The text mode (docs/text.md, Text modes): quarter-pixel pens when RUR_TEXT_SUBPIXEL / --text-subpixel is on,
// whole-pixel advances otherwise (the engine reads the same variable: native/src/engine/text/mod.rs)
export const textSubpixel = (env = process.env) => ["on", "enabled", "true", "1"].includes(String(env.RUR_TEXT_SUBPIXEL ?? "").trim().toLowerCase());

// RUR_ISOLATED=1 (--isolated): no render service, and the state dir's engine build as it is
export const isolated = (env = process.env) => env.RUR_ISOLATED === "1" || env.RUR_ISOLATED === "true";
// The render service is not used: --isolated, or --no-service / RUR_NO_SERVICE=1 alone
export const serviceDisabled = (env = process.env) => isolated(env) || isolated(process.env)
	|| env.RUR_NO_SERVICE === "1" || process.env.RUR_NO_SERVICE === "1";

// Read at call time (tests set RUR_CACHE_DIR before importing the modules that use it)
export function stateDir(env = process.env) {
	return path.resolve(env.RUR_STATE_DIR || env.RUR_CACHE_DIR || DEFAULT_STATE_DIR);
}

export function outDir(env = process.env) {
	return path.resolve(env.RUR_OUT_DIR || DEFAULT_OUT_DIR);
}

export const STATE_DIR = stateDir();
export const OUT_DIR = outDir();

const same = (a, b) => process.platform === "win32" ? a.toLowerCase() === b.toLowerCase() : a === b;
export const isDefaultStateDir = (dir = stateDir()) => same(path.resolve(dir), DEFAULT_STATE_DIR);

// `relative` inside the state dir, else the checkout's .cache copy when only that one exists
// (read-only use: prebuilt FreeType, Studio snapshots)
export function sharedStatePath(relative, dir = stateDir()) {
	const own = path.join(dir, relative);
	if (isDefaultStateDir(dir) || fs.existsSync(own)) return own;
	const fallback = path.join(DEFAULT_STATE_DIR, relative);
	return fs.existsSync(fallback) ? fallback : own;
}
