// The pinned FreeType source the native app builds against (tools/native-host/source.mjs). The
// fingerprint covers these files, so a change to them rebuilds the app (tools/native-app/config.mjs).
// (The Luau VM and Lune's datatypes come patched with the engine, roblox-engine-emulator.)
import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

export const HOST_FILES = ["config.mjs", "build-lock.mjs", "source.mjs"];

// Source bytes as the fingerprints hash them: text with CRLF line endings as LF, so a checkout with
// core.autocrlf=true, a copy of it and a Linux clone agree. A file with a NUL byte is binary, as is.
export function normalizedSource(bytes) {
	if (bytes.includes(0) || !bytes.includes(13)) return bytes;
	return Buffer.from(bytes.toString("latin1").replaceAll("\r\n", "\n"), "latin1");
}

export function buildFingerprint() {
	const hash = createHash("sha256");
	for (const name of HOST_FILES) {
		hash.update(name).update("\0").update(normalizedSource(fs.readFileSync(path.join(ROOT, "tools/native-host", name))));
	}
	return hash.digest("hex");
}
