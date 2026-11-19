// Tests of build-cache.mjs, the key of a release build's dependency cache
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { buildCacheKey, lockWithoutOwnVersions, manifestWithoutVersion } from "./build-cache.mjs";

const SCRIPT = fileURLToPath(new URL("./build-cache.mjs", import.meta.url));
const FILES = ["native/Cargo.toml", "native/Cargo.lock", "native/.cargo/config.toml"];

const manifest = (version, extra = "") => `[package]\nname = "tool"\nversion = "${version}"\nedition = "2024"\n\n[dependencies]\nserde = { version = "1.0.200" }\nengine = { git = "https://example.invalid/engine", rev = "abc" }\n${extra}\n[profile.dist]\ninherits = "release"\nlto = "thin"\n`;
const lock = (version, engineRev = "abc", serde = "1.0.200") => [
	"version = 4",
	"",
	"[[package]]",
	'name = "engine"',
	'version = "0.5.0"',
	`source = "git+https://example.invalid/engine?rev=${engineRev}#${engineRev}"`,
	"",
	"[[package]]",
	'name = "serde"',
	`version = "${serde}"`,
	'source = "registry+https://github.com/rust-lang/crates.io-index"',
	'checksum = "00"',
	"",
	"[[package]]",
	'name = "tool"',
	`version = "${version}"`,
	"dependencies = [",
	' "engine",',
	' "serde",',
	"]",
	"",
].join("\n");
const tree = (map) => (file) => (file in map ? map[file] : null);
const key = (map) => buildCacheKey(FILES, tree(map));
const base = { "native/Cargo.toml": manifest("0.7.0"), "native/Cargo.lock": lock("0.7.0"), "native/.cargo/config.toml": "[net]\ngit-fetch-with-cli = true\n" };

test("a version bump keeps the build cache key", () => {
	const bumped = { ...base, "native/Cargo.toml": manifest("0.8.0"), "native/Cargo.lock": lock("0.8.0") };
	assert.equal(key(bumped), key(base));
	assert.match(key(base), /^[0-9a-f]{16}$/);
	// Windows line endings in a checkout count the same
	const crlf = Object.fromEntries(Object.entries(base).map(([file, text]) => [file, text.replace(/\n/g, "\r\n")]));
	assert.equal(key(crlf), key(base));
});

test("a changed dependency, pin, feature, profile or config changes the key", () => {
	const variants = [
		{ "native/Cargo.lock": lock("0.7.0", "def") },
		{ "native/Cargo.lock": lock("0.7.0", "abc", "1.0.201") },
		{ "native/Cargo.toml": manifest("0.7.0", '[features]\ndefault = ["x"]\n') },
		{ "native/Cargo.toml": manifest("0.7.0").replace('lto = "thin"', 'lto = "fat"') },
		{ "native/Cargo.toml": manifest("0.7.0").replace('version = "1.0.200"', 'version = "1.0.201"') },
		{ "native/.cargo/config.toml": "[net]\ngit-fetch-with-cli = false\n" },
		{ "native/.cargo/config.toml": null },
	];
	const keys = new Set([key(base), ...variants.map((change) => key({ ...base, ...change }))]);
	assert.equal(keys.size, variants.length + 1);
});

test("only the workspace's own versions are left out", () => {
	// A dependency's version under [package]'s neighbours stays; the workspace's own version goes
	const workspace = '[workspace.package]\nversion = "1.2.3"\n\n[workspace.dependencies]\nserde = { version = "1.0.0" }\n[dependencies.extra]\nversion = "2.0.0"\n';
	assert.equal(manifestWithoutVersion(workspace), workspace.replace('version = "1.2.3"', 'version = "0.0.0"'));
	assert.equal(manifestWithoutVersion('[package]\nversion.workspace = true\n'), '[package]\nversion.workspace = true\n');
	const stripped = lockWithoutOwnVersions(lock("0.7.0"));
	assert.match(stripped, /name = "tool"\nversion = "0\.0\.0"/);
	assert.match(stripped, /name = "serde"\nversion = "1\.0\.200"/);
	assert.match(stripped, /name = "engine"\nversion = "0\.5\.0"/);
});

test("pruning leaves only the dependencies' outputs", () => {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "build-cache-test-"));
	try {
		const host = path.join(dir, "target/dist");
		const target = path.join(dir, "target/x86_64-pc-windows-msvc/dist");
		const own = [
			"target/dist/build/rur-native-0a1b/build-script-build.exe",
			"target/dist/.fingerprint/rur-native-0a1b/build-script",
			"target/x86_64-pc-windows-msvc/dist/rur-native.exe",
			"target/x86_64-pc-windows-msvc/dist/rur-native.d",
			"target/x86_64-pc-windows-msvc/dist/workspace.txt",
			"target/x86_64-pc-windows-msvc/dist/incremental/x/y",
			"target/x86_64-pc-windows-msvc/dist/deps/rur_native-1111.exe",
			"target/x86_64-pc-windows-msvc/dist/deps/rur_native-1111.d",
			"target/x86_64-pc-windows-msvc/dist/deps/librur_native-2222.rlib",
			"target/x86_64-pc-windows-msvc/dist/deps/standalone-3333.exe",
			"target/x86_64-pc-windows-msvc/dist/build/rur-native-4444/out/ft.o",
			"target/x86_64-pc-windows-msvc/dist/.fingerprint/rur-native-5555/lib-rur_native",
		];
		const kept = [
			"target/dist/deps/serde_derive-aaaa.dll",
			"target/dist/build/mlua-sys-bbbb/build-script-build.exe",
			"target/dist/.fingerprint/mlua-sys-bbbb/build-script",
			"target/x86_64-pc-windows-msvc/dist/deps/libserde-cccc.rlib",
			"target/x86_64-pc-windows-msvc/dist/deps/librur_nativeish-dddd.rlib",
			"target/x86_64-pc-windows-msvc/dist/build/mlua-sys-eeee/out/lvm.o",
			"target/x86_64-pc-windows-msvc/dist/.fingerprint/serde-ffff/lib-serde",
		];
		for (const file of [...own, ...kept]) {
			fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
			fs.writeFileSync(path.join(dir, file), "x");
		}
		const result = spawnSync(process.execPath, [SCRIPT, "--prune", host, target, path.join(dir, "missing"), "--names", "rur-native,standalone"], { encoding: "utf8" });
		assert.equal(result.status, 0, result.stderr);
		for (const file of own) assert.ok(!fs.existsSync(path.join(dir, file)), `${file} was left`);
		for (const file of kept) assert.ok(fs.existsSync(path.join(dir, file)), `${file} was deleted`);
		assert.equal(spawnSync(process.execPath, [SCRIPT, "--prune", host], { encoding: "utf8" }).status, 2);
		assert.equal(spawnSync(process.execPath, [SCRIPT, "--prune", "--names", "x"], { encoding: "utf8" }).status, 2);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
});

test("the script prints the key of the files it is given", () => {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "build-cache-test-"));
	try {
		for (const [file, text] of Object.entries(base)) {
			fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
			fs.writeFileSync(path.join(dir, file), text);
		}
		const result = spawnSync(process.execPath, [SCRIPT, ...FILES], { cwd: dir, encoding: "utf8" });
		assert.equal(result.status, 0, result.stderr);
		assert.equal(result.stdout.trim(), key(base));
		assert.equal(spawnSync(process.execPath, [SCRIPT], { cwd: dir, encoding: "utf8" }).status, 2);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
});
