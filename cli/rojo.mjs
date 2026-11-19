// Rojo project files (*.project.json) as engine mounts (`rur-native engine --mount / --instance`,
// the emulator's src/project.rs mount_at).
//
// - A DataModel tree (`"$className": "DataModel"`): each node with a `$path` mounts that directory
//   or file at its instance path (ReplicatedStorage/Shared, StarterPlayer/StarterPlayerScripts/...);
//   `$className` nodes without a `$path` below a service become instances of that class.
// - Any other tree (a plugin or model project: the root is `src` as a bare Folder under no service)
//   is mounted at `ReplicatedStorage/<project name>`, or where `--rojo-at Service/Name` says.
// - A node whose `$path` names another *.project.json is that project's tree, as Rojo builds it: the
//   tree's class, `$path` and children (paths relative to that file), then the node's own children;
//   the node's `$className` wins only over a Folder, and any other pair is refused. The tree's root
//   too, so a root that is a `$path` to a DataModel project, with more services beside it, is a
//   DataModel tree. Chains are followed; project files whose `$path`s form a cycle are refused.
// - `{ "optional": path }` is skipped when missing; `$properties`, `$attributes` and
//   `$ignoreUnknownInstances` are ignored.
//
// `--mount Service/Name=<dir|file>` (repeatable, `;`-separated too) adds explicit mounts.
// native/src/rojo.rs is the same in Rust (`rur-native render --rojo`, and `rur-native rojo-mounts <file>`, which
// tools/tests/cli.mjs compares with rojoMounts); its reader is the engine's (roblox_engine_emulator::rojo, which
// `ree run --rojo` uses too), so a rule changes here and in the engine together.

import fs from "node:fs";
import path from "node:path";

// Services a DataModel tree names at its top level (the engine checks the rest)
const isService = name => /^[A-Z]/.test(name);

// The folders Wally installs packages into
const WALLY_FOLDERS = ["Packages", "ServerPackages", "DevPackages"];

// What to say about a $path that does not exist (native/src/rojo.rs missing_path)
function missingPath(file, at, source) {
	const dir = path.dirname(file);
	const relative = path.relative(dir, source);
	const shown = relative && !relative.startsWith("..") && !path.isAbsolute(relative) ? relative : source;
	const wally = WALLY_FOLDERS.includes(path.basename(source)) && fs.existsSync(path.join(dir, "wally.toml")) ? `: run \`wally install\` in ${dir}` : "";
	return `${path.basename(file)} maps ${at.replaceAll("/", ".")} to ${shown}, which doesn't exist${wally}`;
}

// The *.project.json files in a folder, by name
function projectFilesIn(dir) {
	let names = [];
	try {
		names = fs.readdirSync(dir);
	} catch {
		return [];
	}
	return names.filter(name => isRojoProject(name)).map(name => path.join(dir, name)).filter(file => fs.statSync(file, { throwIfNoEntry: false })?.isFile()).sort();
}

// The project a folder names by itself (native/src/rojo.rs project_file_in): { file } for its default.project.json,
// else its only *.project.json; { several } when it has more and none is default.project.json; {} for none
export function projectFileIn(dir) {
	const fallback = path.join(dir, "default.project.json");
	if (fs.statSync(fallback, { throwIfNoEntry: false })?.isFile()) return { file: fallback };
	const files = projectFilesIn(dir);
	return files.length === 1 ? { file: files[0] } : files.length > 1 ? { several: files } : {};
}

export function isRojoProject(file) {
	return typeof file === "string" && /\.project\.json$/i.test(file);
}

// How many project files deep a `$path` chain can go (the engine's MAX_PROJECT_NESTING)
const MAX_PROJECT_NESTING = 32;

// A JSON object: not an array or null
const isObject = value => Boolean(value) && typeof value === "object" && !Array.isArray(value);

// `file` relative to `dir` when it is inside it (the engine's rojo.rs shown)
function shown(file, dir) {
	const relative = path.relative(dir, file);
	return relative && !relative.startsWith("..") && !path.isAbsolute(relative) ? relative : file;
}

// `file` as the cycle check compares it: its real path, when it has one
function identity(file) {
	try {
		return fs.realpathSync.native(file);
	} catch {
		return file;
	}
}

function readProject(file) {
	const text = fs.readFileSync(file, "utf8");
	try {
		return JSON.parse(text);
	} catch (error) {
		throw new Error(`${file}: ${error.message}`);
	}
}

// [{ kind: "mount" | "instance", path: "Service/Name", value }] for a project file
// (`onPath(source)` sees every $path before it is checked: --git-ref extracts it there)
//
// The tree is built first, then placed (the engine's rojo.rs walk): a node is { className, source, children },
// its source { kind: "found" | "missing" | "skipped", path } for a `$path` that isn't a project file, else null.
export function rojoMounts(projectFile, { at, onPath } = {}) {
	const file = path.resolve(projectFile);
	const project = readProject(file);
	if (!isObject(project) || !isObject(project.tree)) throw new Error(`${file}: not a Rojo project (no "tree")`);
	// The project files the node being built sits in, the outermost first
	const files = [{ file, real: identity(file) }];
	// Node `name` (its `$path`s relative to `base`) as Rojo builds it (snapshot_project_node in Rojo's
	// src/snapshot_middleware/project.rs): a `$path` to another project file makes the node that project's tree, and
	// the node's own children follow its children
	const build = (node, name, base) => {
		const built = { className: typeof node.$className === "string" ? node.$className : undefined, source: null, children: [] };
		const optional = isObject(node.$path);
		const pathValue = optional ? node.$path.optional : node.$path;
		if (typeof pathValue === "string") {
			const source = path.resolve(base, pathValue);
			if (onPath) onPath(source);
			if (!fs.existsSync(source)) {
				built.source = { kind: optional ? "skipped" : "missing", path: source };
			} else if (isRojoProject(source)) {
				const referenced = buildProject(source);
				built.className = combinedClass(name, built.className, referenced.className, source);
				built.source = referenced.source;
				built.children = referenced.children;
			} else {
				built.source = { kind: "found", path: source };
			}
		}
		for (const [childName, child] of Object.entries(node)) {
			if (childName.startsWith("$") || !isObject(child)) continue;
			built.children.push([childName, build(child, childName, base)]);
		}
		return built;
	};
	// The tree of project file `source`, which a `$path` names; nothing for a file without a tree. A file already
	// among `files` is refused: a project can't include itself.
	const buildProject = source => {
		const real = identity(source);
		const first = files.findIndex(seen => seen.real === real);
		if (first >= 0) {
			const cycle = [...files.slice(first).map(seen => seen.file), source].map(seen => shown(seen, path.dirname(file)));
			throw new Error(`${file}: the project files' $paths form a cycle (${cycle.join(" -> ")}): a project can't include itself`);
		}
		if (files.length > MAX_PROJECT_NESTING) throw new Error(`${file}: project nesting too deep`);
		const referenced = readProject(source);
		if (!isObject(referenced) || !isObject(referenced.tree)) return { className: undefined, source: null, children: [] };
		files.push({ file: source, real });
		try {
			return build(referenced.tree, "tree", path.dirname(source));
		} finally {
			files.pop();
		}
	};
	// Rojo's rule for a node whose `$path` names a project: the project root's class, or the node's own `$className`
	// when the root's is a Folder or names none; both with another root class is refused
	const combinedClass = (name, own, fromPath, source) => {
		if (own !== undefined && fromPath !== undefined && fromPath !== "Folder") {
			const inFile = files[files.length - 1].file;
			throw new Error(`${inFile}: ${name} has "$className": "${own}" and a $path to ${shown(source, path.dirname(inFile))}, whose tree is a ${fromPath}: Rojo takes both only when the $path is a Folder`);
		}
		return own ?? fromPath;
	};
	const mounts = [];
	const missing = [];
	// The mounts of built node `node` at instance path `segments`, then its children's below it
	const place = (node, segments) => {
		const at = segments.join("/");
		if (node.source?.kind === "found") mounts.push({ kind: "mount", path: at, value: node.source.path });
		else if (node.source?.kind === "missing") missing.push([at, node.source.path]);
		else if (!node.source && node.className !== undefined && segments.length > 1) mounts.push({ kind: "instance", path: at, value: node.className });
		for (const [name, child] of node.children) place(child, [...segments, name]);
	};
	// (the root's class is the one its project files give: a tree that is a `$path` to a DataModel project is one)
	const tree = build(project.tree, "tree", path.dirname(file));
	if (tree.className === "DataModel") {
		for (const [name, child] of tree.children) {
			if (!isService(name)) throw new Error(`${file}: ${name} at the top of a DataModel tree is not a service`);
			place(child, [name]);
		}
	} else {
		const segments = at && at !== true ? String(at).split(/[/\\]/).filter(Boolean) : ["ReplicatedStorage", String(project.name || "Project")];
		if (segments.length < 2) throw new Error(`--rojo-at ${at}: needs Service/Name`);
		place(tree, segments);
	}
	for (const [where, source] of missing) process.stderr.write(`rur: warning: ${missingPath(file, where, source)}\n`);
	return mounts;
}

// The run's Rojo project file: --rojo <file>, or --project <file.project.json>
export function rojoFileOf(args) {
	return args.rojo && args.rojo !== true ? String(args.rojo) : isRojoProject(args.project) ? String(args.project) : null;
}

// Every mount of a run: --rojo <file> (or --project <file.project.json>) and --mount entries
export function runMounts(args, { onPath } = {}) {
	const mounts = [];
	const rojo = rojoFileOf(args);
	if (rojo) mounts.push(...rojoMounts(rojo, { at: args["rojo-at"], onPath }));
	const explicit = args.mount === undefined || args.mount === true ? [] : (Array.isArray(args.mount) ? args.mount : [args.mount]);
	for (const item of explicit.flatMap(value => String(value).split(";")).filter(Boolean)) {
		const index = item.indexOf("=");
		if (index <= 0) throw new Error(`--mount ${item}: expected Service/Name=<dir|file>`);
		mounts.push({ kind: "mount", path: item.slice(0, index), value: path.resolve(item.slice(index + 1)) });
	}
	return mounts;
}

// The options that choose a run's project: with any of them no project is detected
const PROJECT_OPTIONS = ["rojo", "project", "roots", "mount", "place", "rojo-at", "git-ref"];

// The project file a run with no project options mounts (native/src/rojo.rs detect): with no --rojo,
// --project, --roots, --mount, --place and no RUR_PROJECT, a --story or --module outside the default project
// mounts the project file nearest it (its folder or a parent: projectFileIn), else the current directory's. Sets
// args.rojo and returns the line saying so; a nearest folder with several project files is named instead; else null.
export function detectRojoProject(args, defaultProject) {
	if (PROJECT_OPTIONS.some(key => args[key] !== undefined && args[key] !== false)) return null;
	if (process.env.RUR_PROJECT) return null;
	const source = ["story", "module"].map(key => args[key]).find(value => value && value !== true);
	if (!source) return null;
	let file = path.resolve(String(source));
	try {
		file = fs.realpathSync.native(file);
	} catch {
		// a missing source is reported by the run
	}
	const real = dir => {
		try {
			return fs.realpathSync.native(dir);
		} catch {
			return path.resolve(dir);
		}
	};
	const project = real(defaultProject);
	const inside = (child, parent) => {
		const relative = path.relative(parent, child);
		return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
	};
	if (inside(file, project)) return null;
	const dirs = [];
	for (let dir = path.dirname(file); ; dir = path.dirname(dir)) {
		dirs.push(dir);
		if (path.dirname(dir) === dir) break;
	}
	dirs.push(process.cwd());
	for (const dir of dirs) {
		const { file: found, several } = projectFileIn(dir);
		if (several) return `rur: warning: ${dir} has several Rojo projects (${several.map(file => path.basename(file)).join(", ")}), so none was mounted: pass --rojo <file> to mount one`;
		if (!found) continue;
		args.rojo = found;
		const mounted = rojoMounts(found).filter(mount => mount.kind === "mount").map(mount => mount.path.replaceAll("/", "."));
		return `rur: Rojo project ${found} (found beside the story; --rojo picks another): mounted ${mounted.length ? mounted.join(", ") : "nothing"}`;
	}
	return null;
}

// Engine arguments for the mounts
export function mountArgs(mounts) {
	return mounts.flatMap(mount => [mount.kind === "instance" ? "--instance" : "--mount", `${mount.path}=${mount.value}`]);
}
