// `--actions` (inline JSON or a JSON file): read and checked before a render runs anything, so a bad
// list is a usage error (exit 64, nothing written) instead of a failed render. The schema is the
// engine's (native/src/engine/input/actions.rs, which checks it again); docs/stories.md "Timed input".
//
//   {t?, type:"move", x, y}   {t?, type:"down"|"up", x?, y?, button?: 1|2|3}   {t?, type:"leave"}
//   {t?, type:"wheel", x?, y?, notches | dy, dx?}   {t?, type:"key", key, down?, shift?, ctrl?, alt?}
//   {t?, type:"text"|"paste", text}
//   {t?, type:"click", <point>, button?, hold?}
//   {t?, type:"drag", from: <point> (or <point> fields on the action), to: <point>, button?, steps?, duration?}
//   <point>: node | target (an `inspect` path or a suffix of one) [+ anchor {x, y}, offset {x, y}], or x, y

import fs from "node:fs";
import path from "node:path";

export class UsageError extends Error {}

export const ACTION_TYPES = ["move", "down", "up", "wheel", "leave", "key", "text", "paste", "click", "drag"];

const describe = value => value === null ? "null" : Array.isArray(value) ? "a list" : typeof value === "object" ? "an object"
	: typeof value === "boolean" ? "a boolean" : typeof value === "number" ? "a number" : "a string";
const isNumber = value => typeof value === "number" && Number.isFinite(value);

function optionalNumber(object, key) {
	if (!(key in object)) return undefined;
	if (!isNumber(object[key])) throw new Error(`\`${key}\` must be a number, not ${describe(object[key])}`);
	return object[key];
}

function optionalBool(object, key) {
	if (key in object && typeof object[key] !== "boolean") throw new Error(`\`${key}\` must be true or false, not ${describe(object[key])}`);
}

function requiredString(object, key) {
	if (!(key in object)) throw new Error(`\`${key}\` (a string) is missing`);
	if (typeof object[key] !== "string") throw new Error(`\`${key}\` must be a string, not ${describe(object[key])}`);
}

const isObject = value => value !== null && typeof value === "object" && !Array.isArray(value);

function pair(object, key) {
	if (!(key in object)) return;
	const value = object[key];
	if (!isObject(value)) throw new Error(`\`${key}\` must be an object {"x": .., "y": ..}, not ${describe(value)}`);
	const extra = Object.keys(value).find(name => name !== "x" && name !== "y");
	if (extra) throw new Error(`\`${key}\` takes x and y, not \`${extra}\``);
	try {
		optionalNumber(value, "x");
		optionalNumber(value, "y");
	} catch (error) {
		throw new Error(`\`${key}\`: ${error.message}`);
	}
}

// A point's fields on `object`: true when it has a node or x, y
function pointFields(object, what) {
	const prefixed = fn => {
		try {
			return fn();
		} catch (error) {
			throw new Error(`${what}: ${error.message}`);
		}
	};
	if ("node" in object && "target" in object) throw new Error(`${what}: give \`node\` or \`target\`, not both`);
	const key = "node" in object ? "node" : "target" in object ? "target" : null;
	const x = prefixed(() => optionalNumber(object, "x"));
	const y = prefixed(() => optionalNumber(object, "y"));
	if (key) {
		if (typeof object[key] !== "string") throw new Error(`${what}: \`${key}\` must be a node path string, not ${describe(object[key])}`);
		if (!object[key].trim()) throw new Error(`${what}: \`${key}\` is empty`);
		if (x !== undefined || y !== undefined) throw new Error(`${what}: give a node or x, y, not both (use \`offset\` to move off the node's anchor)`);
		prefixed(() => pair(object, "anchor"));
		prefixed(() => pair(object, "offset"));
		return true;
	}
	for (const name of ["anchor", "offset"]) {
		if (name in object) throw new Error(`${what}: \`${name}\` needs a \`node\``);
	}
	if ((x === undefined) !== (y === undefined)) throw new Error(`${what}: needs both x and y`);
	return x !== undefined;
}

function pointValue(value, what) {
	if (typeof value === "string" && value.trim()) return;
	if (isObject(value)) {
		if (!pointFields(value, what)) throw new Error(`${what}: needs \`node\` (or \`target\`) or x, y`);
		return;
	}
	throw new Error(`${what}: must be a node path or {"node": ..} / {"x": .., "y": ..}, not ${describe(value)}`);
}

function checkButton(object) {
	const button = optionalNumber(object, "button");
	if (button !== undefined && ![1, 2, 3].includes(button)) throw new Error(`\`button\` must be 1, 2 or 3, not ${button}`);
}

// Seconds after its start that an action's last event comes (a click's hold, a drag's duration)
function span(action) {
	if (action.type === "click") return action.hold ?? 0;
	if (action.type === "drag") return action.duration ?? 0.25;
	return 0;
}

function check(action) {
	if (!isObject(action)) throw new Error(`an action is an object, not ${describe(action)}`);
	if (!("type" in action)) throw new Error(`\`type\` is missing (one of ${ACTION_TYPES.join(", ")})`);
	if (typeof action.type !== "string") throw new Error(`\`type\` must be a string, not ${describe(action.type)}`);
	if (!ACTION_TYPES.includes(action.type)) throw new Error(`unknown type "${action.type}" (one of ${ACTION_TYPES.join(", ")})`);
	const t = optionalNumber(action, "t");
	if (t !== undefined && t < 0) throw new Error(`\`t\` must be 0 or more, not ${t}`);
	switch (action.type) {
		case "move": case "down": case "up": case "wheel": {
			const x = optionalNumber(action, "x");
			const y = optionalNumber(action, "y");
			if (action.type === "move" && (x === undefined || y === undefined)) throw new Error("move: needs x and y");
			if (action.type === "down" || action.type === "up") checkButton(action);
			if (action.type === "wheel") {
				const notches = optionalNumber(action, "notches");
				const dy = optionalNumber(action, "dy");
				optionalNumber(action, "dx");
				if (notches === undefined && dy === undefined) throw new Error("wheel: needs `notches` (+1 = one notch away from the user) or `dy` (px, 100 per notch)");
			}
			break;
		}
		case "key":
			requiredString(action, "key");
			for (const flag of ["down", "shift", "ctrl", "alt"]) optionalBool(action, flag);
			break;
		case "text": case "paste":
			requiredString(action, "text");
			break;
		case "click": {
			if (!pointFields(action, "click")) throw new Error("click: needs `node` (or `target`) or x, y");
			checkButton(action);
			const hold = optionalNumber(action, "hold");
			if (hold !== undefined && hold < 0) throw new Error(`\`hold\` must be 0 or more, not ${hold}`);
			break;
		}
		case "drag": {
			const own = pointFields(action, "drag");
			if ("from" in action && own) throw new Error("drag: give `from` or node / x, y on the action, not both");
			if ("from" in action) pointValue(action.from, "drag `from`");
			else if (!own) throw new Error('drag: needs `from` (a node path or {"x": .., "y": ..})');
			if (!("to" in action)) throw new Error('drag: needs `to` (a node path or {"x": .., "y": ..})');
			pointValue(action.to, "drag `to`");
			checkButton(action);
			const steps = optionalNumber(action, "steps");
			if (steps !== undefined && (steps < 1 || steps > 10000 || !Number.isInteger(steps))) throw new Error(`\`steps\` must be a whole number from 1 to 10000, not ${steps}`);
			const duration = optionalNumber(action, "duration");
			if (duration !== undefined && duration < 0) throw new Error(`\`duration\` must be 0 or more, not ${duration}`);
			break;
		}
	}
}

// Throws a UsageError naming the first bad action (`--actions[<index>] (<type>): ...`). `time`: the
// run's --time; an action starting or ending after it would be squeezed onto the capture moment.
export function validateActions(actions, { time } = {}) {
	if (!Array.isArray(actions)) throw new UsageError(`--actions: a JSON list of actions expected, not ${describe(actions)}`);
	actions.forEach((action, index) => {
		const label = `--actions[${index}]${isObject(action) && typeof action.type === "string" ? ` (${action.type})` : ""}`;
		try {
			check(action);
		} catch (error) {
			throw new UsageError(`${label}: ${error.message}`);
		}
		if (time !== undefined) {
			const end = (action.t ?? 0) + span(action);
			if (end > time + 1e-9) {
				throw new UsageError(`${label}: ${span(action) ? `ends at ${+end.toFixed(6)} s` : `is at ${+end.toFixed(6)} s`}, after the capture at --time ${time}; raise --time`);
			}
		}
	});
	return actions;
}

// The list of `--actions <inline JSON | file>`, validated (UsageError when it cannot be used)
export function readActions(spec, { time } = {}) {
	if (spec === true || spec === undefined || spec === null || String(spec).trim() === "") throw new UsageError("--actions needs a JSON list or a JSON file");
	const text = String(spec);
	const inline = text.trim().startsWith("[") || text.trim().startsWith("{");
	let source = "--actions";
	let json = text;
	if (!inline) {
		const file = path.resolve(text);
		try {
			json = fs.readFileSync(file, "utf8");
		} catch (error) {
			throw new UsageError(`--actions ${file}: ${error.code === "ENOENT" ? "no such file" : error.message} (give a JSON list or a JSON file)`);
		}
		source = `--actions ${file}`;
	}
	let actions;
	try {
		actions = JSON.parse(json);
	} catch (error) {
		throw new UsageError(`${source}: not valid JSON: ${error.message}`);
	}
	try {
		return validateActions(actions, { time });
	} catch (error) {
		if (inline || !(error instanceof UsageError)) throw error;
		throw new UsageError(`${error.message} (in ${source.slice("--actions ".length)})`);
	}
}
