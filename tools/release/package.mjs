// Packs one platform's release zip and its SOURCE.md.
//
//   node tools/release/package.mjs --binary <file> --os windows|linux|macos --arch x86_64|aarch64
//     --licenses <THIRD_PARTY_LICENSES.txt> --out <folder> [--commit <sha>] [--tag <tag>]
//
// The zip is written here, not by a zip tool, so it is the same on every runner: flat, entries in a
// fixed order, the executable first with Unix mode 0755 (also when built on Windows). Installers
// pick the executable from the zip (foreman takes the first entry, aftman the first with the
// platform's suffix, rokit scores every entry), so the binary must be the only executable and
// the first entry.
import fs from "node:fs";
import path from "node:path";
import { deflateRawSync, inflateRawSync } from "node:zlib";
import { execFileSync } from "node:child_process";
import { ROOT, isMain, options, version } from "./version.mjs";

// The zip format's record signatures
const LOCAL_FILE_HEADER = 0x04034b50;
const CENTRAL_DIRECTORY_ENTRY = 0x02014b50;
const END_OF_CENTRAL_DIRECTORY = 0x06054b50;
const UTF8_NAMES = 0x800;
const DEFLATE = 8;
const DOS_DATE_1980_01_01 = 0x21; // a fixed date, so the zip does not depend on the build time
const MADE_BY_UNIX = 0x314; // "Unix, zip version 2.0": lets readers find the mode in the attributes

export function crc32(bytes) {
	let crc = 0xffffffff;
	for (const byte of bytes) {
		crc ^= byte;
		for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
	}
	return (crc ^ 0xffffffff) >>> 0;
}

// entries: [{ name, bytes, mode }]. Writes each entry deflated, then the central directory.
export function zip(entries) {
	const localParts = [], centralParts = [];
	let offset = 0;
	for (const { name, bytes, mode = 0o644 } of entries) {
		if (!name || /[\\/]/.test(name)) throw new Error("zip entries must be flat: no folders");
		const filename = Buffer.from(name);
		const compressed = deflateRawSync(bytes);

		const header = Buffer.alloc(30);
		header.writeUInt32LE(LOCAL_FILE_HEADER);
		header.writeUInt16LE(20, 4); // version needed to extract
		header.writeUInt16LE(UTF8_NAMES, 6);
		header.writeUInt16LE(DEFLATE, 8);
		header.writeUInt16LE(DOS_DATE_1980_01_01, 12);
		header.writeUInt32LE(crc32(bytes), 14);
		header.writeUInt32LE(compressed.length, 18);
		header.writeUInt32LE(bytes.length, 22);
		header.writeUInt16LE(filename.length, 26);
		localParts.push(header, filename, compressed);

		// The directory entry repeats the header's fields (from "version needed" on) and adds the file mode
		const entry = Buffer.alloc(46);
		entry.writeUInt32LE(CENTRAL_DIRECTORY_ENTRY);
		entry.writeUInt16LE(MADE_BY_UNIX, 4);
		header.copy(entry, 6, 4, 30);
		entry.writeUInt32LE(((0o100000 | mode) << 16) >>> 0, 38); // regular file, Unix mode in the high half
		entry.writeUInt32LE(offset, 42);
		centralParts.push(entry, filename);

		offset += header.length + filename.length + compressed.length;
	}

	const directory = Buffer.concat(centralParts);
	const end = Buffer.alloc(22);
	end.writeUInt32LE(END_OF_CENTRAL_DIRECTORY);
	end.writeUInt16LE(entries.length, 8);
	end.writeUInt16LE(entries.length, 10);
	end.writeUInt32LE(directory.length, 12);
	end.writeUInt32LE(offset, 16);
	return Buffer.concat([...localParts, directory, end]);
}

// Reads back a zip written by `zip`: [{ name, mode, bytes, offset }] in archive order, with every
// entry inflated and its CRC checked. Only reads the plain layout `zip` writes (no zip comment).
export function inspect(bytes) {
	const end = bytes.length - 22;
	if (bytes.readUInt32LE(end) !== END_OF_CENTRAL_DIRECTORY) throw new Error("the zip has no central directory");
	const count = bytes.readUInt16LE(end + 10);
	let at = bytes.readUInt32LE(end + 16);

	const entries = [];
	for (let i = 0; i < count; i++) {
		if (bytes.readUInt32LE(at) !== CENTRAL_DIRECTORY_ENTRY) throw new Error("the zip has a damaged directory entry");
		const nameLength = bytes.readUInt16LE(at + 28);
		const extraLength = bytes.readUInt16LE(at + 30);
		const commentLength = bytes.readUInt16LE(at + 32);
		const compressedSize = bytes.readUInt32LE(at + 20);
		const offset = bytes.readUInt32LE(at + 42);
		const name = bytes.subarray(at + 46, at + 46 + nameLength).toString();

		const dataStart = offset + 30 + bytes.readUInt16LE(offset + 26) + bytes.readUInt16LE(offset + 28);
		const data = inflateRawSync(bytes.subarray(dataStart, dataStart + compressedSize));
		if (crc32(data) !== bytes.readUInt32LE(at + 16)) throw new Error(`the zip entry ${name} fails its CRC check`);

		entries.push({ name, mode: bytes.readUInt32LE(at + 38) >>> 16, bytes: data, offset });
		at += 46 + nameLength + extraLength + commentLength;
	}
	return entries;
}

// "<url> at <commit>" for each git dependency pinned in Cargo.lock
function gitDependencies(root) {
	const lock = fs.readFileSync(path.join(root, "native/Cargo.lock"), "utf8");
	const pins = [...lock.matchAll(/source = "git\+([^"#]+)#([a-f0-9]+)"/g)].map(match => {
		const url = match[1].replace(/\?.*/, ""); // without the ?rev= part
		return `${url} at ${match[2]}`;
	});
	return [...new Set(pins)];
}

// SOURCE.md: where the Corresponding Source of this binary is, for the GPL
export function source(root, tag, commit) {
	if (!/^v\d+\.\d+\.\d+$/.test(tag) || !/^[a-f0-9]{40}$/.test(commit)) throw new Error("the source tag must look like v1.2.3 and the commit must be a full SHA-1");
	const git = gitDependencies(root);
	return `# Corresponding Source

This rur-native binary is distributed under GPL-3.0-or-later.

Source: https://github.com/Anime-Vanguards/roblox-ui-renderer/tree/${tag}
Commit: ${commit}
GitHub source archive: https://github.com/Anime-Vanguards/roblox-ui-renderer/archive/refs/tags/${tag}.tar.gz
For a dry run, the commit above identifies the source; the version tag may not exist yet.

Rust crates are pinned by native/Cargo.lock. Crates.io packages include their SHA-256 checksums there.
Git dependencies (including their vendored sources):
${git.map(value => "- " + value).join("\n")}

FreeType 2.14.2: the exact SHA-256-checked freetype-2.14.2.tar.gz used for the build is attached to this release.

Rebuild with Rust 1.90.0 and a C++ toolchain: npm ci; node tools/native-app/build.mjs --prepare; then, in native/, cargo build --profile dist --locked --bin rur-native --target <triple>.
Set RUR_RELEASE_BUILD=1 for that cargo build, which makes the binary use standalone defaults, and set RUR_FREETYPE_SRC to the prepared freetype-2.14.2 directory. The public default features are used.
Target triples: x86_64-pc-windows-msvc, x86_64-unknown-linux-gnu, aarch64-apple-darwin. On Windows set RUSTFLAGS=-C target-feature=+crt-static before building, to include the C runtime. See .github/workflows/release.yml at the source tag for system build packages.
`;
}

// The files that ship next to the executable, as [name in the zip, file in the repository]
const SHIPPED_FILES = [
	["LICENSE", "LICENSE"],
	["LICENSE-GPL-3.0.txt", "licenses/GPL-3.0.txt"],
	["NOTICE", "NOTICE"],
	["THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md"],
];

export function packageRelease({ root = ROOT, binary, os, arch, out, licenses, tag, commit }) {
	const releaseVersion = version(root);
	if (!["windows", "linux", "macos"].includes(os) || !["x86_64", "aarch64"].includes(arch)) throw new Error(`unknown platform ${os} ${arch}`);

	const executable = "rur-native" + (os === "windows" ? ".exe" : "");
	const sourceNotes = Buffer.from(source(root, tag || `v${releaseVersion}`, commit));
	const entries = [{ name: executable, bytes: fs.readFileSync(binary), mode: 0o755 }];
	for (const [name, file] of SHIPPED_FILES) entries.push({ name, bytes: fs.readFileSync(path.join(root, file)) });
	entries.push({ name: "THIRD_PARTY_LICENSES.txt", bytes: fs.readFileSync(licenses) });
	entries.push({ name: "SOURCE.md", bytes: sourceNotes });

	// SOURCE.md is also kept beside the zip
	fs.mkdirSync(out, { recursive: true });
	fs.writeFileSync(path.join(out, "SOURCE.md"), sourceNotes);

	const bytes = zip(entries);
	checkInstallerContract(inspect(bytes), executable);
	const file = path.join(out, `rur-native-${releaseVersion}-${os}-${arch}.zip`);
	fs.writeFileSync(file, bytes);
	return file;
}

// Installers need the binary first (at offset 0, mode 0755) and nothing else that looks executable
function checkInstallerContract(entries, executable) {
	const [first, ...others] = entries;
	const binaryFirst = first.name === executable && first.offset === 0 && (first.mode & 0o777) === 0o755;
	const otherExecutable = others.some(entry => (entry.mode & 0o111) || entry.name.endsWith(".exe"));
	if (!binaryFirst || otherExecutable) throw new Error("the zip must start with the executable (mode 0755) and hold no other executable file");
}

if (isMain(import.meta.url)) {
	const args = options(process.argv.slice(2));
	const commit = args.commit || execFileSync("git", ["rev-parse", "HEAD"], { cwd: ROOT, encoding: "utf8" }).trim();
	console.log(packageRelease({ ...args, commit }));
}
