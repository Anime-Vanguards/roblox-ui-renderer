// A checkout without its npm packages says so: the missing ones are listed, and a module that could not be
// loaded for want of one names the command that installs them instead of Node's "Cannot find package".
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { missingPackages, packageError } from "../../cli/packages.mjs";

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-packages-test-"));
try {
	fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify({ dependencies: { present: "^1.0.0", absent: "^1.0.0" } }));
	fs.mkdirSync(path.join(dir, "node_modules", "present"), { recursive: true });
	fs.writeFileSync(path.join(dir, "node_modules", "present", "package.json"), "{}");
	assert.deepEqual(missingPackages(dir), ["absent"]);

	const notFound = Object.assign(new Error("Cannot find package 'absent' imported from /somewhere/module.mjs"), { code: "ERR_MODULE_NOT_FOUND" });
	const explained = packageError(notFound, dir);
	assert.match(explained.message, /npm packages are not installed \(absent\): run `npm install` in /);
	assert.ok(explained.message.includes(dir));
	assert.equal(explained.cause, notFound);

	// Another failure, or a missing module while every package is installed, is not rewritten
	const other = new Error("unexpected token");
	assert.equal(packageError(other, dir), other);
	fs.mkdirSync(path.join(dir, "node_modules", "absent"), { recursive: true });
	fs.writeFileSync(path.join(dir, "node_modules", "absent", "package.json"), "{}");
	assert.deepEqual(missingPackages(dir), []);
	assert.equal(packageError(notFound, dir), notFound);

	// No package.json: nothing to report
	assert.deepEqual(missingPackages(path.join(dir, "nowhere")), []);
	// This checkout's own packages are installed wherever the tests run
	assert.deepEqual(missingPackages(), []);
} finally {
	fs.rmSync(dir, { recursive: true, force: true });
}
console.log("npm packages: PASS (missing dependencies are listed; a module that needs one says to run npm install)");
