// rur's npm packages (package.json "dependencies"): which are missing from this checkout, and what to say
// when a module cannot be loaded for want of them.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// The dependencies node_modules does not hold: a checkout that was cloned or updated without `npm install`
export function missingPackages(root = ROOT) {
	let dependencies = {};
	try {
		dependencies = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8")).dependencies || {};
	} catch {
		return [];
	}
	return Object.keys(dependencies).filter(name => !fs.existsSync(path.join(root, "node_modules", name, "package.json")));
}

// A module that failed to load because a package is not installed says which command installs it, in place
// of Node's "Cannot find package 'x' imported from <file>". Any other error is returned as it is.
export function packageError(error, root = ROOT) {
	if (error?.code !== "ERR_MODULE_NOT_FOUND") return error;
	const missing = missingPackages(root);
	if (!missing.length) return error;
	return new Error(`rur's npm packages are not installed (${missing.join(", ")}): run \`npm install\` in ${root}`, { cause: error });
}
