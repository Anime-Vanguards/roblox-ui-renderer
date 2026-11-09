// The smoke test's file inventory: the size and modification time of every file under some roots.
import fs from "node:fs";
import path from "node:path";

// Returns a map from each file under `roots` to its "size:mtime", leaving out symbolic links and
// the `sandbox` folder. A root that is missing or unset is skipped, and so is a folder reached
// twice, for example as two roots that name the same place. A root inside the sandbox is still
// read: only the sandbox met while walking another root is skipped.
//
// Roots and the sandbox are both resolved to their real locations, because temporary folders can
// be reached through links (/var is a link to /private/var on macOS, and /tmp can be one on
// Linux). Compared as written, a sandbox named by its link would never equal the walked path.
export function fileStates(roots, sandbox) {
	const excludedSandbox = fs.realpathSync(sandbox);
	const walkedFolders = new Set();
	const stateByFile = new Map();

	function walk(folder) {
		if (walkedFolders.has(folder)) return;
		walkedFolders.add(folder);
		for (const entry of fs.readdirSync(folder, { withFileTypes: true })) {
			const file = path.join(folder, entry.name);
			if (file === excludedSandbox || entry.isSymbolicLink()) continue;
			if (entry.isDirectory()) {
				walk(file);
			} else if (entry.isFile()) {
				const stat = fs.statSync(file);
				stateByFile.set(file, `${stat.size}:${stat.mtimeMs}`);
			}
		}
	}

	for (const root of roots) {
		if (!root || !fs.existsSync(root)) continue;
		walk(fs.realpathSync(root));
	}
	return stateByFile;
}
