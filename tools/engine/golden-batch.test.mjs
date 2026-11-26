import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import test from "node:test";
import {
	BATCH_ANSWER,
	BATCH_SCENE,
	BatchWorker,
	digest,
	digestSceneWithNodeCache,
	makeLiveTracker,
	normalizeScene,
	normalizeText,
	normalizeValue,
} from "./golden.mjs";

function slowNormalizeValue(value) {
	if (typeof value === "string") return normalizeText(value);
	if (Array.isArray(value)) return value.map(slowNormalizeValue);
	if (value && typeof value === "object") {
		const result = {};
		for (const key of Object.keys(value).sort()) result[key] = slowNormalizeValue(value[key]);
		return result;
	}
	return value;
}

class FakeChild extends EventEmitter {
	constructor(handler) {
		super();
		this.pid = 0;
		this.exitCode = null;
		this.signalCode = null;
		this.stdin = new PassThrough();
		this.stdout = new PassThrough();
		this.stderr = new PassThrough();
		this.killed = false;
		let pending = "";
		this.stdin.setEncoding("utf8");
		this.stdin.on("data", chunk => {
			pending += chunk;
			let nl = pending.indexOf("\n");
			while (nl >= 0) {
				const line = pending.slice(0, nl).trim();
				pending = pending.slice(nl + 1);
				if (line) handler(this, JSON.parse(line));
				nl = pending.indexOf("\n");
			}
		});
	}

	kill(signal = "SIGKILL") {
		this.killed = true;
		this.signalCode = signal;
	}
}

test("BatchWorker recovers cleanly from a mid-batch timeout and ignores stale child events", async () => {
	let spawns = 0;
	let staleChild = null;
	const worker = new BatchWorker({
		timeoutMs: 40,
		spawnCommand() {
			spawns++;
			const generation = spawns;
			const child = new FakeChild((self, job) => {
				if (generation === 1) {
					// Hang job 1 past timeout; do NOT emit close immediately so stale close fires during job 2.
					staleChild = self;
					return;
				}
				// While job 2 is in flight on the replacement worker, fire stale stdout and close from child 1.
				if (job.argv[0] === "job-2" && staleChild) {
					const stale = staleChild;
					staleChild = null;
					stale.stdout.write(`${BATCH_SCENE}${job.id - 1} {"stale":true}\n`);
					stale.stdout.write(`${BATCH_ANSWER}${JSON.stringify({ id: job.id - 1, code: 0 })}\n`);
					stale.exitCode = 1;
					stale.emit("close", 1);
				}
				setTimeout(() => {
					self.stdout.write(`${BATCH_SCENE}${job.id} ${JSON.stringify({ job: job.argv[0], seq: job.id })}\n`);
					self.stdout.write(`${BATCH_ANSWER}${JSON.stringify({ id: job.id, code: 0 })}\n`);
				}, 5);
			});
			return child;
		},
	});

	const r1 = await worker.run(["job-1"], {}, { inline: true });
	assert.equal(r1, "timeout");

	for (const name of ["job-2", "job-3", "job-4"]) {
		let scene = null;
		const answer = await worker.run([name], {}, {
			inline: true,
			onLine(line) {
				if (line.startsWith(BATCH_SCENE)) scene = JSON.parse(line.slice(BATCH_SCENE.length));
			},
		});
		assert.equal(answer?.code, 0, `${name} should succeed on replacement worker`);
		assert.deepEqual(scene, { job: name, seq: answer.id }, `${name} must get its own scene`);
	}
	assert.equal(spawns, 2);
	await worker.close(100);
});

test("BatchWorker rejects mismatched job id on answer or scene line", async () => {
	const worker = new BatchWorker({
		timeoutMs: 500,
		spawnCommand() {
			return new FakeChild((self, job) => {
				self.stdout.write(`${BATCH_SCENE}${job.id + 99} {"wrong":true}\n`);
				self.stdout.write(`${BATCH_ANSWER}${JSON.stringify({ id: job.id + 99, code: 0 })}\n`);
			});
		},
	});
	let seenScene = false;
	const answer = await worker.run(["bad-id"], {}, {
		inline: true,
		onLine() {
			seenScene = true;
		},
	});
	assert.equal(answer, null, "mismatched job id must be rejected");
	assert.equal(seenScene, false, "mismatched scene line must not reach caller");
	await worker.close(100);
});

test("BatchWorker.close enforces its timeout when a worker ignores stdin EOF", async () => {
	let spawned = null;
	const worker = new BatchWorker({
		timeoutMs: 500,
		spawnCommand() {
			spawned = new FakeChild((self, job) => {
				self.stdout.write(`${BATCH_ANSWER}${JSON.stringify({ id: job.id, code: 0 })}\n`);
			});
			return spawned;
		},
	});
	const res = await worker.run(["ok"], {});
	assert.equal(res?.code, 0);
	const started = Date.now();
	await worker.close(60);
	assert.ok(Date.now() - started < 1000, "close() must resolve after timeout instead of hanging");
	assert.equal(spawned.killed, true, "hung worker must be killed on close timeout");
});

test("normalizeValue fast path matches slow path including own __proto__ keys", () => {
	const inputs = [
		'{"a":1,"b":"plain","c":[true,null,"table: 0x1234abcd"]}',
		'{"z":1,"a":2,"nested":{"b":"x","a":"y"}}',
		// Sorted object with own __proto__ key ("__proto__" < "b")
		'{"__proto__":{"polluted":1},"b":"table: 0xdeadbeef"}',
		// Unsorted object with own __proto__ key
		'{"z":9,"__proto__":{"polluted":2},"a":"ok"}',
	];
	for (const raw of inputs) {
		const fast = normalizeValue(JSON.parse(raw));
		const slow = slowNormalizeValue(JSON.parse(raw));
		assert.equal(JSON.stringify(fast), JSON.stringify(slow), `mismatch on ${raw}`);
		assert.deepEqual(Object.keys(fast), Object.keys(slow), `own keys mismatch on ${raw}`);
		assert.equal(Object.hasOwn(fast, "__proto__"), Object.hasOwn(slow, "__proto__"), `own __proto__ mismatch on ${raw}`);
	}
});

test("normalizeText fast path matches full normalization on plain, address, and Studio paths", () => {
	assert.equal(normalizeText("plain label 123"), "plain label 123");
	assert.equal(normalizeText("table: 0x00ff12AB and function: 0x1234"), "table: 0x? and function: 0x?");
	assert.equal(
		normalizeText("C:/Users/dev/AppData/Local/Roblox/Versions/version-0123456789abcdef/content/fonts/A.ttf"),
		"<studio-content>/fonts/A.ttf",
	);
	assert.equal(
		normalizeText("C:\\Users\\dev\\AppData\\Local\\Roblox\\Versions\\version-0123456789abcdef\\ExtraContent\\Icons.png"),
		"<studio>/ExtraContent\\Icons.png",
	);
});

test("makeLiveTracker rebuilds patched scenes identically to full unpatched scenes", () => {
	const fullScenes = [
		{
			version: 1,
			nodes: [
				{ id: 1, text: "hello", x: 0 },
				{ id: 2, text: "table: 0x1111", x: 10 },
				{ id: 3, text: "world", x: 20 },
				{ id: 4, text: "keep", x: 30 },
			],
			extra: "first",
			diagnostics: { output: [], timings: { total: 0.1 }, fonts: [{ key: "b" }, { key: "a" }] },
		},
		// Frame 2: identical scene
		{
			version: 1,
			nodes: [
				{ id: 1, text: "hello", x: 0 },
				{ id: 2, text: "table: 0x2222", x: 10 },
				{ id: 3, text: "world", x: 20 },
				{ id: 4, text: "keep", x: 30 },
			],
			extra: "first",
			diagnostics: { output: [], timings: { total: 0.2 }, fonts: [{ key: "a" }, { key: "b" }] },
		},
		// Frame 3: single node updated + key deleted + key added
		{
			version: 1,
			nodes: [
				{ id: 1, text: "hello", x: 0 },
				{ id: 2, text: "userdata: 0x3333", x: 15 },
				{ id: 3, text: "world", x: 20 },
				{ id: 4, text: "keep", x: 30 },
			],
			added: "yes",
			diagnostics: { output: ["msg"], timings: { total: 0.3 }, fonts: [{ key: "b" }, { key: "a" }] },
		},
		// Frame 4: nodes shrink (with another address string that normalizes identically)
		{
			version: 1,
			nodes: [
				{ id: 1, text: "hello", x: 0 },
				{ id: 2, text: "userdata: 0x4444", x: 15 },
			],
			added: "yes",
			diagnostics: { output: ["msg"], timings: { total: 0.4 }, fonts: [{ key: "a" }, { key: "b" }] },
		},
		// Frame 5: full nodes replacement
		{
			version: 1,
			nodes: [
				{ id: 10, text: "replaced-1", x: 1 },
				{ id: 11, text: "replaced-2", x: 2 },
			],
			added: "yes",
			diagnostics: { output: ["msg"], timings: { total: 0.5 }, fonts: [{ key: "a" }, { key: "b" }] },
		},
		// Frame 6: editable image fallback
		{
			version: 1,
			images: ["editable-images/999-2.png", "editable-images/999-1.png"],
			nodes: [{ id: 10, image: "editable-images/999-2.png" }],
			diagnostics: { output: [] },
		},
	];

	const fullTracker = makeLiveTracker();
	const patchedTracker = makeLiveTracker();
	const readyLine = `@@RUR ${JSON.stringify({ type: "ready", controls: {} })}`;
	fullTracker.pushLine(readyLine);
	patchedTracker.pushLine(readyLine);

	for (const scene of fullScenes) {
		fullTracker.pushLine(`@@RUR ${JSON.stringify({ type: "scene", scene })}`);
	}
	const patchedReplies = [
		{ type: "scene", scene: fullScenes[0] },
		{ type: "scene", same: true },
		{
			type: "scene",
			patch: {
				del: ["extra"],
				set: { added: "yes", diagnostics: fullScenes[2].diagnostics },
				nodesPatch: { len: 4, set: [[1, fullScenes[2].nodes[1]]] },
			},
		},
		{
			type: "scene",
			patch: {
				nodesPatch: { len: 2, set: [] },
			},
		},
		{
			type: "scene",
			patch: {
				set: { nodes: fullScenes[4].nodes },
			},
		},
		{ type: "scene", scene: fullScenes[5] },
	];
	for (const reply of patchedReplies) {
		patchedTracker.pushLine(`@@RUR ${JSON.stringify(reply)}`);
	}

	const fullRes = fullTracker.result([]);
	const patchedRes = patchedTracker.result([]);
	assert.deepEqual(patchedRes.scenes, fullRes.scenes, "every patched frame digest must equal full scene digest");
	assert.deepEqual(patchedRes.lastScene, fullRes.lastScene, "final scene must equal full scene");

	// Also verify digestSceneWithNodeCache matches plain digest().
	const normalized = normalizeScene(structuredClone(fullScenes[0]));
	const nodeStrings = normalized.nodes.map(n => JSON.stringify(n));
	assert.equal(digestSceneWithNodeCache(normalized, nodeStrings), digest(normalized));
});
