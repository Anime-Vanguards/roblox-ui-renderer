// Rojo project files (*.project.json) as engine mounts (`rur-native engine --mount / --instance`,
// the emulator's src/project.rs mount_at).
//
// - A DataModel tree (`"$className": "DataModel"`): each node with a `$path` mounts that directory
//   or file at its instance path (ReplicatedStorage/Shared, StarterPlayer/StarterPlayerScripts/...);
//   `$className` nodes without a `$path` below a service become instances of that class.
// - Any other tree (a plugin or model project: the root is `src` as a bare Folder under no service)
//   is mounted at `ReplicatedStorage/<project name>`, or where `--rojo-at Service/Name` says.
// - `$path` to another *.project.json nests that project's tree; `{ "optional": path }` is skipped
//   when missing; `$properties`, `$attributes` and `$ignoreUnknownInstances` are ignored.
//
// `--mount Service/Name=<dir|file>` (repeatable, `;`-separated too) adds explicit mounts.
// native/src/rojo.rs is the same in Rust (`rur-native render --rojo`, and `rur-native rojo-mounts <file>`, which
// tools/tests/cli.mjs compares with rojoMounts); change both together.

import fs from "node:fs";
import path from "node:path";

// Services a DataModel tree names at its top level (the engine checks the rest)
const isService = name => /^[A-Z]/.test(name);

export function isRojoProject(file) {
	return typeof file === "string" && /\.project\.json$/i.test(file);
}

// [{ kind: "mount" | "instance", path: "Service/Name", value }] for a project file
// (`onPath(source)` sees every $path before it is checked: --git-ref extracts it there)
export function rojoMounts(projectFile, { at, onPath } = {}) {
	const file = path.resolve(projectFile);
	const project = JSON.parse(fs.readFileSync(file, "utf8"));
	if (!project || typeof project.tree !== "object") throw new Error(`${file}: not a Rojo project (no "tree")`);
	const mounts = [];
	const visit = (node, segments, base, depth = 0) => {
		if (depth > 32) throw new Error(`${file}: project nesting too deep`);
		let pathValue = node.$path;
		if (pathValue && typeof pathValue === "object") pathValue = pathValue.optional;
		if (typeof pathValue === "string") {
			const source = path.resolve(base, pathValue);
			if (onPath) onPath(source);
			if (!fs.existsSync(source)) {
				if (typeof node.$path !== "object") process.stderr.write(`rur: ${path.basename(file)}: ${segments.join("/") || "tree"} $path ${source} does not exist\n`);
			} else if (isRojoProject(source)) {
				const nested = JSON.parse(fs.readFileSync(source, "utf8"));
				if (nested && typeof nested.tree === "object") visit(nested.tree, segments, path.dirname(source), depth + 1);
			} else if (segments.length) {
				mounts.push({ kind: "mount", path: segments.join("/"), value: source });
			}
		} else if (typeof node.$className === "string" && segments.length > 1) {
			mounts.push({ kind: "instance", path: segments.join("/"), value: node.$className });
		}
		for (const [name, child] of Object.entries(node)) {
			if (name.startsWith("$") || !child || typeof child !== "object") continue;
			visit(child, [...segments, name], base, depth);
		}
	};
	const base = path.dirname(file);
	if (project.tree.$className === "DataModel") {
		for (const [name, child] of Object.entries(project.tree)) {
			if (name.startsWith("$") || !child || typeof child !== "object") continue;
			if (!isService(name)) throw new Error(`${file}: ${name} at the top of a DataModel tree is not a service`);
			visit(child, [name], base);
		}
	} else {
		const place = at && at !== true ? String(at).split(/[/\\]/).filter(Boolean) : ["ReplicatedStorage", String(project.name || "Project")];
		if (place.length < 2) throw new Error(`--rojo-at ${at}: needs Service/Name`);
		visit(project.tree, place, base);
	}
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

// The default.project.json a run with no project options mounts (native/src/rojo.rs detect): with no --rojo,
// --project, --roots, --mount, --place and no RUR_PROJECT, a --story or --module outside the default project
// mounts the default.project.json nearest it (its folder or a parent), else the current directory's. Sets
// args.rojo and returns the line saying so, or null.
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
	const candidates = [];
	for (let dir = path.dirname(file); ; dir = path.dirname(dir)) {
		candidates.push(path.join(dir, "default.project.json"));
		if (path.dirname(dir) === dir) break;
	}
	candidates.push(path.join(process.cwd(), "default.project.json"));
	const found = candidates.find(candidate => fs.existsSync(candidate) && fs.statSync(candidate).isFile());
	if (!found) return null;
	args.rojo = found;
	const mounted = rojoMounts(found).map(mount => mount.path);
	return `rur: Rojo project ${found} (found beside the story; --rojo, --project or --roots picks another): mounted ${mounted.length ? mounted.join(", ") : "nothing"}`;
}

// Engine arguments for the mounts
export function mountArgs(mounts) {
	return mounts.flatMap(mount => [mount.kind === "instance" ? "--instance" : "--mount", `${mount.path}=${mount.value}`]);
}
