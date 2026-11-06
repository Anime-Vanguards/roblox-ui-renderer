import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";

const MALFORMED_GRACE_MS = 60_000;

// The kernel lease survives neither process exit nor crashes. The file is only
// metadata, so reclaiming a stale file cannot remove another builder's lease.
export async function acquireBuildLock(filename) {
	fs.mkdirSync(path.dirname(filename), { recursive: true });
	let canonical = path.join(fs.realpathSync(path.dirname(filename)), path.basename(filename));
	if (process.platform === "win32") canonical = canonical.toLowerCase();
	const hash = createHash("sha256").update(canonical).digest("hex");
	const address = process.platform === "win32"
		? { path: `\\\\.\\pipe\\rur-build-${hash}` }
		: { host: "127.0.0.1", port: 1024 + (Number.parseInt(hash.slice(0, 8), 16) % 55_000), exclusive: true };
	const server = net.createServer(socket => socket.destroy());
	try {
		await new Promise((resolve, reject) => {
			server.once("error", reject);
			server.listen(address, () => { server.removeListener("error", reject); resolve(); });
		});
	} catch (cause) {
		throw new Error(`Another native build owns ${filename} (or its local build lease is occupied)`, { cause });
	}
	const close = () => new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
	try {
		if (fs.existsSync(filename)) {
			const stat = fs.statSync(filename);
			let previous;
			try { previous = JSON.parse(fs.readFileSync(filename, "utf8")); } catch { /* interrupted metadata write */ }
			if (Number.isSafeInteger(previous?.pid) && previous.pid > 0) {
				let alive = true;
				try { process.kill(previous.pid, 0); } catch (error) { if (error.code === "ESRCH") alive = false; }
				if (alive) throw new Error(`Another native build owns ${filename} (PID ${previous.pid})`);
			} else if (Date.now() - stat.mtimeMs < MALFORMED_GRACE_MS) {
				throw new Error(`Native build metadata ${filename} is incomplete; retry after one minute`);
			}
		}
		const token = randomUUID();
		fs.writeFileSync(filename, JSON.stringify({ pid: process.pid, token }));
		let released = false;
		return async () => {
			if (released) return;
			released = true;
			try {
				let owner;
				try { owner = JSON.parse(fs.readFileSync(filename, "utf8")); } catch { /* preserve replaced metadata */ }
				if (owner?.token === token) fs.unlinkSync(filename);
			} finally { await close(); }
		};
	} catch (error) {
		await close();
		throw error;
	}
}
