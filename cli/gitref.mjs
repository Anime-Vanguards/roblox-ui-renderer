// `--git-ref <ref>`: runs against the project's sources as they are at a git commit, for before/after renders
// without a second checkout. The extraction is rur-native's (`rur-native git-ref`, native/src/gitref.rs, which
// `rur diff` and `rur sweep --git-ref` use too): the files a run reads (the roots and the project files next to them,
// the Rojo project and its $paths, --mount sources, the story) are extracted from each repository's <ref> into
// <state dir>/git-refs/<repo>-<hash>/<commit>/ and reused by later runs of the same commit. A file the commit does not
// have (a story or mount added since, a folder git ignores that the project maps) is taken from the working tree.

import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { ensureEngine } from "./engine.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// The run arguments with every source moved to args["git-ref"], and the run's project as engine arguments (`--roots`
// and the mounts, at the ref): `roots` are the run's roots (cli/rur.mjs projectRoots), `rojoFile` its Rojo project
// file. Returns { args, project, gitRef } (gitRef: { ref, commits: [{ repo, commit, dir }] }, for the summary).
export async function applyGitRef(args, { roots, rojoFile }) {
	const argv = ["git-ref", "--repo", ROOT, "--git-ref", String(args["git-ref"]), "--roots", roots.join(";")];
	if (rojoFile) argv.push("--rojo", path.resolve(rojoFile));
	if (args["rojo-at"] && args["rojo-at"] !== true) argv.push("--rojo-at", String(args["rojo-at"]));
	const mounts = args.mount === undefined || args.mount === true ? [] : (Array.isArray(args.mount) ? args.mount : [args.mount]);
	for (const mount of mounts) argv.push("--mount", String(mount));
	for (const key of ["story", "fixture", "module", "model"]) {
		if (args[key] && args[key] !== true) argv.push(`--${key}`, String(args[key]));
	}
	// (its notes, such as a file taken from the working tree, go to stderr)
	const result = spawnSync(await ensureEngine(), argv, { cwd: process.cwd(), encoding: "utf8", windowsHide: true, stdio: ["ignore", "pipe", "inherit"], maxBuffer: 64 * 1024 * 1024 });
	if (result.error) throw result.error;
	if (result.status !== 0) throw new Error(`--git-ref ${args["git-ref"]}: rur-native git-ref failed (exit ${result.status})`);
	const answer = JSON.parse(result.stdout.trim().split(/\r?\n/).pop());
	const mapped = { ...args, ...answer.sources };
	delete mapped["git-ref"];
	return { args: mapped, project: answer.project, gitRef: answer.gitRef };
}
