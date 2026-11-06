// Re-runs the offline side of saved Studio comparisons with the current runtime, exporter and renderer
// (the same arguments and recorded frame clock as `rur compare`), next to the saved studio.png, so text
// work can be scored against a capture without taking Studio again.
//
//   node tools/analysis/text_rerun.mjs <out/compare/name>... [--out-root out/text/rerun]
// Writes <out-root>/<name>/ (a normal render dir); score it with
//   python tools/analysis/text_parity.py <out/compare/name> --rur <out-root>/<name>

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { render } from "../../cli/rur.mjs";
import { withRecordedClock } from "../../cli/clock.mjs";
import { withStudioPlayer } from "../../cli/compare.mjs";
import { placeOf, placeRoots } from "../../cli/places.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const argv = process.argv.slice(2);
const outRootIndex = argv.indexOf("--out-root");
const outRoot = path.resolve(ROOT, outRootIndex >= 0 ? argv[outRootIndex + 1] : "out/text/rerun");
const dirs = argv.filter((arg, index) => !arg.startsWith("--") && argv[index - 1] !== "--out-root");

for (const dir of dirs) {
	const compareDir = path.resolve(ROOT, dir);
	const report = JSON.parse(fs.readFileSync(path.join(compareDir, "report.json"), "utf8"));
	const options = report.options || {};
	const source = String(report.source);
	const name = path.basename(compareDir);
	const kind = /\.rbxmx?$/.test(source) ? "model" : source.includes(".story.") ? "story" : "fixture";
	const args = {
		[kind]: path.resolve(ROOT, source),
		out: path.join(outRoot, name),
		viewport: options.viewport || "1200x900",
		scale: String(options.scale || 1.5),
		time: String(options.time ?? 1),
		background: (options.background || [46, 46, 46]).join(","),
		reference: path.join(compareDir, "studio.png"),
	};
	if (options.camera) args.camera = options.camera;
	if (options.placeId) args["place-id"] = String(options.placeId);
	if (options.place) args.place = options.place;
	if (options.controls) args.controls = typeof options.controls === "string" ? options.controls : JSON.stringify(options.controls);
	if (kind === "story") {
		// Story roots: the project's place that holds the story (RUR_PROJECT, else the example project)
		const project = path.resolve(process.env.RUR_PROJECT || path.join(ROOT, "examples", "project"));
		args.roots = placeRoots(project, options.place || placeOf(project, source)).join(";");
	}
	const clock = options.clock && fs.existsSync(options.clock) ? options.clock : null;
	const summary = await withStudioPlayer(options.player || null, () => withRecordedClock(clock, () => render(args)));
	console.log(`${name}: ${summary.status}, ${summary.nodes} nodes -> ${path.relative(ROOT, args.out)}`);
}
