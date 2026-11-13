// A static file server for a story bundle (tools/web/bundle.mjs) or any page directory, on 127.0.0.1:
//
//   node tools/web/serve.mjs <dir> [--port 4600]
//
// Files are served as they are (a .gz file is application/gzip, not Content-Encoding: the page inflates
// it itself, as it must on claude.ai). Used by tools/web/frame-identity.mjs.
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { isMain } from "../../cli/entry.mjs";

const TYPES = {
	".html": "text/html; charset=utf-8",
	".js": "text/javascript; charset=utf-8",
	".mjs": "text/javascript; charset=utf-8",
	".json": "application/json",
	".wasm": "application/wasm",
	".gz": "application/gzip",
	".bin": "application/octet-stream",
	".png": "image/png",
	".css": "text/css; charset=utf-8",
	".svg": "image/svg+xml",
	".woff2": "font/woff2",
};

// Resolves with {server, port, url}
export function serve(root, { port = 0 } = {}) {
	root = path.resolve(root);
	const server = http.createServer((request, response) => {
		const url = new URL(request.url, "http://127.0.0.1");
		const file = path.resolve(root, `.${decodeURIComponent(url.pathname)}`);
		const inside = file === root || file.startsWith(root + path.sep);
		const target = inside && fs.existsSync(file) && fs.statSync(file).isDirectory() ? path.join(file, "index.html") : file;
		if (!inside || !fs.existsSync(target)) {
			response.writeHead(404, { "content-type": "text/plain" });
			response.end("not found");
			return;
		}
		const stat = fs.statSync(target);
		response.writeHead(200, { "content-type": TYPES[path.extname(target).toLowerCase()] || "application/octet-stream", "content-length": stat.size, "cache-control": "no-store" });
		fs.createReadStream(target).pipe(response);
	});
	return new Promise((resolve, reject) => {
		server.once("error", reject);
		server.listen(port, "127.0.0.1", () => {
			const actual = server.address().port;
			resolve({ server, port: actual, url: `http://127.0.0.1:${actual}/` });
		});
	});
}

if (isMain(import.meta.url)) {
	const args = process.argv.slice(2);
	const dir = args.find(arg => !arg.startsWith("--"));
	if (!dir) throw new Error("usage: node tools/web/serve.mjs <dir> [--port 4600]");
	const portAt = args.indexOf("--port");
	const { url } = await serve(dir, { port: portAt >= 0 ? Number(args[portAt + 1]) : 4600 });
	console.log(`serving ${path.resolve(dir)} at ${url}`);
}
