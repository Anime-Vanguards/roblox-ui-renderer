// Cross-process lock for the shared Roblox Studio session. Studio has one viewport, so anything that
// mounts GUI into CoreGui and captures it must run exclusively; parallel agents queue here.

// Studio is one per machine, so the lock is too: RUR_STUDIO_LOCK, else <temp>/rur/studio.lock, shared by
// every renderer checkout and state dir and written into none of them. A holder of the lock file
// older versions kept in the checkout (.cache/studio.lock) is waited for as well (read only).

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { DEFAULT_STATE_DIR } from "./paths.mjs";

export const LOCK_FILE = path.resolve(process.env.RUR_STUDIO_LOCK || path.join(os.tmpdir(), "rur", "studio.lock"));
const LEGACY_LOCK_FILE = path.join(DEFAULT_STATE_DIR, "studio.lock");
const STALE_MS = 10 * 60 * 1000;
const WAIT_MS = 45 * 60 * 1000;

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

function processAlive(pid) {
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		return error.code === "EPERM";
	}
}

function readHolder(file) {
	try {
		return JSON.parse(fs.readFileSync(file, "utf8"));
	} catch {
		return null;
	}
}

const live = holder => !!holder && processAlive(holder.pid) && Date.now() - holder.since <= STALE_MS;

// { file, pid, owner, since } of whoever holds the Studio lock, or null (rur doctor)
export function studioLockHolder() {
	for (const file of [LOCK_FILE, LEGACY_LOCK_FILE]) {
		const holder = readHolder(file);
		if (live(holder)) return { file, ...holder };
	}
	return null;
}

function tryAcquire(owner) {
	if (path.resolve(LEGACY_LOCK_FILE) !== LOCK_FILE && live(readHolder(LEGACY_LOCK_FILE))) return false;
	fs.mkdirSync(path.dirname(LOCK_FILE), { recursive: true });
	try {
		const handle = fs.openSync(LOCK_FILE, "wx");
		fs.writeSync(handle, JSON.stringify({ pid: process.pid, owner, since: Date.now() }));
		fs.closeSync(handle);
		return true;
	} catch (error) {
		if (error.code !== "EEXIST") throw error;
	}

	// Steal locks left behind by dead or hung holders
	let holder = null;
	try {
		holder = JSON.parse(fs.readFileSync(LOCK_FILE, "utf8"));
	} catch {
		holder = null;
	}
	const stale = !holder || !processAlive(holder.pid) || Date.now() - holder.since > STALE_MS;
	if (stale) {
		fs.rmSync(LOCK_FILE, { force: true });
	}
	return false;
}

export async function acquireStudioLock(owner = "rur") {
	const started = Date.now();
	let announced = false;
	while (!tryAcquire(owner)) {
		if (Date.now() - started > WAIT_MS) throw new Error("timed out waiting for the Studio lock");
		if (!announced) {
			process.stderr.write(`rur: waiting for the Studio lock (${LOCK_FILE})\n`);
			announced = true;
		}
		await sleep(1000);
	}
	return () => {
		try {
			const holder = JSON.parse(fs.readFileSync(LOCK_FILE, "utf8"));
			if (holder.pid === process.pid) fs.rmSync(LOCK_FILE, { force: true });
		} catch {
			// already released
		}
	};
}

export async function withStudioLock(owner, fn) {
	const release = await acquireStudioLock(owner);
	try {
		return await fn();
	} finally {
		release();
	}
}
