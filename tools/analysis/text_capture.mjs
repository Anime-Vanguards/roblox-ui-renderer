// Studio capture of a text probe, bracketed by projection-stretch checks: Studio's viewport projection is
// intermittently stretched vertically by half a pixel (notes/text.md "Capture environment"), which blurs
// every glyph. Before and after the capture, fixtures/text-probe-place.luau is captured again and compared
// with a known clean capture (out/compare/text-probe-place/studio.png): identical pixels = clean. A stretched
// Studio is waited out (--wait seconds between tries, --tries).
//
//   node tools/analysis/text_capture.mjs fixtures/text-probe-X.luau [--tries 10] [--wait 60] [compare args...]
// Default compare args: --viewport 1200x780 --time 0.2 --background 0,0,0

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const argv = process.argv.slice(2);
const option = (name, fallback) => {
	const index = argv.indexOf(name);
	if (index < 0) {
		return fallback;
	}
	const value = argv[index + 1];
	argv.splice(index, 2);
	return value;
};
const tries = Number(option("--tries", 10));
const wait = Number(option("--wait", 60));
const fixture = argv.shift();
const extra = argv.length ? argv : ["--viewport", "1200x780", "--time", "0.2", "--background", "0,0,0"];

const reference = path.join(ROOT, "out/compare/text-probe-place/studio.png");
const sleep = seconds => spawnSync(process.execPath, ["-e", `setTimeout(() => {}, ${seconds * 1000})`]);

function compare(args) {
	const result = spawnSync(process.execPath, [path.join(ROOT, "cli/rur.mjs"), "compare", ...args], { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
	if (result.status !== 0) {
		throw new Error(`compare failed: ${result.stderr.slice(-2000)}`);
	}
}

function clean() {
	const out = path.join(ROOT, "out/compare/stretch-check");
	compare(["--fixture", path.join(ROOT, "fixtures/text-probe-place.luau"), "--viewport", "1200x780", "--time", "0.2", "--background", "0,0,0", "--out", out, "--recapture"]);
	const a = PNG.sync.read(fs.readFileSync(reference)), b = PNG.sync.read(fs.readFileSync(path.join(out, "studio.png")));
	// RGB only: since 2026-09-26 captures carry alpha 0 in the last two (partial) rows while their colours are
	// unchanged, which failed every byte-for-byte check
	if (a.width !== b.width || a.height !== b.height) return false;
	// Only the columns Studio's viewport actually shows: when the Studio window is narrower than the
	// check's 2060 logical px (2004 on 2026-09-28), the right column of labels is cut off, which is not a
	// stretch (report.json studio.viewport, logical px; 1.5x).
	let visible = a.width;
	try {
		const viewport = JSON.parse(fs.readFileSync(path.join(out, "report.json"), "utf8")).studio.viewport;
		visible = Math.min(a.width, Math.floor(viewport[0] * 1.5) - 8);
	} catch {}
	for (let y = 0; y < a.height; y++) {
		for (let x = 0; x < visible; x++) {
			const k = (y * a.width + x) * 4;
			if (a.data[k] !== b.data[k] || a.data[k + 1] !== b.data[k + 1] || a.data[k + 2] !== b.data[k + 2]) return false;
		}
	}
	return true;
}

for (let attempt = 1; attempt <= tries; attempt++) {
	if (!clean()) {
		console.log(`attempt ${attempt}: Studio is stretched, waiting ${wait} s`);
		sleep(wait);
		continue;
	}
	// --recapture: a saved capture would be reused without touching Studio (cli/compare.mjs)
	compare(["--fixture", path.resolve(ROOT, fixture), ...extra, "--recapture"]);
	if (clean()) {
		console.log(`${fixture}: clean capture (attempt ${attempt})`);
		process.exit(0);
	}
	console.log(`attempt ${attempt}: Studio became stretched during the capture, retrying`);
}
console.log(`${fixture}: no clean capture after ${tries} attempts`);
process.exit(1);
