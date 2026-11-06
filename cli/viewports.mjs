// A persistent 3D worker process owns the 3D renderer's GPU and asset caches (docs/viewports.md). Each
// content-addressed snapshot becomes an ordinary transparent image in rur's asset store (desktop and
// headless share this).
import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import readline from "node:readline";
import { createHash } from "node:crypto";
import { ROOT, stateDir } from "./paths.mjs";

function executable() {
	if (process.env.RUR_VIEWPORT_WORKER) return path.resolve(process.env.RUR_VIEWPORT_WORKER);
	const sibling = path.resolve(ROOT, "../roblox-object-renderer/target/release", process.platform === "win32" ? "ror.exe" : "ror");
	if (fs.existsSync(sibling)) return sibling;
	throw new Error("ViewportFrames need a 3D viewport worker: set RUR_VIEWPORT_WORKER to an executable with a viewport-worker command (docs/viewports.md), or build the roblox-object-renderer checkout beside this one.");
}

// One-shot renders (the render pipeline's resolver asks with `persist`) keep each snapshot's image under
// its content key and the worker build, so a new process, such as a restarted render service, does not start
// the worker to draw a snapshot it has drawn before. The newest PERSISTED images are kept.
const PERSISTED = 512;

function workerStamp() {
	const file = executable();
	const stat = fs.statSync(file);
	return `${file}\0${stat.size}\0${stat.mtimeMs}`;
}

function prunePersisted(directory) {
	const names = fs.readdirSync(directory).filter(name => name.endsWith(".png"));
	if (names.length <= PERSISTED) return;
	const entries = names.map(name => {
		try {
			return { name, time: fs.statSync(path.join(directory, name)).mtimeMs };
		} catch {
			return { name, time: 0 };
		}
	}).sort((a, b) => a.time - b.time);
	for (const entry of entries.slice(0, entries.length - PERSISTED)) {
		fs.rmSync(path.join(directory, entry.name), { force: true });
	}
}

export class ViewportWorker {
	constructor({ offline = false } = {}) {
		this.offline = offline;
		this.child = null;
		this.pending = null;
		this.snapshots = new Map();
		this.directory = null;
	}
	close() {
		this.child?.stdin.end();
		this.child?.kill();
		this.child = null;
		this.fail(new Error("Viewport renderer closed"));
		if (this.directory) {
			try { fs.rmSync(this.directory, { recursive: true, force: true, maxRetries: 3, retryDelay: 20 }); } catch { /* a killed GPU worker may still hold its last file */ }
		}
		this.directory = null;
		this.snapshots.clear();
	}
	fail(error) {
		if (this.pending) {
			clearTimeout(this.pending.timer);
			this.pending.reject(error);
			this.pending = null;
		}
	}
	retain(slots) {
		const active = new Set(slots);
		for (const slot of this.snapshots.keys()) {
			if (!active.has(slot)) {
				fs.rmSync(path.join(this.directory, `viewport-${slot}.png`), { force: true });
				this.snapshots.delete(slot);
			}
		}
	}
	start() {
		if (this.child) return;
		const child = spawn(executable(), ["viewport-worker", ...(this.offline ? ["--offline"] : [])],
			{ stdio: ["pipe", "pipe", "ignore"], windowsHide: true });
		this.child = child;
		const lines = readline.createInterface({ input: child.stdout });
		lines.on("line", line => {
			const pending = this.pending;
			if (!pending) return;
			this.pending = null;
			clearTimeout(pending.timer);
			try {
				const reply = JSON.parse(line);
				if (reply.error) throw new Error(reply.error);
				pending.resolve(reply);
			} catch (error) { pending.reject(error); }
		});
		child.on("error", error => this.fail(error));
		child.on("exit", () => {
			if (this.child === child) {
				this.child = null;
				this.fail(new Error("the viewport worker exited (does it have a viewport-worker command? docs/viewports.md)"));
			}
			lines.close();
		});
		child.stdin.on("error", error => this.fail(error));
	}
	async render(uri, viewport, { persist = false } = {}) {
		const match = /^rur-viewport:\/\/([0-9]+)\/([a-f0-9]{64})$/.exec(uri);
		if (!match || !viewport) throw new Error("Missing ViewportFrame snapshot");
		const [, slot, key] = match;
		let persisted = null;
		if (persist) {
			this.stamp ??= workerStamp();
			const directory = path.join(stateDir(), "viewports-persisted");
			persisted = path.join(directory, `${createHash("sha256").update(`${key}\0${this.stamp}`).digest("hex")}.png`);
			if (fs.existsSync(persisted)) {
				const now = new Date();
				try { fs.utimesSync(persisted, now, now); } catch { /* another process is replacing it */ }
				return { file: persisted };
			}
		}
		if (!this.directory) {
			const base = path.join(stateDir(), "viewports-live");
			fs.mkdirSync(base, { recursive: true });
			this.directory = fs.mkdtempSync(path.join(base, "worker-"));
		}
		const file = path.join(this.directory, `viewport-${slot}.png`);
		let result = { file };
		if (this.snapshots.get(slot) !== key || !fs.existsSync(file)) {
			result = await this.request({ viewport, out: file });
			this.snapshots.set(slot, key);
		}
		if (!persisted) return result;
		// Written beside its name and renamed, so a reader never sees half of it
		fs.mkdirSync(path.dirname(persisted), { recursive: true });
		const partial = `${persisted}.${process.pid}.tmp`;
		fs.copyFileSync(file, partial);
		fs.renameSync(partial, persisted);
		prunePersisted(path.dirname(persisted));
		return { ...result, file: persisted };
	}
	async model(request) {
		if (request.command === "catalog") return this.request(request);
		const directory = path.join(stateDir(), "model-bundles");
		fs.mkdirSync(directory, { recursive: true });
		const key = createHash("sha256").update(JSON.stringify([request.av, request.path])).digest("hex");
		// The worker checks the manifest's pinned version on every new session; this filename is replaced
		// atomically, because a previous render may have hard-linked it into its output bundle.
		return this.request({ ...request, out: path.join(directory, `${key}.rbxm`) });
	}
	request(request) {
		this.start();
		if (this.pending) throw new Error("Viewport requests must be serialized");
		return new Promise((resolve, reject) => {
			const timer = setTimeout(() => {
				this.fail(new Error("viewport render timed out"));
				this.close();
			}, 90_000);
			this.pending = { resolve, reject, timer };
			this.child.stdin.write(JSON.stringify(request) + "\n");
		});
	}
}
