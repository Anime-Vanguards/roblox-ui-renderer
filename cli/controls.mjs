// A story's UI Labs controls, as the desktop app lists them: the story mounted in an engine session
// (`rur-native engine ... --session 1`, native/src/engine/session.rs) whose first reply carries them.
//
//   const { storyControls } = await import("<renderer>/cli/controls.mjs");
//   await storyControls({ story, place, project, viewport, scale })   → [{ path, kind, value, ... }]
//
// A tool that previews a story's states builds one per control value from these.

import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { ensureEngine } from "./engine.mjs";
import { runtimeArgs, snapshotDir } from "./rur.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PREFIX = "@@RUR ";

export async function storyControls(options, { timeoutMs = 45000 } = {}) {
	const engine = await ensureEngine();
	const args = { viewport: "1200x900", scale: "1.5", ...options, time: "0" };
	args["snapshot-dir"] = snapshotDir(args);
	const child = spawn(engine, [...runtimeArgs(args), "--session", "1"], { cwd: ROOT, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
	child.stderr.resume();
	child.stdin.on("error", () => {});
	return new Promise((resolve, reject) => {
		let buffer = "";
		const finish = (error, value) => {
			clearTimeout(timer);
			try {
				child.stdin.end(`${JSON.stringify({ cmd: "quit" })}\n`);
			} catch {
				// already gone
			}
			setTimeout(() => child.exitCode === null && child.kill(), 1500).unref();
			if (error) reject(error);
			else resolve(value);
		};
		const timer = setTimeout(() => finish(new Error("timed out")), timeoutMs);
		child.on("error", error => finish(error));
		child.on("close", code => finish(new Error(`the engine exited (${code}) before its first reply`)));
		child.stdout.on("data", chunk => {
			buffer += chunk;
			// Complete lines only (the reply holds the whole scene)
			const line = buffer.split(/\r?\n/).slice(0, -1).find(text => text.startsWith(PREFIX));
			if (!line) return;
			child.removeAllListeners("close");
			const reply = JSON.parse(line.slice(PREFIX.length));
			if (reply.type === "error") finish(new Error(reply.message || "the story failed to mount"));
			else finish(null, Array.isArray(reply.controls) ? reply.controls : []);
		});
	});
}
