// Page-side host of the browser build (native/src/web.rs): loads a story bundle (tools/web/bundle.mjs)
// into the module's in-memory file system, mounts stories on a canvas and drives their frames with
// the page's input. No UI of its own: a page (native/web/index.html, or any page that embeds the
// module) owns the canvas placement and turns its pointer / wheel / key events into the engine's
// session events in logical viewport coordinates.
//
//   const host = await RurHost.load({ base: "./", canvas, onStatus });
//   await host.mount(0);                  // host.stories: [{name, path, config}]
//   host.start(stats => ...);             // one engine frame per display frame
//   host.queue({type: "move", x, y});     // session events (docs/stories.md "Timed input")
//
// Everything is fetched from `base` (the page's own directory): bundle.json, bundle.<n>.gz.wasm,
// rur-web.gz.wasm and, unless the page already defines createRurWeb, rur-web.js. The .gz.wasm files hold
// gzipped bytes in a WebAssembly custom section (artifact hosts serve .wasm); DecompressionStream inflates them.
"use strict";

(function (global) {
	// The gzipped payload of a `.gz.wasm` file: a WebAssembly header and one custom section
	// (tools/web/bundle.mjs gzipWasm)
	function unwrap(bytes, url) {
		const magic = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x00];
		if (magic.some((byte, i) => bytes[i] !== byte)) throw new Error(`${url} is not a wrapped bundle file`);
		let at = magic.length;
		const leb = () => {
			let value = 0;
			let shift = 0;
			for (;;) {
				const byte = bytes[at++];
				value += (byte & 0x7f) * 2 ** shift;
				shift += 7;
				if (!(byte & 0x80)) return value;
			}
		};
		const size = leb();
		const end = at + size;
		const nameLength = leb();
		at += nameLength;
		return bytes.subarray(at, end);
	}

	const inflate = async (response, url) => {
		const gz = url.endsWith(".gz.wasm") || url.endsWith(".gz");
		if (!gz) return new Uint8Array(await response.arrayBuffer());
		if (typeof DecompressionStream !== "function") throw new Error("this browser cannot inflate gzip (DecompressionStream)");
		let bytes = new Uint8Array(await response.arrayBuffer());
		if (url.endsWith(".gz.wasm")) bytes = unwrap(bytes, url);
		return new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"))).arrayBuffer());
	};

	async function fetchBytes(url, onBytes) {
		const response = await fetch(url);
		if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
		const total = Number(response.headers.get("content-length")) || 0;
		if (onBytes && response.body && total) {
			// Count the compressed bytes as they arrive (progress), then inflate
			const reader = response.body.getReader();
			const chunks = [];
			let received = 0;
			for (;;) {
				const { done, value } = await reader.read();
				if (done) break;
				chunks.push(value);
				received += value.length;
				onBytes(value.length);
			}
			return inflate(new Response(new Blob(chunks)), url);
		}
		const bytes = await inflate(response, url);
		if (onBytes) onBytes(bytes.length);
		return bytes;
	}

	// Lets the page paint its status before a long synchronous call (a timer: a hidden page gets no
	// animation frames)
	const nextPaint = () => new Promise(resolve => {
		let done = false;
		const finish = () => {
			if (!done) {
				done = true;
				setTimeout(resolve, 0);
			}
		};
		requestAnimationFrame(finish);
		setTimeout(finish, 100);
	});

	function loadScript(url) {
		return new Promise((resolve, reject) => {
			const script = document.createElement("script");
			script.src = url;
			script.onload = resolve;
			script.onerror = () => reject(new Error(`${url} did not load`));
			document.head.append(script);
		});
	}

	// DOM KeyboardEvent.code -> Enum.KeyCode name (native/src/input.rs key_name)
	const KEY_NAMES = {
		Enter: "Return", NumpadEnter: "KeypadEnter", Space: "Space", Backspace: "Backspace", Tab: "Tab", Escape: "Escape",
		Delete: "Delete", Insert: "Insert", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
		ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
		ShiftLeft: "LeftShift", ShiftRight: "RightShift", ControlLeft: "LeftControl", ControlRight: "RightControl",
		AltLeft: "LeftAlt", AltRight: "RightAlt", MetaLeft: "LeftSuper", MetaRight: "RightSuper",
		Minus: "Minus", Equal: "Equals", BracketLeft: "LeftBracket", BracketRight: "RightBracket", Backslash: "BackSlash",
		Semicolon: "Semicolon", Quote: "Quote", Comma: "Comma", Period: "Period", Slash: "Slash", Backquote: "Backquote",
	};
	const DIGITS = ["Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine"];
	function keyName(code) {
		if (KEY_NAMES[code]) return KEY_NAMES[code];
		let match = /^Key([A-Z])$/.exec(code);
		if (match) return match[1];
		match = /^Digit(\d)$/.exec(code);
		if (match) return DIGITS[Number(match[1])];
		match = /^Numpad(\d)$/.exec(code);
		if (match) return `Keypad${DIGITS[Number(match[1])]}`;
		if (/^F\d{1,2}$/.test(code)) return code;
		return null;
	}

	// The session events of a DOM key event: the key, and the typed text of a printable key press
	function keyEvents(event, down) {
		const events = [];
		const name = keyName(event.code);
		const ctrl = event.ctrlKey || event.metaKey;
		if (name) events.push({ type: "key", key: name, down, shift: event.shiftKey, ctrl, alt: event.altKey });
		if (down && !ctrl && !event.altKey && event.key && [...event.key].length === 1) events.push({ type: "text", text: event.key });
		return events;
	}

	// Wheel notches of a DOM wheel event (+1 = one notch away from the user). A mouse notch arrives as
	// ~100 px (or 3 lines) and is half a notch, as the desktop app's winit line delta is
	// (native/src/input.rs wheel_notches); trackpad deltas stay proportional
	function wheelNotches(event, pageHeight) {
		const delta = -event.deltaY;
		if (!Number.isFinite(delta) || !delta) return 0;
		const lines = event.deltaMode === 1 ? delta / 3 : event.deltaMode === 2 ? delta * pageHeight / 100 : delta / 100;
		return 0.5 * lines;
	}

	// FNV-1a (32-bit) of bytes as lower-case hex without leading zeros (native/src/headless.rs fnv1a)
	function fnv1a(bytes) {
		let hash = 0x811c9dc5;
		for (let i = 0; i < bytes.length; i++) {
			hash ^= bytes[i];
			hash = Math.imul(hash, 0x01000193) >>> 0;
		}
		return hash.toString(16);
	}

	class RurHost {
		constructor(module, index, canvas) {
			this.module = module;
			this.index = index;
			this.canvas = canvas;
			this.stories = index.stories;
			this.story = null;
			this.ready = null;
			this.events = [];
			this.running = false;
			this.onFrame = null;
			this.last = 0;
			const cwrap = module.cwrap;
			this.calls = {
				init: cwrap("rur_web_init", "number", ["string"]),
				frame: cwrap("rur_web_frame", "number", ["number", "string", "number"]),
				scene: cwrap("rur_web_scene", "number", []),
				drop: cwrap("rur_web_drop", "number", []),
				gpu: cwrap("rur_web_gpu", "number", []),
			};
		}

		// Fetches the bundle and the module from `base` and fills the file system. `onStatus(text,
		// fraction)` reports progress.
		static async load({ base = "", canvas, onStatus = () => {}, print = null } = {}) {
			const url = name => `${base}${name}`;
			onStatus("Loading", 0);
			const index = await fetch(url("bundle.json")).then(response => {
				if (!response.ok) throw new Error(`bundle.json: HTTP ${response.status}`);
				return response.json();
			});
			if (typeof global.createRurWeb !== "function") await loadScript(url("rur-web.js"));
			let received = 0;
			const expected = index.bytes || 0;
			const onBytes = count => {
				received += count;
				if (expected) onStatus("Loading", Math.min(0.95, received / expected));
			};
			// The module first: the renderer's shaders compile (seconds on a first visit) while the story's
			// files still download
			const partsLoaded = Promise.all(index.parts.map(name => fetchBytes(url(name), onBytes)));
			partsLoaded.catch(() => {});
			const wasmBinary = await fetchBytes(url(index.module || "rur-web.gz.wasm"), onBytes);
			const module = await global.createRurWeb({
				canvas,
				// The module from the bytes fetched above (the loader would fetch rur_web.wasm itself)
				instantiateWasm(imports, done) {
					WebAssembly.instantiate(wasmBinary, imports).then(result => done(result.instance, result.module), error => console.error(error));
					return {};
				},
				print: print || (text => console.log(text)),
				printErr: print || (text => console.warn(text)),
			});
			const host = new RurHost(module, index, canvas);
			onStatus("Preparing the renderer", Math.min(0.95, received / (expected || 1)));
			await nextPaint();
			const gpuStarted = performance.now();
			host.gpu = host.call(host.calls.gpu);
			host.gpu.wallMs = performance.now() - gpuStarted;
			const parts = await partsLoaded;
			onStatus("Starting the story", 0.97);
			await nextPaint();
			const FS = module.FS;
			for (const dir of index.dirs) FS.mkdirTree(dir);
			const packed = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
			parts.reduce((at, part) => (packed.set(part, at), at + part.length), 0);
			for (const [file, offset, length] of index.files) FS.writeFile(file, packed.subarray(offset, offset + length));
			for (const file of index.empty) FS.writeFile(file, new Uint8Array(0));
			return host;
		}

		call(fn, ...args) {
			const reply = JSON.parse(this.module.UTF8ToString(fn(...args)));
			if (reply.error) throw new Error(reply.error);
			return reply.ok;
		}

		// Mounts story `which` (index or name); options: {firstDraw, controls}. Sizes the canvas to
		// the frame (physical pixels) first.
		mount(which = 0, { firstDraw = true, controls } = {}) {
			const story = typeof which === "number" ? this.stories[which] : this.stories.find(entry => entry.name === which || entry.path === which);
			if (!story) throw new Error(`no story ${which} in this bundle`);
			const config = { ...story.config, env: this.index.env, firstDraw };
			if (controls) config.controls = controls;
			const margin = config.margin || 0;
			this.canvas.width = Math.round((config.viewport[0] + 2 * margin) * config.scale);
			this.canvas.height = Math.round((config.viewport[1] + 2 * margin) * config.scale);
			this.events = [];
			this.ready = this.call(this.calls.init, JSON.stringify(config));
			// A story that failed to mount says why instead of leaving an empty stage
			if (this.ready.status === "error") throw new Error(`${story.path}: ${this.ready.failure || "the story failed to mount"}`);
			this.story = story;
			return this.ready;
		}

		// One frame: `events` go to the engine with `dt` seconds; draws when anything changed (or force)
		frame(dt, events = [], force = false) {
			return this.call(this.calls.frame, dt, JSON.stringify(events), force ? 1 : 0);
		}

		scene() {
			return JSON.parse(this.call(this.calls.scene));
		}

		// Queues a session event for the next frame (moves coalesce, as the desktop app's do)
		queue(event) {
			const last = this.events[this.events.length - 1];
			if (event.type === "move" && last && last.type === "move") this.events[this.events.length - 1] = event;
			else this.events.push(event);
		}

		// One engine frame per display frame (requestAnimationFrame; a timer keeps a hidden or
		// throttled page ticking slowly). `onFrame(stats)` sees each frame's timings.
		start(onFrame) {
			if (this.running) return;
			this.running = true;
			this.onFrame = onFrame || null;
			this.last = performance.now();
			const tick = now => {
				if (!this.running) return;
				const dt = Math.min(Math.max((now - this.last) / 1000, 1 / 240), 0.1);
				this.last = now;
				const events = this.events.splice(0);
				const started = performance.now();
				let stats;
				try {
					stats = this.frame(dt, events);
				} catch (error) {
					this.running = false;
					if (this.onFrame) this.onFrame({ error });
					return;
				}
				stats.wallMs = performance.now() - started;
				stats.events = events.length;
				if (this.onFrame) this.onFrame(stats);
				schedule();
			};
			const schedule = () => {
				let done = false;
				const run = now => {
					if (!done) {
						done = true;
						tick(now);
					}
				};
				requestAnimationFrame(run);
				setTimeout(() => run(performance.now()), 250);
			};
			schedule();
		}

		stop() {
			this.running = false;
		}

		// The canvas as drawn (call in the same task as the frame: the drawing buffer is not
		// preserved), with alpha un-premultiplied like render.png (GpuRenderer::readback_png_rgba)
		readPixels() {
			const gl = this.canvas.getContext("webgl2");
			const { width, height } = this.canvas;
			const flipped = new Uint8Array(width * height * 4);
			gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, flipped);
			const rgba = new Uint8Array(width * height * 4);
			const row = width * 4;
			for (let y = 0; y < height; y++) rgba.set(flipped.subarray((height - 1 - y) * row, (height - y) * row), y * row);
			for (let i = 0; i < rgba.length; i += 4) {
				const alpha = rgba[i + 3];
				if (alpha > 0 && alpha < 255) {
					for (let c = 0; c < 3; c++) rgba[i + c] = Math.min(255, Math.floor((rgba[i + c] * 255 + (alpha >> 1)) / alpha));
				}
			}
			return { width, height, rgba };
		}
	}

	RurHost.keyEvents = keyEvents;
	RurHost.keyName = keyName;
	RurHost.wheelNotches = wheelNotches;
	RurHost.fnv1a = fnv1a;
	global.RurHost = RurHost;
})(typeof window !== "undefined" ? window : globalThis);
