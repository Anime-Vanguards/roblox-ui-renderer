// Runs a Luau probe in the open Studio (Edit datamodel) while holding the Studio lock, prints the
// string it returns (or writes it to --out). Probes must only create temporary GUI under CoreGui and
// destroy it before returning.
//
//   node tools/interaction/run.mjs tools/interaction/probes/<name>.luau [--out file.json] [--studio <name>]

import fs from "node:fs";
import path from "node:path";
import { runLuau, DEFAULT_STUDIO } from "../../cli/studio.mjs";
import { withStudioLock } from "../../cli/lock.mjs";

const args = process.argv.slice(2);
const file = args.find(arg => !arg.startsWith("--") && !args[args.indexOf(arg) - 1]?.startsWith("--"));
const flag = name => {
	const index = args.indexOf(`--${name}`);
	return index >= 0 ? args[index + 1] : undefined;
};
if (!file) {
	process.stderr.write("usage: node tools/interaction/run.mjs <probe.luau> [--out file] [--studio name]\n");
	process.exit(2);
}

const code = fs.readFileSync(file, "utf8");
const studio = flag("studio") || DEFAULT_STUDIO;
const result = await withStudioLock(`interaction ${path.basename(file)}`, () => runLuau(code, { studio }));
const out = flag("out");
if (out) {
	fs.mkdirSync(path.dirname(out), { recursive: true });
	fs.writeFileSync(out, result.text);
	process.stdout.write(`wrote ${result.text.length} chars to ${out}\n`);
} else {
	process.stdout.write(result.text + "\n");
}
