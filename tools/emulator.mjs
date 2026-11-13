// The engine rur builds against: roblox-engine-emulator (https://github.com/Anime-Vanguards/roblox-engine-emulator),
// a cargo git dependency pinned to a commit in native/Cargo.toml (the crate and, under [patch.crates-io],
// its vendored Luau VM and mlua-sys: all at the same commit). docs/how-it-works.md "The engine: roblox-engine-emulator".
//
//   node tools/emulator.mjs status          the pinned commit, the link (if any), the checkout beside this one
//   node tools/emulator.mjs link [<dir>]    build against a local checkout (default ../roblox-engine-emulator):
//                                           edits there rebuild rur like edits here (co-development)
//   node tools/emulator.mjs unlink          back to the pinned commit (Cargo.lock restored to it)
//   node tools/emulator.mjs pin [<commit>]  pin another commit (default: the local checkout's HEAD, which must
//                                           be pushed); updates Cargo.toml and Cargo.lock
//
// A link is an untracked cargo config, <checkout>/.cargo/config.toml, patching the git dependency with
// the local path; plain cargo commands in native/ use it too. While linked, Cargo.lock names the
// emulator as a path package (cargo rewrites it): unlink or pin puts it back. The Luau VM stays the
// pinned commit's even when linked: a VM change (the emulator's vendor/) needs a pushed commit and a pin.
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { isMain } from "../cli/entry.mjs";

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const EMULATOR_URL = "https://github.com/Anime-Vanguards/roblox-engine-emulator";
const LINK_FILE = path.join(ROOT, ".cargo", "config.toml");
const MANIFEST = path.join(ROOT, "native", "Cargo.toml");
const LOCK = path.join(ROOT, "native", "Cargo.lock");
const SIBLING = path.resolve(ROOT, "..", "roblox-engine-emulator");

// The engine's git URL as native/Cargo.toml names it: EMULATOR_URL, or another URL a fork or
// mirror of the engine uses. Pins and links follow the manifest, so they keep working if it moves.
export function emulatorUrl() {
	const text = fs.readFileSync(MANIFEST, "utf8");
	return /roblox-engine-emulator\s*=\s*\{\s*git\s*=\s*"([^"]+)"/.exec(text)?.[1] ?? EMULATOR_URL;
}

// The linked checkout (absolute), or null when rur builds the pinned commit
export function emulatorLink() {
	let text;
	try {
		text = fs.readFileSync(LINK_FILE, "utf8");
	} catch {
		return null;
	}
	const match = /roblox-engine-emulator\s*=\s*\{\s*path\s*=\s*"([^"]+)"/.exec(text);
	return match ? path.resolve(ROOT, match[1]) : null;
}

// The files of a linked checkout a build depends on (repo-relative, forward slashes): the crate's
// sources, data and manifest, and the vendored Lune crates it compiles from its own tree
export function emulatorSourceFiles(dir = emulatorLink()) {
	if (!dir) return [];
	const list = [];
	const walk = (relative) => {
		const full = path.join(dir, relative);
		const stat = fs.statSync(full, { throwIfNoEntry: false });
		if (!stat) return;
		if (stat.isDirectory()) {
			for (const name of fs.readdirSync(full).sort()) walk(path.join(relative, name));
		} else {
			list.push(relative.replaceAll("\\", "/"));
		}
	};
	for (const entry of ["Cargo.toml", "src", "data", "vendor/lune-roblox", "vendor/lune-utils"]) walk(entry);
	return list;
}

// The commit native/Cargo.toml pins (both entries must agree)
export function pinnedCommit() {
	const text = fs.readFileSync(MANIFEST, "utf8");
	const pin = new RegExp(`git = "${emulatorUrl().replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}", rev = "([0-9a-f]+)"`, "g");
	const revs = [...text.matchAll(pin)].map(match => match[1]);
	if (!revs.length) throw new Error("native/Cargo.toml pins no roblox-engine-emulator commit");
	if (new Set(revs).size !== 1) throw new Error(`native/Cargo.toml pins two emulator commits: ${[...new Set(revs)].join(", ")}`);
	return revs[0];
}

function git(dir, ...args) {
	const result = spawnSync("git", ["-C", dir, ...args], { encoding: "utf8" });
	return result.status === 0 ? result.stdout.trim() : null;
}

function cargo(...args) {
	const result = spawnSync("cargo", args, { cwd: path.join(ROOT, "native"), encoding: "utf8", stdio: ["ignore", "ignore", "pipe"], env: { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS || "8" } });
	if (result.status !== 0) throw new Error(`cargo ${args.join(" ")} failed:\n${result.stderr.slice(-2000)}`);
}

// Cargo.lock re-resolved for the current manifest and link (fetches the pinned commit when needed)
function settleLock() {
	cargo("metadata", "--format-version", "1");
	const changed = git(ROOT, "diff", "--stat", "--", "native/Cargo.lock");
	return changed ? `Cargo.lock: ${changed.split("\n").pop().trim()}` : "Cargo.lock unchanged";
}

function describeCheckout(dir) {
	const head = git(dir, "log", "-1", "--format=%h %cs %s");
	if (!head) return `${dir}: not a git checkout`;
	const dirty = (git(dir, "status", "--porcelain") || "").split("\n").filter(Boolean).length;
	return `${dir} at ${head}${dirty ? ` (${dirty} changed files)` : ""}`;
}

function status() {
	const pinned = pinnedCommit();
	console.log(`pinned   ${pinned.slice(0, 12)} (native/Cargo.toml)`);
	const link = emulatorLink();
	console.log(`link     ${link ? `${describeCheckout(link)}: builds use this checkout` : "none: builds use the pinned commit"}`);
	if (fs.existsSync(SIBLING)) {
		const head = git(SIBLING, "rev-parse", "HEAD");
		console.log(`checkout ${describeCheckout(SIBLING)}${head === pinned ? " (the pinned commit)" : ""}`);
	}
}

function link(dir) {
	const target = path.resolve(dir || SIBLING);
	if (!fs.existsSync(path.join(target, "Cargo.toml")) || !/name = "roblox-engine-emulator"/.test(fs.readFileSync(path.join(target, "Cargo.toml"), "utf8"))) {
		throw new Error(`${target} is not a roblox-engine-emulator checkout`);
	}
	fs.mkdirSync(path.dirname(LINK_FILE), { recursive: true });
	// (the profile viewer, a crate of the same repository, from the same checkout when it has one)
	const viewer = path.join(target, "profile-view");
	fs.writeFileSync(LINK_FILE, [
		"# Written by `node tools/emulator.mjs link`: rur builds against this roblox-engine-emulator checkout",
		"# instead of the commit native/Cargo.toml pins (`node tools/emulator.mjs unlink` to go back)",
		`[patch."${emulatorUrl()}"]`,
		`roblox-engine-emulator = { path = "${target.replaceAll("\\", "/")}" }`,
		...(fs.existsSync(path.join(viewer, "Cargo.toml")) ? [`ree-profile-view = { path = "${viewer.replaceAll("\\", "/")}" }`] : []),
		"",
	].join("\n"));
	console.log(`linked ${describeCheckout(target)}; ${settleLock()}`);
}

function unlink() {
	if (fs.existsSync(LINK_FILE)) fs.rmSync(LINK_FILE);
	if (fs.existsSync(path.dirname(LINK_FILE)) && !fs.readdirSync(path.dirname(LINK_FILE)).length) fs.rmdirSync(path.dirname(LINK_FILE));
	console.log(`unlinked: builds use ${pinnedCommit().slice(0, 12)}; ${settleLock()}`);
}

function pin(commit) {
	let rev = commit;
	if (!rev) {
		const source = emulatorLink() || SIBLING;
		rev = git(source, "rev-parse", "HEAD");
		if (!rev) throw new Error(`no commit given and no checkout at ${source}`);
		git(source, "fetch", "-q", "origin");
		if (!(git(source, "branch", "-r", "--contains", rev) || "").trim()) throw new Error(`${rev.slice(0, 12)} is not on ${EMULATOR_URL} yet: push it first`);
	} else if (!/^[0-9a-f]{40}$/.test(rev)) {
		const full = git(fs.existsSync(SIBLING) ? SIBLING : ROOT, "rev-parse", rev);
		if (!full) throw new Error(`pin takes a full commit hash (or one the checkout beside this one knows): ${rev}`);
		rev = full;
	}
	const before = pinnedCommit();
	const text = fs.readFileSync(MANIFEST, "utf8").replaceAll(`rev = "${before}"`, `rev = "${rev}"`);
	fs.writeFileSync(MANIFEST, text);
	console.log(`pinned ${rev.slice(0, 12)} (was ${before.slice(0, 12)}); ${settleLock()}`);
}

if (isMain(import.meta.url)) {
	const [command, argument] = process.argv.slice(2);
	try {
		if (!command || command === "status") status();
		else if (command === "link") link(argument);
		else if (command === "unlink") unlink();
		else if (command === "pin") pin(argument);
		else throw new Error("usage: node tools/emulator.mjs status | link [<dir>] | unlink | pin [<commit>]");
	} catch (error) {
		console.error(`emulator: ${error.message}`);
		process.exit(1);
	}
}
