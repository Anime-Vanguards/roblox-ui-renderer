// A `rur compare` report is read only with the engine that drew it, and a capture in the other text state
// than rur's text mode is refused (cli/compare.mjs readReport, checkTextState).
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { checkTextState, engineIdentity, readReport } from "../../cli/compare.mjs";

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-compare-reports-"));
try {
	const engine = path.join(dir, "engine.exe");
	fs.writeFileSync(engine, "one build");
	const out = path.join(dir, "fixture");
	fs.mkdirSync(out);
	assert.throws(() => readReport(out, engine), /no report/);
	// A report from before reports named their engine
	fs.writeFileSync(path.join(out, "report.json"), JSON.stringify({ pixels: { over2: 521 } }));
	assert.throws(() => readReport(out, engine), /does not say which engine/);
	// This engine's report reads
	const identity = engineIdentity(engine);
	assert.equal(identity.sha256.length, 64);
	fs.writeFileSync(path.join(out, "report.json"), JSON.stringify({ engine: identity, pixels: { over2: 0 } }));
	assert.equal(readReport(out, engine).pixels.over2, 0);
	// The engine rebuilt since: the report is an error, not a number
	fs.writeFileSync(engine, "another build");
	assert.throws(() => readReport(out, engine), /another engine/);

	// Text state: Studio's flag against rur's mode
	assert.doesNotThrow(() => checkTextState("false", false));
	assert.doesNotThrow(() => checkTextState("true", true));
	assert.doesNotThrow(() => checkTextState("unknown", false));
	assert.throws(() => checkTextState("true", false), /--text-subpixel on/);
	assert.throws(() => checkTextState("false", true), /--text-subpixel off/);
} finally {
	fs.rmSync(dir, { recursive: true, force: true });
}
console.log("compare reports: ok");
