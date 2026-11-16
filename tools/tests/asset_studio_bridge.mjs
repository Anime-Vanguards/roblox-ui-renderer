// The HTTP Studio bridge (cli/studio-images.mjs): robloxstudio-mcp 3 or later, found by its unauthenticated
// GET /health, is used through its own API with RUR_STUDIO_MCP_TOKEN in X-MCP-Auth; an older /health, or none,
// keeps the older API, which gets no token. A RUR_STUDIO_MCP_URL off this machine is refused before any
// request, a redirect is refused rather than followed, a missing or refused token fails with an error that
// names RUR_STUDIO_MCP_TOKEN, and no error quotes the token. The bridges are synthetic.
import assert from "node:assert/strict";
import http from "node:http";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const { readStudioImages, studioImageIds } = await import(pathToFileURL(path.join(root, "cli", "studio-images.mjs")).href);

const TOKEN = "5f0c9e2a7b3d4c1e8f6a0b9d2c7e4f1a3b8d6c0e9f2a5b7c1d4e8f0a3b6c9d2e";
const WRONG = "0d9c8b7a6f5e4d3c2b1a0f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c";
const UNSENDABLE = "line-one\nline-two";
// 2 x 1 pixels: opaque red, and green at alpha 0
const PIXELS = Buffer.from([255, 0, 0, 255, 0, 255, 0, 0]);
const IMAGE = { width: 2, height: 1, encoding: "raw", pixels: PIXELS.toString("base64") };

// Every request a bridge received, and every error text rur gave (none may quote a token)
const requests = [];
const errors = [];
const routes = () => requests.map(entry => entry.route);

// A request to a host other than 127.0.0.1 is recorded and fails at once instead of being sent
const realFetch = globalThis.fetch;
const offMachine = [];
globalThis.fetch = async (url, options) => {
	if (new URL(url).hostname !== "127.0.0.1") {
		offMachine.push(String(url));
		throw new Error("the test refused a request off the machine");
	}
	return realFetch(url, options);
};

function bridge(answer) {
	return http.createServer((request, response) => {
		let text = "";
		request.on("data", chunk => text += chunk);
		request.on("end", () => {
			const body = text ? JSON.parse(text) : undefined;
			requests.push({ route: `${request.method} ${request.url}`, token: request.headers["x-mcp-auth"], authorization: request.headers.authorization, body });
			const [status, value, headers = {}] = answer(request, body);
			response.writeHead(status, { "Content-Type": "application/json", ...headers });
			response.end(value === undefined ? "" : JSON.stringify(value));
		});
	});
}

// The place scan names image 31; any other Luau reads the image
const luauReply = body => ({ success: true, returnValue: JSON.stringify(body.code.includes("GetDescendants") ? ["31"] : [IMAGE]) });

// robloxstudio-mcp 3: /health without a token, the tools only with it (a missing or wrong one is 401). `rows`
// lists the sessions as that version does; `tools` can answer a tool instead
function v3(version, rows, tools = () => null) {
	return (request, body) => {
		if (request.method === "GET" && request.url === "/health") return [200, { status: "ok", service: "robloxstudio-mcp", version, serverVersion: version }];
		if (request.headers["x-mcp-auth"] !== TOKEN) return [401, { error: "unauthorized", message: "Missing or invalid auth token." }];
		const answer = tools(request, body);
		if (answer) return answer;
		if (request.method === "POST" && request.url === "/mcp/get_connected_instances") return [200, { instances: rows, multiplayerGroups: [] }];
		if (request.method === "POST" && request.url === "/mcp/execute_luau") return [200, luauReply(body)];
		return [404, { error: "not_found" }];
	};
}
const ROWS = [
	{ id: "instance:run-1", placeId: 1, placeName: "Test Place", peers: { server: "peer:run-1" } },
	{ id: "instance:edit-1", placeId: 1, placeName: "Test Place", peers: { edit: "peer:edit-1" } },
	{ id: "instance:edit-2", placeId: 2, placeName: "Other Place", peers: { edit: "peer:edit-2", server: "peer:edit-2s" } },
];

// The older API, with `health` the answer to GET /health
function legacy(health) {
	return (request, body) => {
		if (request.method === "GET" && request.url === "/health") return health;
		if (request.method === "GET" && request.url === "/instances") return [200, { instances: [{ instanceId: "edit-1", role: "edit", placeName: "Test Place" }] }];
		if (request.method === "POST" && request.url === "/mcp/execute_luau") return [200, { content: [{ type: "text", text: JSON.stringify(luauReply(body)) }] }];
		return [404, {}];
	};
}

// Where the redirecting bridges point: it must never be asked anything
let awayAsked = 0;
const away = bridge(() => {
	awayAsked++;
	return [200, { instances: [] }];
});
const redirect = request => [302, undefined, { Location: `http://127.0.0.1:${away.address().port}${request.url}` }];

const servers = {
	current: bridge(v3("3.5.0", ROWS)),
	first: bridge(v3("v3.0.0", [{ id: "instance-a", name: "Test Place", roles: ["server", "edit"] }])),
	older: bridge(legacy([200, { status: "ok", service: "robloxstudio-mcp", version: "2.23.1" }])),
	bare: bridge(legacy([404, {}])),
	redirecting: bridge(redirect),
	redirectingTools: bridge(v3("3.5.0", ROWS, redirect)),
	leakyLuau: bridge(v3("3.5.0", ROWS, request => request.url === "/mcp/execute_luau" && [200, { success: false, error: `Studio saw ${TOKEN} in the code` }])),
	leakyTool: bridge(v3("3.5.0", ROWS, request => request.url === "/mcp/get_connected_instances" && [500, { error: "tool_failed", message: `bad request from ${TOKEN}` }])),
	away,
};
const url = name => `http://127.0.0.1:${servers[name].address().port}`;

async function read(address, env = {}) {
	requests.length = 0;
	const result = await readStudioImages(["31"], { RUR_STUDIO_MCP_URL: address, ...env });
	errors.push(...result.errors.values());
	return result;
}
async function scanFails(address, env, pattern) {
	await assert.rejects(studioImageIds({ RUR_STUDIO_MCP_URL: address, ...env }), error => {
		errors.push(error.message);
		return pattern.test(error.message);
	});
}
const pixels = result => [...result.images.keys()];

try {
	await Promise.all(Object.values(servers).map(server => new Promise(resolve => server.listen(0, "127.0.0.1", resolve))));

	// robloxstudio-mcp 3: /health without the token, then its tools with it in X-MCP-Auth; only edit sessions
	let result = await read(url("current"), { RUR_STUDIO_MCP_TOKEN: TOKEN });
	assert.deepEqual(pixels(result), ["31"]);
	assert.deepEqual(routes(), ["GET /health", "POST /mcp/get_connected_instances", "POST /mcp/execute_luau"]);
	assert.deepEqual(requests.map(entry => entry.token), [undefined, TOKEN, TOKEN], "the token goes only to the tools");
	assert.ok(requests.every(entry => entry.authorization === undefined), "in one header only");
	assert.deepEqual([requests[2].body.instance_id, requests[2].body.target], ["instance:edit-1", "edit"], "the first edit session, in edit");
	result = await read(url("current"), { RUR_STUDIO_MCP_TOKEN: TOKEN, RUR_STUDIO: "Other" });
	assert.deepEqual(pixels(result), ["31"]);
	assert.equal(requests[2].body.instance_id, "instance:edit-2", "RUR_STUDIO picks the session");
	requests.length = 0;
	assert.deepEqual(await studioImageIds({ RUR_STUDIO_MCP_URL: url("current"), RUR_STUDIO_MCP_TOKEN: TOKEN }), { ids: ["31"], studios: ["Test Place", "Other Place"] });
	// 3.0 lists sessions as { id, name, roles }; the URL parser's 127.1 is 127.0.0.1; the token is trimmed
	result = await read(`http://127.1:${servers.first.address().port}/`, { RUR_STUDIO_MCP_TOKEN: ` ${TOKEN}\n` });
	assert.deepEqual(pixels(result), ["31"]);
	assert.equal(requests.at(-1).body.instance_id, "instance-a");

	// An older /health, or none: the older API, and no token on any request
	for (const name of ["older", "bare"]) {
		result = await read(url(name), { RUR_STUDIO_MCP_TOKEN: TOKEN });
		assert.deepEqual(pixels(result), ["31"], name);
		assert.deepEqual(routes(), ["GET /health", "GET /instances", "POST /mcp/execute_luau"], name);
		assert.ok(requests.every(entry => entry.token === undefined && entry.authorization === undefined), `${name} gets no token`);
		assert.equal(requests[2].body.sessionId, "edit-1");
	}

	// A bridge off this machine, or no http URL: refused, and nothing is sent
	for (const address of ["http://192.0.2.10:58741", "http://example.com:58741", "http://127.0.0.1.example.com:58741", "http://[::ffff:c000:20a]:58741", "ftp://127.0.0.1:58741", "127.0.0.1:58741"]) {
		offMachine.length = 0;
		result = await read(address, { RUR_STUDIO_MCP_TOKEN: TOKEN });
		assert.match(result.errors.get("31"), /no Studio to read it from \(RUR_STUDIO_MCP_URL must be an http:\/\/ or https:\/\/ address on this machine \(127\.0\.0\.1, ::1 or localhost\)/, address);
		await scanFails(address, { RUR_STUDIO_MCP_TOKEN: TOKEN }, /RUR_STUDIO_MCP_URL must be an http:\/\/ or https:\/\/ address on this machine/);
		assert.deepEqual(offMachine, [], `nothing is sent to ${address}`);
	}

	// robloxstudio-mcp 3 without a token, or with one a header cannot carry: only /health is asked
	for (const env of [{}, { RUR_STUDIO_MCP_TOKEN: "  " }]) {
		result = await read(url("current"), env);
		assert.match(result.errors.get("31"), /robloxstudio-mcp 3\.5\.0, which needs its auth token: set RUR_STUDIO_MCP_TOKEN/);
		assert.deepEqual(routes(), ["GET /health"]);
	}
	result = await read(url("current"), { RUR_STUDIO_MCP_TOKEN: UNSENDABLE });
	assert.match(result.errors.get("31"), /RUR_STUDIO_MCP_TOKEN holds characters an HTTP header cannot carry/);
	assert.deepEqual(routes(), ["GET /health"]);

	// A token the server refuses: said so, without the token
	result = await read(url("current"), { RUR_STUDIO_MCP_TOKEN: WRONG });
	assert.match(result.errors.get("31"), /the Studio bridge refused RUR_STUDIO_MCP_TOKEN \(HTTP 401\)/);
	assert.deepEqual(routes(), ["GET /health", "POST /mcp/get_connected_instances"]);
	await scanFails(url("current"), { RUR_STUDIO_MCP_TOKEN: WRONG }, /refused RUR_STUDIO_MCP_TOKEN \(HTTP 401\)/);

	// Redirects are refused, never followed: from /health, and from a tool that would carry the token
	result = await read(url("redirecting"), { RUR_STUDIO_MCP_TOKEN: TOKEN });
	assert.match(result.errors.get("31"), /Studio bridge \/health answered with a redirect \(HTTP 302\), which is not followed/);
	result = await read(url("redirectingTools"), { RUR_STUDIO_MCP_TOKEN: TOKEN });
	assert.match(result.errors.get("31"), /Studio bridge \/mcp\/get_connected_instances answered with a redirect \(HTTP 302\)/);
	await scanFails(url("redirecting"), {}, /answered with a redirect/);
	assert.equal(awayAsked, 0, "no redirect was followed");

	// A server that quotes the token in an error: rur's error has it taken out
	result = await read(url("leakyLuau"), { RUR_STUDIO_MCP_TOKEN: TOKEN });
	assert.match(result.errors.get("31"), /Studio \(Other Place\) could not read it: Studio saw <RUR_STUDIO_MCP_TOKEN> in the code/, "the last session asked");
	result = await read(url("leakyTool"), { RUR_STUDIO_MCP_TOKEN: TOKEN });
	assert.match(result.errors.get("31"), /Studio bridge \/mcp\/get_connected_instances: HTTP 500 \(bad request from <RUR_STUDIO_MCP_TOKEN>\)/);

	// No error text holds a token
	assert.ok(errors.length >= 20, `${errors.length} error texts checked`);
	for (const text of errors) {
		for (const secret of [TOKEN, WRONG, UNSENDABLE, "line-one", "line-two"]) assert.ok(!text.includes(secret), `an error quotes a token: ${text}`);
	}
	console.log(`Asset studio bridge: robloxstudio-mcp 3 with its token, the older API, loopback only, no redirects and ${errors.length} error texts without the token passed.`);
} finally {
	globalThis.fetch = realFetch;
	for (const server of Object.values(servers)) server.close();
}
