// The release version, read from every manifest. Releases are built only when they all agree, so
// this runs first in the workflow, before any platform builds.
//
//   node tools/release/version.mjs [--tag v1.2.3]    prints the version
//
// With --tag it also fails unless the tag is exactly `v<version>`.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

const readText = (root, file) => fs.readFileSync(path.join(root, file), "utf8");

export function version(root = ROOT, tag) {
	const packageVersion = JSON.parse(readText(root, "package.json")).version;
	const lock = JSON.parse(readText(root, "package-lock.json"));
	const cargoVersion = readText(root, "native/Cargo.toml").match(/^version\s*=\s*"([^"]+)"/m)?.[1];
	// The lockfile entry for the binary's own crate (\r? for a checkout with Windows line endings)
	const cargoLockVersion = readText(root, "native/Cargo.lock").match(/name = "rur-native"\r?\nversion = "([^"]+)"/)?.[1];

	const others = [cargoVersion, cargoLockVersion, lock.version, lock.packages[""].version];
	if (!/^\d+\.\d+\.\d+$/.test(packageVersion) || others.some(other => other !== packageVersion)) {
		throw new Error("package.json, package-lock.json, native/Cargo.toml and native/Cargo.lock must all hold the same x.y.z version");
	}
	if (tag !== undefined && tag !== `v${packageVersion}`) {
		throw new Error(`the tag must be v${packageVersion}, not ${tag}`);
	}
	return packageVersion;
}

// Reads `--name value` pairs from the command line into { name: value }
export function options(argv) {
	const result = {};
	for (let i = 0; i < argv.length; i += 2) {
		if (!argv[i].startsWith("--") || argv[i + 1] === undefined) throw new Error("expected pairs of --option value");
		result[argv[i].slice(2)] = argv[i + 1];
	}
	return result;
}

// True when the module was started from the command line, not imported by another script or a test.
// Both paths are compared as real paths: started through a linked folder (a symlink, or a junction
// on Windows), the command line names the link while import.meta.url names the real file, and a
// plain comparison would see two different files and run nothing.
export function isMain(moduleUrl) {
	if (!process.argv[1]) return false;
	const entry = path.resolve(process.argv[1]);
	const module = fileURLToPath(moduleUrl);
	try {
		return fs.realpathSync(entry) === fs.realpathSync(module);
	} catch {
		// A path that cannot be resolved (it does not exist) can only match as written
		return entry === module;
	}
}

if (isMain(import.meta.url)) console.log(version(ROOT, options(process.argv.slice(2)).tag));
