// Studio capture stretch (notes/text.md "Capture environment"): Studio's viewport projection is
// intermittently off by half a pixel over the viewport height, which blurs text and images in every capture.
// tools/analysis/text_subpixel.py fits each plain label's sub-pixel offset in a compare dir; a clean capture
// fits dy = 0 everywhere, a stretched one dy growing with y (overHeight: the growth over 1350 px).
import fs from "node:fs";
import path from "node:path";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

// A capture whose fitted dy grows by this much over the viewport height is stretched
export const STRETCH_LIMIT = 0.25;

// The fit depends on the compare dir's Studio capture, offline render and scene (and the script): it is
// kept in <dir>/capture-stretch.json under a hash of those and computed again only when one changed
export function captureStretch(dir) {
	const script = path.join(ROOT, "tools", "analysis", "text_subpixel.py");
	const key = createHash("sha1");
	for (const file of [path.join(dir, "studio.png"), path.join(dir, "rur", "render.png"), path.join(dir, "rur", "scene.json"), script]) {
		try {
			key.update(fs.readFileSync(file));
		} catch {
			key.update("missing");
		}
		key.update("\0");
	}
	const digest = key.digest("hex");
	const cacheFile = path.join(dir, "capture-stretch.json");
	try {
		const cached = JSON.parse(fs.readFileSync(cacheFile, "utf8"));
		if (cached.key === digest) return cached.stretch;
	} catch {
		// compute it
	}
	const stretch = fitStretch(dir, script);
	try {
		fs.writeFileSync(cacheFile, JSON.stringify({ key: digest, stretch }));
	} catch {
		// read-only dir: not cached
	}
	return stretch;
}

function fitStretch(dir, script) {
	const result = spawnSync(process.env.RUR_PYTHON || "python", [script, dir], { cwd: ROOT, encoding: "utf8", maxBuffer: 1 << 26 });
	if (result.status !== 0) {
		return null;
	}
	const points = [];
	for (const line of result.stdout.split(/\r?\n/)) {
		const match = line.match(/top\s+(-?[\d.]+).*best dx [+-][\d.]+ dy ([+-][\d.]+)/);
		if (match) {
			points.push([Number(match[1]), Number(match[2])]);
		}
	}
	if (points.length < 4) {
		return null;
	}
	const meanY = points.reduce((sum, [y]) => sum + y, 0) / points.length;
	const meanD = points.reduce((sum, [, d]) => sum + d, 0) / points.length;
	let covariance = 0, variance = 0;
	for (const [y, d] of points) {
		covariance += (y - meanY) * (d - meanD);
		variance += (y - meanY) * (y - meanY);
	}
	const slope = variance > 0 ? covariance / variance : 0;
	return { labels: points.length, slope, overHeight: +(slope * 1350).toFixed(3) };
}
