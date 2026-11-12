// Adds up `rur compare` reports: pixels more than 2 levels apart per comparison and in total. Pixels a fixture
// marks unstable (a `-- compare-unstable` header line, cli/compare.mjs unstableOf) are listed with the reason
// and left out of the total.
//   node tools/analysis/compare_total.mjs [name ...] [--dir out/compare]
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { pixelLine } from "../../cli/compare.mjs";

// The lines and the total for reports ({ name, pixels } each; pixels as comparePngs returns them)
export function totals(reports) {
	const lines = [];
	let total = 0, excluded = 0;
	for (const { name, pixels } of reports) {
		const counted = pixels.counted ?? pixels.over2 ?? 0;
		total += counted;
		excluded += pixels.unstable ? pixels.unstable.over2 : 0;
		lines.push(`${name}: ${pixelLine({ over2: counted, ...pixels, counted })}`);
	}
	lines.push(`total: ${total.toLocaleString("en-US")} px over 2 levels in ${reports.length} comparison${reports.length === 1 ? "" : "s"}${excluded ? ` (${excluded.toLocaleString("en-US")} px not counted: unstable in Studio)` : ""}`);
	return { lines, total, excluded };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const argv = process.argv.slice(2);
	const at = argv.indexOf("--dir");
	const dir = path.resolve(at >= 0 ? argv.splice(at, 2)[1] : "out/compare");
	const names = argv.length ? argv : fs.readdirSync(dir).filter(name => fs.existsSync(path.join(dir, name, "report.json"))).sort();
	const reports = [];
	for (const name of names) {
		const file = path.join(dir, name, "report.json");
		if (!fs.existsSync(file)) {
			console.error(`${name}: no report (${file})`);
			process.exitCode = 1;
			continue;
		}
		const pixels = JSON.parse(fs.readFileSync(file, "utf8")).pixels;
		if (pixels.over2 === undefined) {
			console.error(`${name}: its report was written before pixels were counted; run rur compare again`);
			process.exitCode = 1;
			continue;
		}
		reports.push({ name, pixels });
	}
	console.log(totals(reports).lines.join("\n"));
}
