// Runs an engine-oracle script (tools/tests/*.luau: a fixture using the `rur` test hooks of
// `rur-native engine --test-hooks 1`, native/src/engine/cli.rs) and returns the value it emitted
// with rur.emit(prefix, value).
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { engineBaseArgs, ensureEngine } from "../../cli/engine.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

export async function runOracle(script, prefix, { viewport = "600x500", scale = "1", rotationClipping = "enabled" } = {}) {
	const out = path.join(ROOT, "out", "selftest", `${path.basename(script, ".luau")}-${rotationClipping}.json`);
	const result = spawnSync(await ensureEngine(), [...engineBaseArgs(), "--test-hooks", "1", "--roots", path.join(ROOT, "tools", "tests", "oracle-root"),
		"--fixture", path.join(ROOT, script), "--viewport", viewport, "--scale", scale, "--time", "0", "--no-snapshots",
		"--rotation-clipping", rotationClipping, "--out", out], { cwd: ROOT, encoding: "utf8", timeout: 30000, maxBuffer: 256 * 1024 * 1024, windowsHide: true });
	if (result.status !== 0) throw new Error(`engine failed (${result.status}): ${result.stderr}`);
	const line = result.stdout.split("\n").find(value => value.startsWith(`${prefix} `));
	if (!line) {
		const scene = JSON.parse((await import("node:fs")).readFileSync(out, "utf8"));
		const errors = (scene.diagnostics?.output || []).filter(entry => entry.kind === "error").map(entry => entry.message);
		throw new Error(`${script} emitted no ${prefix} line: ${scene.status} ${scene.failure || ""} ${errors.join("\n")}`);
	}
	return JSON.parse(line.slice(prefix.length + 1));
}
