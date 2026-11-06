// A headless Chromium (Chrome or Edge) driven over the DevTools protocol, with no npm dependencies:
// launch it, open a page, evaluate expressions, close it. Headless: no window, nothing takes focus.
// RUR_BROWSER names the executable; otherwise the usual Chrome / Edge installs are tried.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { withoutCookie } from "../../cli/assets.mjs";

export function findBrowser() {
	const candidates = [process.env.RUR_BROWSER];
	if (process.platform === "win32") {
		for (const base of [process.env.PROGRAMFILES, process.env["PROGRAMFILES(X86)"], process.env.LOCALAPPDATA]) {
			if (!base) continue;
			candidates.push(path.join(base, "Google", "Chrome", "Application", "chrome.exe"), path.join(base, "Microsoft", "Edge", "Application", "msedge.exe"));
		}
	} else if (process.platform === "darwin") {
		candidates.push("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge");
	} else {
		candidates.push("/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser", "/usr/bin/microsoft-edge");
	}
	return candidates.find(file => file && fs.existsSync(file)) || null;
}

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

export async function launch({ executable = findBrowser(), args = [] } = {}) {
	if (!executable) throw new Error("no Chrome or Edge found (set RUR_BROWSER)");
	if (typeof WebSocket !== "function") throw new Error("Node 22+ is needed (global WebSocket)");
	const profile = fs.mkdtempSync(path.join(os.tmpdir(), "rur-browser-"));
	const child = spawn(executable, [
		"--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--no-default-browser-check",
		"--disable-extensions", "--disable-background-timer-throttling", "--disable-renderer-backgrounding", "--ignore-gpu-blocklist",
		...args, "about:blank",
	], { stdio: "ignore", windowsHide: true, env: withoutCookie() });
	const portFile = path.join(profile, "DevToolsActivePort");
	let port = null;
	for (let i = 0; i < 200 && !port; i++) {
		await sleep(50);
		if (fs.existsSync(portFile)) port = Number(fs.readFileSync(portFile, "utf8").split("\n")[0]) || null;
	}
	if (!port) {
		child.kill();
		throw new Error("the browser did not open its DevTools port");
	}
	const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
	const page = targets.find(target => target.type === "page");
	const socket = new WebSocket(page.webSocketDebuggerUrl);
	await new Promise((resolve, reject) => {
		socket.onopen = resolve;
		socket.onerror = () => reject(new Error("DevTools connection failed"));
	});
	let next = 0;
	const pending = new Map();
	const listeners = [];
	socket.onmessage = message => {
		const data = JSON.parse(message.data);
		if (data.id && pending.has(data.id)) {
			const { resolve, reject } = pending.get(data.id);
			pending.delete(data.id);
			if (data.error) reject(new Error(data.error.message));
			else resolve(data.result);
		} else if (data.method) {
			for (const listener of listeners) listener(data);
		}
	};
	const send = (method, params = {}) => new Promise((resolve, reject) => {
		const id = ++next;
		pending.set(id, { resolve, reject });
		socket.send(JSON.stringify({ id, method, params }));
	});
	const console = [];
	listeners.push(event => {
		if (event.method === "Runtime.consoleAPICalled") console.push(`${event.params.type}: ${event.params.args.map(arg => arg.value ?? arg.description ?? "").join(" ")}`);
		if (event.method === "Runtime.exceptionThrown") console.push(`exception: ${event.params.exceptionDetails.exception?.description || event.params.exceptionDetails.text}`);
	});
	await send("Runtime.enable");
	await send("Page.enable");
	return {
		console,
		send,
		async goto(url) {
			await send("Page.navigate", { url });
		},
		// Evaluates `expression` in the page (awaiting a promise); returns its JSON value
		async evaluate(expression, timeoutMs = 600000) {
			const result = await Promise.race([
				send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }),
				sleep(timeoutMs).then(() => {
					throw new Error(`evaluate timed out after ${timeoutMs} ms`);
				}),
			]);
			if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
			return result.result.value;
		},
		async waitFor(expression, timeoutMs = 300000) {
			const deadline = Date.now() + timeoutMs;
			for (;;) {
				const value = await this.evaluate(expression).catch(() => null);
				if (value) return value;
				if (Date.now() > deadline) throw new Error(`timed out waiting for ${expression}`);
				await sleep(100);
			}
		},
		async close() {
			try {
				await send("Browser.close");
			} catch {}
			socket.close();
			await sleep(300);
			try {
				child.kill();
			} catch {}
			try {
				fs.rmSync(profile, { recursive: true, force: true });
			} catch {}
		},
	};
}
