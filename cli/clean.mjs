// `rur clean`: the output root (cli/paths.mjs OUT_DIR) without what costs space and holds nothing.
// One walk of the tree; every step below is lossless:
//
//   <dir>/assets/<file>       an image a render copied before render directories linked them
//                             (native/src/story_render.rs link_or_copy): replaced by a hard link to the
//                             cache file, or to the first identical copy elsewhere in the tree, when the
//                             bytes are equal (a frozen copy that differs from today's cache stays)
//   <render dir>/index.html   the retired Chromium pipeline's page (beside scene.json; a web bundle's
//                             index.html, beside rur-web.js, stays)
//   frames/NNNNN.{png,json}   a frame identical to one met before (both sides of a pixel-identical A/B,
//                             a still stretch) saved before captures stored frames by content: linked
//   .frames/<sha>.*           --save-frames store entries no frames/ file links to any more
//                             (native/src/headless.rs FrameStore)
//
// `--older-than <days>` also removes, outside regress/ and golden/ (frozen baselines), what was left
// untouched that long and is only the by-product of a run:
//   frames/                   --save-frames sequences
//   trace.json                headless runs' traces (beside benchmark.json, which holds the numbers)
//
// `--dry-run` reports what would go. Hard links need the cache and the output root on one volume;
// across volumes images stay copies.

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { OUT_DIR, stateDir } from "./paths.mjs";

const PROTECTED = new Set(["regress", "golden"]);
const DAY = 24 * 60 * 60 * 1000;

const list = directory => {
	try {
		return fs.readdirSync(directory, { withFileTypes: true });
	} catch {
		return [];
	}
};

function sha256(file) {
	return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

// Bytes and files under `file` (a file counted once however many links it has: what removing frees)
function measure(file) {
	let bytes = 0;
	let files = 0;
	const walk = entry => {
		let stat;
		try {
			stat = fs.lstatSync(entry);
		} catch {
			return;
		}
		if (stat.isDirectory()) {
			for (const item of list(entry)) walk(path.join(entry, item.name));
		} else {
			files++;
			if (stat.nlink <= 1) bytes += stat.size;
		}
	};
	walk(file);
	return { bytes, files };
}

// `target` replaced by a hard link to `source`: linked beside it and renamed over it, so `target`
// never goes missing
function relink(source, target) {
	const temporary = `${target}.${process.pid}.link`;
	fs.rmSync(temporary, { force: true });
	fs.linkSync(source, temporary);
	try {
		fs.renameSync(temporary, target);
	} catch (error) {
		fs.rmSync(temporary, { force: true });
		throw error;
	}
}

export function clean({ root = OUT_DIR, olderThanDays = null, dryRun = false, log = () => {} } = {}) {
	const result = { linked: { files: 0, bytes: 0 }, frames: { files: 0, bytes: 0 }, retired: { files: 0, bytes: 0 }, expired: { files: 0, bytes: 0 }, store: { files: 0, bytes: 0 }, failed: [] };
	const cacheAssets = path.join(stateDir(), "assets");
	const cutoff = olderThanDays === null ? null : Date.now() - olderThanDays * DAY;
	// sha256 -> the file every identical copy is linked to; sha256 by cache file (hashed when an out copy
	// of the same name and size is met)
	const canonical = new Map();
	const cacheHashed = new Set();
	const stores = [];

	const fail = (file, error) => result.failed.push({ path: file, error: error.code || error.message });

	// `file` linked to the first identical file met (an image: the cache file of its name first)
	const linkDuplicate = (file, bucket, { image = false } = {}) => {
		let stat;
		try {
			stat = fs.statSync(file, { bigint: true });
		} catch (error) {
			return fail(file, error);
		}
		if (!image && stat.nlink > 1n) return; // a frame captured into the store, or linked by an earlier clean
		if (image) {
			const name = path.basename(file);
			const cached = path.join(cacheAssets, name);
			let cachedStat = null;
			try {
				cachedStat = fs.statSync(cached, { bigint: true });
			} catch {}
			if (cachedStat && cachedStat.ino === stat.ino && cachedStat.dev === stat.dev) return; // already a link
			if (cachedStat && !cacheHashed.has(name)) {
				cacheHashed.add(name);
				if (cachedStat.size === stat.size) canonical.set(sha256(cached), cached);
			}
		}
		let hash;
		try {
			hash = sha256(file);
		} catch (error) {
			return fail(file, error);
		}
		const source = canonical.get(hash);
		if (!source) {
			canonical.set(hash, file);
			return;
		}
		const sourceStat = fs.statSync(source, { bigint: true });
		if (sourceStat.ino === stat.ino && sourceStat.dev === stat.dev) return;
		try {
			if (!dryRun) relink(source, file);
			bucket.files++;
			bucket.bytes += Number(stat.size);
		} catch (error) {
			// NTFS allows 1023 links to a file (a scroll bar image every render uses): this copy starts
			// the next group
			if (sourceStat.nlink >= 1023n) canonical.set(hash, file);
			else fail(file, error);
		}
	};

	const remove = (file, bucket) => {
		const { bytes, files } = measure(file);
		try {
			if (!dryRun) fs.rmSync(file, { recursive: true, force: true, maxRetries: 2 });
			bucket.files += files;
			bucket.bytes += bytes;
			log(`${dryRun ? "would remove" : "removed"} ${file}`);
		} catch (error) {
			fail(file, error);
		}
	};

	const older = file => {
		try {
			return fs.statSync(file).mtimeMs < cutoff;
		} catch {
			return false;
		}
	};

	const walk = (directory, protectedTree) => {
		const entries = list(directory);
		const names = new Set(entries.map(entry => entry.name));
		for (const entry of entries) {
			const file = path.join(directory, entry.name);
			if (entry.isDirectory()) {
				if (entry.name === ".frames") {
					stores.push(file);
				} else if (entry.name === "assets" && names.has("manifest.json")) {
					for (const asset of list(file)) if (asset.isFile()) linkDuplicate(path.join(file, asset.name), result.linked, { image: true });
				} else if (entry.name === "frames" && cutoff !== null && !protectedTree && older(file)) {
					remove(file, result.expired);
				} else if (entry.name === "frames") {
					for (const frame of list(file)) if (frame.isFile() && /^\d+\.(png|json)$/.test(frame.name)) linkDuplicate(path.join(file, frame.name), result.frames);
				} else {
					walk(file, protectedTree || (directory === root && PROTECTED.has(entry.name)));
				}
			} else if (entry.isFile()) {
				if (entry.name === "index.html" && names.has("scene.json") && !names.has("rur-web.js")) {
					remove(file, result.retired);
				} else if (entry.name === "trace.json" && names.has("benchmark.json") && cutoff !== null && !protectedTree && older(file)) {
					remove(file, result.expired);
				}
			}
		}
	};

	walk(root, false);
	// After expiry: an entry only the store itself holds (one link) is no frame's any more
	for (const store of stores) {
		for (const entry of list(store)) {
			const file = path.join(store, entry.name);
			try {
				const stat = fs.statSync(file);
				// A .tmp is a capture's write in progress (headless.rs FrameStore), unless it is old
				if (stat.nlink > 1 || (entry.name.endsWith(".tmp") && Date.now() - stat.mtimeMs < DAY)) continue;
				if (!dryRun) fs.rmSync(file, { force: true });
				result.store.files++;
				result.store.bytes += stat.size;
			} catch (error) {
				fail(file, error);
			}
		}
	}
	return result;
}
