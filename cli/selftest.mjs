// Deterministic regression checks for engine output, the CLI, the native build lock and assets. The
// engine's and renderer's own unit tests (ports of the former Luau runtime tests, the renderer's frozen
// record and pixel goldens, scroll physics against Studio recordings) run with
// `cd native; cargo test --release`; renderer pixels are checked by tools/regress/renderer.mjs.
import path from "node:path";
import os from "node:os";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { ensureEngine } from "./engine.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

export async function selftest() {
	// One engine build for every check (a stale build is rebuilt here, not inside a timed check)
	process.env.RUR_NATIVE_ENGINE = await ensureEngine();
	const checks = [
		// The engine against its golden output (fixtures, probes and live sessions; tools/engine/golden.mjs)
		[process.execPath, ["tools/engine/golden.mjs", "--fast"]],
		[process.execPath, ["tools/tests/cli.mjs"]],
		[process.execPath, ["tools/tests/kept-runs.mjs"]],
		[process.execPath, ["tools/tests/native-build-lock.mjs"]],
		[process.execPath, ["tools/tests/native-assets.mjs"]],
		[process.execPath, ["tools/analysis/image_assets_test.mjs"]],
		[process.execPath, ["tools/tests/asset_cookie.mjs"]],
		[process.execPath, ["tools/tests/asset_records.mjs"]],
		[process.execPath, ["tools/tests/asset_batch.mjs"]],
		[process.execPath, ["tools/tests/asset_studio.mjs"]],
		[process.execPath, ["tools/tests/placeholders.mjs"]],
		[process.execPath, ["tools/tests/web-bundle-directories.mjs"]],
		[process.execPath, ["tools/tests/places.mjs"]],
		[process.execPath, ["tools/tests/examples.mjs"]],
	];
	// The checks are independent (each writes its own out/ directory): they run concurrently and each
	// one's output is printed whole, in the list's order
	const jobs = Math.max(1, Number(process.env.RUR_JOBS) || Math.min(8, Math.max(2, Math.floor(os.availableParallelism() / 4))));
	const run = ([command, args]) => new Promise(resolve => {
		const child = spawn(command, args, { cwd: ROOT, windowsHide: true });
		let output = "";
		child.stdout.on("data", chunk => { output += chunk; });
		child.stderr.on("data", chunk => { output += chunk; });
		const timer = setTimeout(() => child.kill(), 120000);
		child.on("error", error => { clearTimeout(timer); resolve({ output, error }); });
		child.on("close", (status, signal) => { clearTimeout(timer); resolve({ output, status, signal }); });
	});
	const results = new Array(checks.length);
	let next = 0, printed = 0, failed = 0;
	const flush = () => {
		while (printed < checks.length && results[printed]) {
			const { output, error, status, signal } = results[printed];
			if (output) process.stdout.write(output.endsWith("\n") ? output : `${output}\n`);
			if (error || status !== 0) {
				failed++;
				console.error(`FAIL ${checks[printed][1].join(" ")}: ${error?.message || `exit ${status ?? signal}`}`);
			}
			printed++;
		}
	};
	await Promise.all(Array.from({ length: Math.min(jobs, checks.length) }, async () => {
		while (next < checks.length) {
			const index = next++;
			results[index] = await run(checks[index]);
			flush();
		}
	}));
	flush();
	console.log(`selftest: ${checks.length - failed}/${checks.length} checks passed`);
	return failed ? 1 : 0;
}
