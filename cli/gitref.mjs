// `--git-ref <ref>`: runs against the project's sources as they are at a git commit, for
// before/after renders without a second checkout.
//
// The files a run reads (the roots and the project files next to them, --rojo / --mount sources,
// the story) are extracted from each repository's <ref> into
// <state dir>/git-refs/<repo>-<hash>/<commit>/ through a temporary index (git read-tree +
// checkout-index; the repository's own index, HEAD and working tree are untouched) and reused by
// later runs of the same commit. Submodules (e.g. external/ data) are linked to the working tree's
// checkout. A story that does not exist at <ref> is rendered from the working tree.

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { stateDir } from "./paths.mjs";
import { withoutCookie } from "./assets.mjs";

function git(cwd, args, options = {}) {
	const result = spawnSync("git", ["-C", cwd, ...args], { encoding: options.encoding === null ? null : "utf8", maxBuffer: 512 * 1024 * 1024, windowsHide: true, env: withoutCookie(), ...options });
	if (result.error) throw new Error(`git ${args[0]}: ${result.error.message}`);
	if (result.status !== 0) throw new Error(`git ${args.join(" ")} (in ${cwd}): ${String(result.stderr || "").trim()}`);
	return result.stdout;
}

const slash = value => value.replace(/\\/g, "/");
const sameCase = value => (process.platform === "win32" ? value.toLowerCase() : value);
const inside = (parent, child) => {
	const relative = path.relative(parent, child);
	return !relative.startsWith("..") && !path.isAbsolute(relative);
};

// A repository at one commit, extracted on demand
class Snapshot {
	constructor(top, ref) {
		this.top = top;
		this.commit = git(top, ["rev-parse", "--verify", "--quiet", `${ref}^{commit}`]).trim();
		const id = crypto.createHash("sha1").update(sameCase(slash(top))).digest("hex").slice(0, 8);
		this.dir = path.join(stateDir(), "git-refs", `${path.basename(top)}-${id}`, this.commit);
		this.stateFile = path.join(this.dir, ".rur-git-ref.json");
		this.index = `${this.dir}.index`;
		this.done = new Set();
		try {
			this.done = new Set(JSON.parse(fs.readFileSync(this.stateFile, "utf8")).pathspecs);
		} catch {
			// nothing extracted yet
		}
	}

	env() {
		return { ...process.env, GIT_INDEX_FILE: this.index };
	}

	// Extracts the files matching `pathspecs` (repository-relative, git pathspec syntax)
	extract(pathspecs) {
		const wanted = [...new Set(pathspecs)].filter(spec => !this.done.has(spec));
		if (!wanted.length) return;
		fs.mkdirSync(this.dir, { recursive: true });
		if (!fs.existsSync(this.index)) git(this.top, ["read-tree", this.commit], { env: this.env() });
		const listed = git(this.top, ["ls-files", "-s", "-z", "--", ...wanted], { env: this.env() }).split("\0").filter(Boolean);
		const files = [];
		for (const line of listed) {
			const [meta, file] = line.split("\t");
			if (meta.startsWith("160000")) {
				// A submodule: link the working tree's checkout (the ref's own commit is not fetched)
				const source = path.join(this.top, file);
				const link = path.join(this.dir, file);
				if (fs.existsSync(source) && !fs.existsSync(link)) {
					fs.mkdirSync(path.dirname(link), { recursive: true });
					fs.symlinkSync(source, link, "junction");
				}
			} else {
				files.push(file);
			}
		}
		if (files.length) {
			git(this.top, ["checkout-index", "-f", "-z", "--stdin", `--prefix=${slash(this.dir)}/`], { env: this.env(), input: files.join("\0") });
		}
		for (const spec of wanted) this.done.add(spec);
		fs.writeFileSync(this.stateFile, JSON.stringify({ commit: this.commit, top: this.top, pathspecs: [...this.done] }, null, 1));
	}
}

export class GitRef {
	constructor(ref) {
		this.ref = String(ref);
		this.snapshots = new Map();
	}

	// The snapshot of the repository `file` belongs to (null outside any repository)
	snapshotOf(file) {
		let dir = fs.existsSync(file) && fs.statSync(file).isDirectory() ? file : path.dirname(file);
		while (!fs.existsSync(dir)) dir = path.dirname(dir);
		let top;
		try {
			top = path.resolve(git(dir, ["rev-parse", "--show-toplevel"]).trim());
		} catch {
			return null;
		}
		const key = sameCase(top);
		if (!this.snapshots.has(key)) {
			try {
				this.snapshots.set(key, new Snapshot(top, this.ref));
			} catch (error) {
				throw new Error(`--git-ref ${this.ref}: not a commit in ${top} (${error.message.split("\n")[0]})`);
			}
		}
		return this.snapshots.get(key);
	}

	// `file` (a working-tree path, or a path inside an extracted snapshot) as it is at the ref;
	// `extra` are further repository-relative pathspecs to extract with it
	map(file, { extra = [], required = false } = {}) {
		const real = fs.existsSync(file) ? fs.realpathSync(file) : path.resolve(file);
		for (const snapshot of this.snapshots.values()) {
			if (inside(snapshot.dir, real)) {
				snapshot.extract([slash(path.relative(snapshot.dir, real)) || "."]);
				return real;
			}
		}
		const snapshot = this.snapshotOf(real);
		if (!snapshot) {
			if (required) throw new Error(`--git-ref: ${file} is not inside a git repository`);
			process.stderr.write(`rur: --git-ref ${this.ref}: ${file} is not in a git repository; using it as it is\n`);
			return file;
		}
		const relative = slash(path.relative(snapshot.top, real));
		snapshot.extract([relative || ".", ...extra]);
		const mapped = path.join(snapshot.dir, relative);
		if (!fs.existsSync(mapped)) {
			process.stderr.write(`rur: --git-ref ${this.ref}: ${relative} does not exist at ${snapshot.commit.slice(0, 10)}; using the working tree's\n`);
			return file;
		}
		return mapped;
	}
}

// The run arguments with every source path moved to args["git-ref"] (see the header): `roots` are
// the run's roots (cli/rur.mjs projectRoots), `rojoFile` its Rojo project file. The project files
// the engine reads next to a root (the files beside it in <project>, such as rur.json, and submodule data
// in external/) come along.
export function applyGitRef(args, { roots, rojoFile }) {
	const ref = new GitRef(args["git-ref"]);
	const result = { ...args };
	delete result["git-ref"];
	const mappedRoots = roots.map(root => {
		const real = fs.realpathSync(root);
		const snapshot = ref.snapshotOf(real);
		if (!snapshot) return root;
		const relative = slash(path.relative(snapshot.top, real));
		const parent = slash(path.dirname(relative));
		const prefix = parent === "." ? "" : `${parent}/`;
		return ref.map(root, { extra: [`:(glob)${prefix}*`, `${prefix}external`] });
	});
	result.roots = mappedRoots.join(";");
	if (rojoFile) {
		const mapped = ref.map(rojoFile, { required: true });
		if (args.rojo && args.rojo !== true) result.rojo = mapped;
		else result.project = mapped;
	}
	if (args.mount !== undefined && args.mount !== true) {
		const list = (Array.isArray(args.mount) ? args.mount : [args.mount]).flatMap(value => String(value).split(";")).filter(Boolean);
		result.mount = list.map(item => {
			const index = item.indexOf("=");
			return index > 0 ? `${item.slice(0, index)}=${ref.map(path.resolve(item.slice(index + 1)))}` : item;
		});
	}
	for (const key of ["story", "fixture", "module", "model"]) {
		if (args[key] && args[key] !== true) result[key] = ref.map(path.resolve(String(args[key])));
	}
	result.gitRef = { ref: ref.ref, commits: [...ref.snapshots.values()].map(snapshot => ({ repo: snapshot.top, commit: snapshot.commit, dir: snapshot.dir })) };
	return { args: result, ref };
}
