// Whether a script was started from the command line, as opposed to imported by another module.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// The real path of a file, or the path as written when it cannot be resolved
export function realPath(file) {
	try {
		return fs.realpathSync(file);
	} catch {
		return path.resolve(file);
	}
}

// True when `node <this module>` started the process. Node resolves the links in a module's own path
// but leaves the command line's entry path as written, so a launch through a linked folder (a symlink,
// or a junction on Windows) names the same file in different words: both are compared as real paths.
// A plain comparison saw two files there, and the script ran nothing and exited 0.
export function isMain(moduleUrl) {
	if (!process.argv[1]) return false;
	return realPath(path.resolve(process.argv[1])) === realPath(fileURLToPath(moduleUrl));
}
