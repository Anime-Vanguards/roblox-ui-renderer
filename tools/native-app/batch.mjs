// Saved scenes to PNG with the native renderer: `rur-native --batch` (native/src/headless.rs) in a few
// processes at once, each over a consecutive slice of the scenes (one GPU device and asset store per
// process). A scene is drawn with the manifest.json beside it (its images and glyph atlas).
//
//   const results = await renderScenes([{ scene: ".../scene.json", out: ".../dir" }, ...], { jobs })
//   → [{ scene, out, ok, error?, width, height, fnv1a }] in the given order; out/render.png per scene
//
// fnv1a is the FNV-1a hash of the frame's RGBA bytes (tools/regress/renderer.mjs compares them).

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { ROOT } from "./config.mjs";
import { ensureEngine } from "../../cli/engine.mjs";

export const defaultJobs = () => Math.max(1, Number(process.env.RUR_JOBS) || Math.min(4, Math.max(1, Math.floor(os.availableParallelism() / 4))));

export async function renderScenes(targets, { jobs = defaultJobs(), executable, work = path.join(ROOT, "out", "native-batch"), log = null } = {}) {
	if (!targets.length) return [];
	executable ||= await ensureEngine();
	const count = Math.max(1, Math.min(jobs, targets.length));
	const slices = Array.from({ length: count }, (_, k) => targets.slice(Math.floor(k * targets.length / count), Math.floor((k + 1) * targets.length / count)));
	const stamp = `${process.pid}-${Date.now()}`;
	const runSlice = (slice, k) => new Promise((resolve, reject) => {
		const dir = path.join(work, `${stamp}-${k}`);
		fs.mkdirSync(dir, { recursive: true });
		const batch = path.join(dir, "captures.json");
		fs.writeFileSync(batch, JSON.stringify(slice.map(target => ({ scene: path.resolve(target.scene), out: path.resolve(target.out) }))));
		const child = spawn(executable, ["--repo", ROOT, "--batch", batch, "--out", dir, "--offline"], { cwd: ROOT, windowsHide: true });
		let output = "";
		child.stdout.on("data", chunk => { output += chunk; });
		child.stderr.on("data", chunk => {
			output += chunk;
			if (log) log(String(chunk));
		});
		child.on("error", reject);
		child.on("close", status => {
			const report = path.join(dir, "batch.json");
			let captures = [];
			try {
				captures = JSON.parse(fs.readFileSync(report, "utf8"));
			} catch {
				captures = slice.map(target => ({ scene: target.scene, out: target.out, ok: false, error: `batch process exited ${status}: ${output.trim().split(/\r?\n/).slice(-3).join(" | ")}` }));
			}
			fs.rmSync(dir, { recursive: true, force: true });
			resolve(captures);
		});
	});
	const runs = await Promise.all(slices.map(runSlice));
	return runs.flat().map((capture, index) => ({ ...capture, scene: targets[index].scene, out: targets[index].out }));
}
