// Runs a command while holding the Studio lock, e.g.
//   node tools/studio-lock.mjs node tools/studio-mcp.cjs luau tools/studio/probe.luau
// Use it for anything that mounts GUI in Studio or captures its viewport outside `rur compare`.

import { spawnSync } from "node:child_process";
import { withStudioLock } from "../cli/lock.mjs";

const [command, ...args] = process.argv.slice(2).filter(arg => arg !== "--");
if (!command) {
	process.stderr.write("usage: node tools/studio-lock.mjs <command> [args...]\n");
	process.exit(2);
}
const status = await withStudioLock(`${command} ${args.join(" ")}`.slice(0, 200), async () => {
	const result = spawnSync(command, args, { stdio: "inherit", shell: process.platform === "win32" });
	return result.status ?? 1;
});
process.exit(status);
