#!/usr/bin/env node
// Starts Roblox Studio for parity work without taking the user's keyboard focus (StudioLaunch.cs has what was
// measured). Every Studio rur's tooling uses starts here.
//
//   node tools/studio/launch.mjs --place <file.rbxl|.rbxlx> [--version 0.747.0.7471481 | --exe <RobloxStudioBeta.exe>]
//       [--scale 1] [--on-screen] [--guard-seconds 45]
//   node tools/studio/launch.mjs --close <pid>      stop that Studio (no save prompt)
//   node tools/studio/launch.mjs --park <pid>       move Studio's drawn cursor to the viewport's corner again
//
// The window opens without activation, is moved above the primary monitor with only a 4 px strip on screen (at the
// monitor's display scale; a window entirely off screen is not composed and captures come out stale) and a guard puts back any window of it that takes the foreground
// for --guard-seconds. --scale 1 starts it at display scale 1 whatever the monitor (Qt's per-process scale
// variables; moving a running Studio between monitors of different scales breaks its viewport size). --on-screen
// leaves the window where Studio puts it (still opened without activation and guarded).
//
// Prints the process id; close it with --close when the batch is done.
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const args = {};
for (let i = 2; i < process.argv.length; i++) {
	const key = process.argv[i].replace(/^--/, "");
	args[key] = process.argv[i + 1] && !process.argv[i + 1].startsWith("--") ? process.argv[++i] : true;
}
const powershell = (parameters, options = {}) => spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", ...parameters], { encoding: "utf8", windowsHide: true, ...options });

function fileVersion(file) {
	const result = powershell(["-Command", `(Get-Item -LiteralPath '${file.replace(/'/g, "''")}').VersionInfo.FileVersion`]);
	return String(result.stdout || "").trim().replace(/,\s*/g, ".");
}

// The installed Studio builds: Roblox's own install (and here the sim's downloaded builds)
function findStudio(version) {
	const local = process.env.LOCALAPPDATA || "";
	const roots = [path.join(local, "Roblox", "Versions")];
	const found = [];
	for (const root of roots) {
		for (const entry of fs.existsSync(root) ? fs.readdirSync(root) : []) {
			const exe = path.join(root, entry, "RobloxStudioBeta.exe");
			if (!fs.existsSync(exe)) continue;
			const v = fileVersion(exe);
			found.push(`${v} (${exe})`);
			if (v === version) return exe;
		}
	}
	throw new Error(`no Studio ${version} installed; found:\n  ${found.join("\n  ") || "none"}\npass --exe <RobloxStudioBeta.exe> or another --version`);
}

function isStudio(pid) {
	const result = powershell(["-Command", `(Get-Process -Id ${Number(pid)} -ErrorAction SilentlyContinue).ProcessName`]);
	return String(result.stdout || "").trim() === "RobloxStudioBeta";
}

if (args.close) {
	const pid = Number(args.close);
	if (!isStudio(pid)) {
		console.error(`process ${pid} is not a Roblox Studio`);
		process.exit(1);
	}
	// Stop-Process ends it without the save prompt a closed window would show (and focus)
	powershell(["-Command", `Stop-Process -Id ${pid} -Force`]);
	console.log(`stopped Studio ${pid}`);
	process.exit(0);
}
if (args.park) {
	const result = powershell(["-File", path.join(HERE, "launch.ps1"), "-ParkPid", String(Number(args.park))]);
	process.stdout.write(result.stdout);
	process.stderr.write(result.stderr);
	process.exit(result.status);
}
if (!args.place) {
	console.error("usage: launch.mjs --place <file> [--version V | --exe <path>] [--scale 1] [--on-screen] [--guard-seconds S] | --close <pid> | --park <pid>");
	process.exit(2);
}
const place = path.resolve(String(args.place));
if (!fs.existsSync(place)) throw new Error(`no place file ${place}`);
const exe = args.exe ? path.resolve(String(args.exe)) : findStudio(String(args.version || "0.747.0.7471481"));
const guardMs = Math.round(Number(args["guard-seconds"] || 45) * 1000);
const parameters = ["-File", path.join(HERE, "launch.ps1"), "-Exe", exe, "-Arguments", `-task EditFile -localPlaceFile "${place}"`, "-GuardMs", String(guardMs)];
if (args["on-screen"]) parameters.push("-OnScreen");
if (String(args.scale || "") === "1") parameters.push("-Scale1", "-Width", "2200", "-Height", "1300");
const result = powershell(parameters, { timeout: guardMs + 60000 });
const output = `${result.stdout || ""}${result.stderr || ""}`;
const pid = /pid=(\d+)/.exec(output);
const stolen = /STOLEN/.test(output);
console.log(output.trim());
if (!pid) {
	console.error("Studio did not start");
	process.exit(1);
}
console.log(`\nStudio ${fileVersion(exe)} started: pid ${pid[1]}${stolen ? " (it took the foreground and was put back)" : ", the foreground was never taken"}`);
console.log(`close it with: node tools/studio/launch.mjs --close ${pid[1]}`);
