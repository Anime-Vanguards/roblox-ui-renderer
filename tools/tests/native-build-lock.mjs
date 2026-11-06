import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { fileURLToPath } from "node:url";
import { acquireBuildLock } from "../native-host/build-lock.mjs";

if (process.argv[2] === "--hold") {
	await acquireBuildLock(process.argv[3]);
	process.stdout.write("ready\n");
} else {
	const directory = fs.mkdtempSync(path.join(os.tmpdir(), "rur-build-lock-"));
	const filename = path.join(directory, "build.lock");
	let child;
	try {
		let release = await acquireBuildLock(filename);
		await assert.rejects(acquireBuildLock(filename), /Another native build/);
		await release();
		await release();
		assert.equal(fs.existsSync(filename), false);

		fs.writeFileSync(filename, "");
		await assert.rejects(acquireBuildLock(filename), /incomplete/);
		assert.equal(fs.readFileSync(filename, "utf8"), "", "fresh incomplete metadata is preserved");
		const old = new Date(Date.now() - 120_000);
		fs.utimesSync(filename, old, old);
		release = await acquireBuildLock(filename);
		await release();

		fs.writeFileSync(filename, JSON.stringify({ pid: process.pid }));
		await assert.rejects(acquireBuildLock(filename), /PID/);
		fs.unlinkSync(filename);

		child = spawn(process.execPath, [fileURLToPath(import.meta.url), "--hold", filename], { stdio: ["ignore", "pipe", "pipe"], windowsHide: true });
		const ready = await Promise.race([
			once(child.stdout, "data").then(([data]) => data.toString()),
			once(child, "exit").then(([code]) => { throw new Error(`Lock child exited early: ${code}`); }),
		]);
		assert.match(ready, /ready/);
		await assert.rejects(acquireBuildLock(filename), /Another native build/);
		const exited = once(child, "exit");
		child.kill();
		await exited;
		child = null;
		assert.equal(fs.existsSync(filename), true, "crash leaves metadata");
		release = await acquireBuildLock(filename);
		await release();

		release = await acquireBuildLock(filename);
		fs.writeFileSync(filename, "replacement");
		await release();
		assert.equal(fs.readFileSync(filename, "utf8"), "replacement", "release preserves unowned metadata");
		console.log("native build locks: PASS (exclusive, fresh/aged malformed, live/dead PID, crash recovery, owned cleanup)");
	} finally {
		if (child) { const exited = once(child, "exit"); child.kill(); await exited; }
		fs.rmSync(directory, { recursive: true, force: true });
	}
}
