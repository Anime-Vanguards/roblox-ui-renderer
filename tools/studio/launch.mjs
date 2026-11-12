#!/usr/bin/env node
// Starts Roblox Studio for parity work where nobody sees it: on a Windows desktop of its own, so none of its windows
// (splash, main window, dialogs) is ever on a monitor or takes the keyboard focus. Every Studio rur's tooling uses
// starts here. StudioLaunch.cs has what was measured.
//
//   node tools/studio/launch.mjs --place <file.rbxl|.rbxlx> [--version 0.747.0.7471481 | --exe <RobloxStudioBeta.exe>]
//       [--scale 1] [--new] [--user-desktop] [--guard-seconds 45]
//   node tools/studio/launch.mjs --list             the Studios started here that still run
//   node tools/studio/launch.mjs --close <pid>      stop that Studio (no save prompt)
//
// A Studio of the same build, place and scale that this launcher started and that still runs is reused (its pid is
// printed and nothing starts): keep one per task and close it when the task ends. --new starts another.
// Studio renders on its desktop, and `rur compare` captures it through the engine (cli/studio.mjs captureEngine),
// mounted under StarterGui. --scale 1 starts it at display scale 1 (Qt's per-process scale variables); otherwise
// it has the primary monitor's scale.
//
// --user-desktop starts it on the user's desktop instead (for PrintWindow captures): minimized and inactive, every
// window parked above the primary monitor with a 4 px strip on screen as it appears, the foreground handed back
// for --guard-seconds. The splash still shows on a monitor for about 10 s there: Studio's busy UI thread applies no
// move from outside until it has loaded.
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { launchedStudios, writeLaunchedStudios, listStudios, STUDIO_DESKTOP } from "../../cli/studio.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const args = {};
for (let i = 2; i < process.argv.length; i++) {
	const key = process.argv[i].replace(/^--/, "");
	args[key] = process.argv[i + 1] && !process.argv[i + 1].startsWith("--") ? process.argv[++i] : true;
}
const powershell = (parameters, options = {}) => spawnSync("powershell", ["-NoProfile", "-ExecutionPolicy", "Bypass", ...parameters], { encoding: "utf8", windowsHide: true, ...options });
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

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

if (args.list) {
	const studios = launchedStudios();
	console.log(studios.length ? studios.map(studio => `pid ${studio.pid}  ${studio.version}  scale ${studio.scale}  ${studio.desktop ? `desktop ${studio.desktop}` : "user desktop"}  ${studio.place}`).join("\n") : "no Studio started by the launcher is running");
	process.exit(0);
}
if (args.close) {
	const pid = Number(args.close);
	if (!isStudio(pid)) {
		console.error(`process ${pid} is not a Roblox Studio`);
		process.exit(1);
	}
	// Stop-Process ends it without the save prompt a closed window would show
	powershell(["-Command", `Stop-Process -Id ${pid} -Force`]);
	writeLaunchedStudios(launchedStudios().filter(studio => studio.pid !== pid));
	console.log(`stopped Studio ${pid}`);
	process.exit(0);
}
if (!args.place) {
	console.error("usage: launch.mjs --place <file> [--version V | --exe <path>] [--scale 1] [--new] [--user-desktop] [--guard-seconds S] | --list | --close <pid>");
	process.exit(2);
}
const place = path.resolve(String(args.place));
if (!fs.existsSync(place)) throw new Error(`no place file ${place}`);
const exe = args.exe ? path.resolve(String(args.exe)) : findStudio(String(args.version || "0.747.0.7471481"));
const scale = String(args.scale || "") === "1" ? 1 : "monitor";
const desktop = args["user-desktop"] ? "" : STUDIO_DESKTOP;
const placeName = path.basename(place);

const running = launchedStudios().find(studio => studio.exe === exe && studio.place === place && studio.scale === scale && studio.desktop === desktop);
if (running && !args.new) {
	console.log(`reusing Studio ${running.version}: pid ${running.pid} (started ${running.startedAt}); --studio ${placeName}`);
	console.log(`close it with: node tools/studio/launch.mjs --close ${running.pid}`);
	process.exit(0);
}

const guardMs = Math.round(Number(args["guard-seconds"] || 45) * 1000);
const parameters = ["-File", path.join(HERE, "launch.ps1"), "-Exe", exe, "-Arguments", `-task EditFile -localPlaceFile "${place}"`, "-GuardMs", String(guardMs)];
if (scale === 1) parameters.push("-Scale1");
if (desktop) parameters.push("-Desktop", desktop);
const result = powershell(parameters, { timeout: guardMs + 60000 });
const output = `${result.stdout || ""}${result.stderr || ""}`;
const pid = /pid=(\d+)/.exec(output);
if (!pid) {
	console.error(output.trim());
	console.error("Studio did not start");
	process.exit(1);
}
const version = fileVersion(exe);
writeLaunchedStudios([...launchedStudios(), { pid: Number(pid[1]), exe, version, place, scale, desktop, startedAt: new Date().toISOString() }]);
if (desktop) {
	// Ready when the Studio MCP bridge lists it (skipped when there is no bridge)
	let ready = false;
	for (let attempt = 0; attempt < 60 && !ready; attempt++) {
		try {
			ready = (await listStudios()).some(studio => typeof studio.name === "string" && studio.name.includes(placeName));
		} catch {
			break;
		}
		if (!ready) await sleep(2000);
	}
	// The window Studio opens there can be small (1302x947 at scale 1, a 794x607 viewport): give it the desktop's size
	if (ready) {
		const resize = path.join(HERE, "..", "wincap", "resize.ps1");
		const size = /size=(\d+)x(\d+)/.exec(powershell(["-File", resize, "-Title", placeName]).stdout || "");
		if (size) {
			const [w, h] = [Number(size[1]), Number(size[2])];
			const target = [3800, 2100];
			if (w < target[0] || h < target[1]) powershell(["-File", resize, "-Title", placeName, "-DeltaWidth", String(Math.max(0, target[0] - w)), "-DeltaHeight", String(Math.max(0, target[1] - h))]);
		}
	}
	console.log(`Studio ${version} started on desktop ${desktop}: pid ${pid[1]}${ready ? ", ready" : ""}; nothing of it is on a monitor`);
} else {
	console.log(output.trim());
	const stolen = /FOREGROUND TAKEN/.test(output), shown = /ON SCREEN/.test(output);
	console.log(`\nStudio ${version} started: pid ${pid[1]}${stolen ? "; it took the foreground and gave it back" : "; the foreground was never taken"}${shown ? "; a window reached a monitor before it was parked" : "; no window was on a monitor"}`);
}
console.log(`use it with --studio ${placeName}; close it with: node tools/studio/launch.mjs --close ${pid[1]}`);
