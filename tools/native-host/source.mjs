// The pinned FreeType source native/build.rs compiles into the crate (the glyph rasterizer). The Luau VM
// and Lune's datatypes come patched with the engine (roblox-engine-emulator's vendor/, through cargo).
import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

// Windows' own bsdtar: Git Bash puts GNU tar first on PATH, which reads "C:\..." as a remote host
const TAR = process.platform === "win32" && fs.existsSync(path.join(process.env.SystemRoot || "C:\\Windows", "System32", "tar.exe"))
	? path.join(process.env.SystemRoot || "C:\\Windows", "System32", "tar.exe") : "tar";

export const digest = bytes => createHash("sha256").update(bytes).digest("hex");

// `stdio`: "inherit", or e.g. ["ignore", 2, 2] to keep a host's stdout clean (cli/engine.mjs builds)
export function runNativeCommand(command, parameters, cwd, env = process.env, stdio = "inherit") {
	const result = spawnSync(command, parameters, { cwd, env, stdio, windowsHide: true });
	if (result.error || result.status !== 0) throw result.error || new Error(`${command} exited ${result.status}`);
}

async function archive(work, name, url, expected, log) {
	const target = path.join(work, name);
	if (!fs.existsSync(target) || digest(fs.readFileSync(target)) !== expected) {
		log(`Downloading ${name}`);
		const response = await fetch(url);
		if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
		const bytes = Buffer.from(await response.arrayBuffer());
		if (digest(bytes) !== expected) throw new Error(`${name}: source checksum mismatch`);
		fs.writeFileSync(target, bytes);
	}
	return target;
}

// A tree's file count, bytes and newest modification time (tar keeps the archive's times, so an edited,
// added or removed file changes it)
function treeStamp(dir) {
	let files = 0, bytes = 0, newest = 0;
	const walk = folder => {
		for (const entry of fs.readdirSync(folder, { withFileTypes: true })) {
			const full = path.join(folder, entry.name);
			if (entry.isDirectory()) walk(full);
			else if (entry.isFile()) {
				const stat = fs.statSync(full);
				files++;
				bytes += stat.size;
				newest = Math.max(newest, stat.mtimeMs);
			}
		}
	};
	try {
		walk(dir);
	} catch {
		return null;
	}
	return { files, bytes, newest };
}

export async function prepareNativeSource(work, { stdio = "inherit" } = {}) {
	const log = stdio === "inherit" ? console.log : console.error;
	work = path.resolve(work);
	fs.mkdirSync(work, { recursive: true });
	// FreeType 2.14.2 (tag VER-2-14-2, commit f4205da14867c5387cd6a329b90ee10a6df6eeff, the release Studio's
	// glyphs match; notes/text.md "FreeType"): native/build.rs compiles it into the crate (native/src/glyphs)
	const sha256 = "bfb1eabd7833ee0b559f450a9a9de33337e89b7f2d6a03a556c08a5fb9f0607e";
	const freetypeArchive = await archive(work, "freetype-2.14.2.tar.gz", "https://github.com/freetype/freetype/archive/refs/tags/VER-2-14-2.tar.gz", sha256, log);
	const freetype = path.join(work, "freetype-2.14.2");
	// Restore the source when it is not the archive's as extracted (a second per build otherwise); callers keep
	// build artifacts in separate target directories.
	const stampFile = path.join(work, "freetype-2.14.2.extracted.json");
	let stamp = null;
	try {
		stamp = JSON.parse(fs.readFileSync(stampFile, "utf8"));
	} catch {
		// never extracted with a stamp
	}
	const tree = treeStamp(freetype);
	if (!stamp || !tree || stamp.archive !== sha256 || stamp.files !== tree.files || stamp.bytes !== tree.bytes || stamp.newest !== tree.newest) {
		fs.mkdirSync(freetype, { recursive: true });
		runNativeCommand(TAR, ["-xzf", freetypeArchive, "--strip-components=1", "-C", freetype], work, process.env, stdio);
		fs.writeFileSync(stampFile, JSON.stringify({ archive: sha256, ...treeStamp(freetype) }));
	}
	return { work, freetype, freetypeArchive };
}
