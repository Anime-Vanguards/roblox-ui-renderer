// Re-renders saved `rur compare` outputs with the current renderer and scores them against their saved
// Studio captures (no Studio, no runtime): for renderer-only changes while Studio is busy.
//   node tools/analysis/rerender_compare.mjs out/compare/<name> [out/compare/<name> ...] [--out out/rerender]
// Prints the saved and the re-rendered exact % (pixels within 2 levels) per compare.
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";
import { comparePngs } from "../../cli/compare.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const args = process.argv.slice(2);
const outIndex = args.indexOf("--out");
const outRoot = path.resolve(outIndex >= 0 ? args[outIndex + 1] : path.join(ROOT, "out", "rerender"));
const dirs = args.filter((arg, index) => !arg.startsWith("--") && (outIndex < 0 || index !== outIndex + 1));

for (const dir of dirs) {
	const name = path.basename(path.resolve(dir));
	const scene = path.join(dir, "rur", "scene.json");
	const studio = path.join(dir, "studio.png");
	if (!fs.existsSync(scene) || !fs.existsSync(studio)) {
		console.log(`${name}: missing rur/scene.json or studio.png`);
		continue;
	}
	// Unique per compare: parity/<name> and an older run's <name> must not share an output folder
	const out = path.join(outRoot, path.relative(path.join(ROOT, "out", "compare"), path.resolve(dir)).replace(/[\\/:]+/g, "_") || name);
	const result = spawnSync(process.execPath, [path.join(ROOT, "cli", "rur.mjs"), "render", "--scene", scene, "--out", out, "--offline"], { cwd: ROOT, encoding: "utf8" });
	if (result.status !== 0) {
		console.log(`${name}: render failed\n${result.stderr.slice(-1000)}`);
		continue;
	}
	const saved = JSON.parse(fs.readFileSync(path.join(dir, "report.json"), "utf8")).pixels?.exactPercent;
	const pixels = comparePngs(PNG.sync.read(fs.readFileSync(studio)), PNG.sync.read(fs.readFileSync(path.join(out, "render.png"))), path.join(out, "diff.png"));
	console.log(`${name}: saved ${saved?.toFixed(4)}% -> now ${pixels.exactPercent.toFixed(4)}%`);
}
