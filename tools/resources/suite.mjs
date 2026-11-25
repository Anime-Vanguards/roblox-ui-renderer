// rur's resource workloads (tools/resources/measure.mjs runs them; AGENTS.md "Resource budgets" says when).
//
// - golden-fast-*: the golden check's fast corpus (`tools/engine/golden.mjs --fast`: every fixture and probe in edit
//   mode, and three live sessions, in batch workers) on a cold and then a warm state folder. It runs from a copy of the
//   check's files in the run's folder, with no frozen golden data beside it and Roblox Studio hidden, so every machine
//   (a runner has no Studio) runs the same thing and nothing is written into the checkout. The entries whose text needs
//   Studio's fonts differ there; what is measured is the work, not the verdict. How many entries ran is golden's own
//   count, checked against the fixtures and probes the copy has.
// - batch-stress: 12 `rur-native engine-batch` workers sharing one cold state folder, each running 30 fixtures over a
//   project this file generates (about 40 MB of bytecode), as the golden check's workers run a game's stories: where a
//   module store that compacts too often, or reads each new pack whole, shows.
// - golden-all-*: `golden.mjs --all` cold and warm, which needs the frozen project in out/golden (`--freeze`), so it
//   runs only on a machine that has one; with 12 workers, as the check runs on a large machine.
//
// Options: --engine <rur-native> (default: RUR_NATIVE_ENGINE, else native/target/release/rur-native).

import fs from "node:fs";
import path from "node:path";

export const FLAGS = [];

export const SUITES = {
	ci: { about: "What CI runs on every push and pull request: under a minute on a 4-CPU runner.", workloads: ["golden-fast-cold", "golden-fast-warm", "batch-stress"] },
	full: { about: "The CI set and the whole golden check on the frozen project, cold and warm (a few minutes; needs out/golden).", workloads: ["golden-fast-cold", "golden-fast-warm", "batch-stress", "golden-all-cold", "golden-all-warm"] },
};

const EXE = process.platform === "win32" ? "rur-native.exe" : "rur-native";

function engine(context) {
	const given = context.args.engine || process.env.RUR_NATIVE_ENGINE;
	const binary = path.resolve(given || path.join(context.repo, "native", "target", "release", EXE));
	if (!fs.existsSync(binary)) throw new Error(`tools/resources: no rur-native at ${binary} (node tools/native-app/build.mjs, or pass --engine <exe>)`);
	return binary;
}

// The folders a workload uses: its state folder (module store, caches), its output and its temp folder
function folders(context, id, state, out) {
	return [
		{ label: "State folder (module store, caches)", dir: state, kind: "state" },
		{ label: "Output folder", dir: out, kind: "out" },
		{ label: "Temp folder", dir: path.join(context.work, id, "temp"), kind: "temp" },
	];
}

// What a run sees of the machine: Studio hidden (no install, no per-user folder to find one in), the state folder given
function hermetic(context, state) {
	const empty = path.join(context.work, "no-studio");
	fs.mkdirSync(path.join(empty, "local"), { recursive: true });
	return { RUR_STATE_DIR: state, RUR_NATIVE_ENGINE: engine(context), ROBLOX_STUDIO_PATH: empty, LOCALAPPDATA: path.join(empty, "local") };
}

const leftOutOfPublic = () => () => false;
const PUBLIC_CORPUS = "";

// A copy of what the golden check needs (its scripts, the CLI, fixtures, data), made once per run folder
function checkCopy(context) {
	const copy = path.join(context.work, "rur");
	if (fs.existsSync(path.join(copy, "done"))) return copy;
	const leftOut = leftOutOfPublic(context.repo);
	for (const name of ["cli", "tools", "fixtures", "data", "package.json"]) {
		fs.cpSync(path.join(context.repo, name), path.join(copy, name), {
			recursive: true,
			filter: source => !source.includes(`${path.sep}resources${path.sep}`) && !leftOut(path.relative(context.repo, source).split(path.sep).join("/")),
		});
	}
	fs.writeFileSync(path.join(copy, "done"), "");
	return copy;
}

// The fewest entries golden's fast corpus can have in `copy`: one per fixture and per probe (created-trace.luau is a
// script, not a probe), and the three live sessions; actions and quarter-pixel text runs only add to it
function fastFloor(copy) {
	const luau = dir => fs.readdirSync(path.join(copy, dir)).filter(name => name.endsWith(".luau") && name !== "created-trace.luau").length;
	return luau("fixtures") + luau(path.join("tools", "engine", "probes")) + 3;
}

// The golden check's last line: "<same>/<all> identical to the golden output[, <n> differ ...] in <s> s". Golden prints it
// once every entry it chose has a verdict, so a crash or an early exit leaves none. How many it chose is its own count on
// this machine; it may not be under `prepared.floor`.
function verifyGolden(prepared) {
	const log = fs.readFileSync(prepared.log, "utf8");
	const match = log.match(/(\d+)\/(\d+) identical to the golden output(?:, (\d+) differ)?/);
	if (!match) return "the golden check printed no result (it crashed or stopped early)";
	const [same, all, differ] = [Number(match[1]), Number(match[2]), Number(match[3] || 0)];
	if (/\d+ timed out/.test(log)) return "entries timed out";
	if (all < prepared.floor) return `golden ran ${all} entries, fewer than the ${prepared.floor} its inputs make`;
	if (same + differ !== all) return `${all - same - differ} entries have no golden`;
	if (prepared.exact && differ) return `${differ} of ${all} entries differ from the golden output`;
	return null;
}

function goldenFast({ id, title, about, fresh, requires }) {
	return {
		id, title, about, requires,
		prepare(context) {
			const copy = checkCopy(context);
			const state = path.join(context.work, "fast-state");
			if (fresh) fs.rmSync(state, { recursive: true, force: true });
			fs.mkdirSync(state, { recursive: true });
			const out = path.join(copy, "out");
			fs.rmSync(out, { recursive: true, force: true });
			fs.rmSync(path.join(context.work, id), { recursive: true, force: true });
			const log = path.join(context.work, id, "golden.log");
			return {
				argv: [context.node, path.join(copy, "tools", "engine", "golden.mjs"), "--fast", "--rerun", "--jobs", "6"],
				cwd: copy, log, env: hermetic(context, state), folders: folders(context, id, state, out),
				// differences are reported (exit 1); every entry golden chose has to run
				acceptExit: [0, 1], floor: fastFloor(copy),
			};
		},
		verify: verifyGolden,
	};
}

function goldenAll({ id, title, about, fresh, requires }) {
	return {
		id, title, about, requires,
		skip: context => (fs.existsSync(path.join(context.repo, "out", "golden", "project")) ? null : "no frozen project in out/golden (golden.mjs --freeze)"),
		prepare(context) {
			const state = path.join(context.work, "all-state");
			if (fresh) fs.rmSync(state, { recursive: true, force: true });
			fs.mkdirSync(state, { recursive: true });
			fs.rmSync(path.join(context.work, id), { recursive: true, force: true });
			const log = path.join(context.work, id, "golden.log");
			return {
				argv: [context.node, path.join(context.repo, "tools", "engine", "golden.mjs"), "--all", "--rerun", "--jobs", "12"],
				cwd: context.repo, log, env: { RUR_STATE_DIR: state, RUR_NATIVE_ENGINE: engine(context) },
				folders: [
					{ label: "State folder (module store, caches)", dir: state, kind: "state" },
					{ label: "Temp folder", dir: path.join(context.work, id, "temp"), kind: "temp" },
				],
				// a developer's machine: one CPU per worker
				cpus: 12, floor: 1, exact: true,
			};
		},
		verify: verifyGolden,
	};
}

// A deterministic module of about `bytes` characters: functions with their own constants and strings, a data table
function moduleSource(name, seed, bytes) {
	const lines = ["local M = {}", `M.Name = "${name}"`];
	let state = seed * 2654435761 % 4294967296;
	const next = () => (state = (state * 1103515245 + 12345) % 2147483648);
	let size = 40, index = 0;
	while (size < bytes) {
		const a = next() % 97 + 3, b = next() % 89 + 2, c = next() % 1000 / 7;
		const body = [
			`function M.F${index}(X: number, Y: number): number`,
			"\tlocal Total = 0",
			`\tfor I = 1, X do Total += (I * ${a} + Y) % ${b} + ${c.toFixed(3)} end`,
			`\tif Total > ${next() % 5000} then return Total - ${a} else return Total * ${b % 7 + 1} end`,
			"end",
			`M.K${index} = { Key = "${name}.${index}.${next().toString(36)}", Weight = ${(next() % 100000) / 100}, Tags = { "${next().toString(36)}", "${next().toString(36)}" } }`,
		];
		for (const line of body) size += line.length + 1;
		lines.push(...body);
		index++;
	}
	lines.push(`M.Count = ${index}`, "return M", "");
	return lines.join("\n");
}

const STRESS = { jobs: 360, workers: 12, core: 24, coreBytes: 8000, own: 4, ownBytes: 28000, older: 13 };

// The generated project: ReplicatedStorage.Lib.Core (every fixture requires it) and one slice of modules per fixture.
// Fixture `job` requires the core, its own slice, the one before (another worker compiles it) and older ones (by then in
// the newest compacted pack), then lays out a row of labels: so every worker keeps loading from the newest packs and
// holds them mapped, as a worker over a real project does.
function stressProject(context) {
	const root = path.join(context.work, "stress-project");
	if (fs.existsSync(path.join(root, "done"))) return root;
	const lib = path.join(root, "roots", "ReplicatedStorage", "Lib");
	fs.mkdirSync(path.join(lib, "Core"), { recursive: true });
	for (let index = 0; index < STRESS.core; index++) fs.writeFileSync(path.join(lib, "Core", `C${index}.luau`), moduleSource(`C${index}`, index + 1, STRESS.coreBytes));
	const fixtures = path.join(root, "fixtures");
	fs.mkdirSync(fixtures, { recursive: true });
	for (let job = 0; job < STRESS.jobs; job++) {
		const slice = path.join(lib, "Slices", `J${String(job).padStart(4, "0")}`);
		fs.mkdirSync(slice, { recursive: true });
		for (let index = 0; index < STRESS.own; index++) fs.writeFileSync(path.join(slice, `J${job}U${index}.luau`), moduleSource(`J${job}U${index}`, 100000 + job * 100 + index, STRESS.ownBytes));
		const older = Array.from({ length: Math.min(STRESS.older, job) }, (_, index) => (job * 37 + index * 13) % job);
		const slices = [...new Set([job, ...(job > 0 ? [job - 1] : []), ...older])];
		fs.writeFileSync(path.join(fixtures, `Job${job}.luau`), [
			`-- resource suite job ${job}: requires slices ${slices.join(", ")}`,
			"return function(Target)",
			"\tlocal Lib = game:GetService(\"ReplicatedStorage\"):WaitForChild(\"Lib\")",
			"\tlocal Total = 0",
			"\tfor _, Module in Lib.Core:GetChildren() do Total += require(Module).F0(3, 1) end",
			`\tfor _, Slice in { ${slices.join(", ")} } do`,
			"\t\tfor _, Module in Lib.Slices[string.format(\"J%04d\", Slice)]:GetChildren() do Total += require(Module).F0(2, 1) end",
			"\tend",
			"\tlocal List = Instance.new(\"Frame\")",
			"\tList.Size = UDim2.fromOffset(400, 300)",
			"\tlocal Layout = Instance.new(\"UIListLayout\")",
			"\tLayout.Parent = List",
			"\tfor Index = 1, 8 do",
			"\t\tlocal Label = Instance.new(\"TextLabel\")",
			"\t\tLabel.Size = UDim2.new(1, 0, 0, 24)",
			`\t\tLabel.Text = "job ${job} row " .. Index .. " total " .. math.floor(Total)`,
			"\t\tLabel.Parent = List",
			"\tend",
			"\tList.Parent = Target",
			"end",
			"",
		].join("\n"));
	}
	fs.writeFileSync(path.join(root, "done"), "");
	return root;
}

export function workloads() {
	return [
		goldenFast({
			id: "golden-fast-cold", title: "Golden check (fast corpus), cold state",
			about: `\`tools/engine/golden.mjs --fast\` with 6 batch workers on a new state folder: every fixture and probe in edit mode and three live sessions, each module compiled and saved to the module store.${PUBLIC_CORPUS}`,
			fresh: true,
		}),
		goldenFast({
			id: "golden-fast-warm", title: "Golden check (fast corpus), warm state",
			about: `The same check again on the state folder the cold run left: modules come from the store, so little should be written.${PUBLIC_CORPUS}`,
			fresh: false, requires: ["golden-fast-cold"],
		}),
		{
			id: "batch-stress", title: "Many batch workers sharing one cold store",
			about: "12 `rur-native engine-batch` workers start together on one new state folder and run 30 fixtures each over a generated project (about 40 MB of bytecode): where compactions pile up and every worker maps every new pack.",
			prepare(context) {
				const exe = engine(context);
				const copy = checkCopy(context);
				const root = stressProject(context);
				const state = path.join(context.work, "stress-state");
				fs.rmSync(state, { recursive: true, force: true });
				fs.mkdirSync(state, { recursive: true });
				const out = path.join(context.work, "batch-stress", "out");
				fs.rmSync(path.join(context.work, "batch-stress"), { recursive: true, force: true });
				fs.mkdirSync(out, { recursive: true });
				const parallel = [];
				for (let worker = 0; worker < STRESS.workers; worker++) {
					const scene = path.join(out, `scene-${worker}.json`);
					const lines = [];
					for (let job = worker; job < STRESS.jobs; job += STRESS.workers) {
						lines.push(JSON.stringify({ argv: ["engine", "--repo", copy, "--fixture", path.join(root, "fixtures", `Job${job}.luau`), "--roots", path.join(root, "roots"), "--no-snapshots", "--out", scene], env: { RUR_STATE_DIR: state } }));
					}
					const jobs = path.join(out, `jobs-${worker}.jsonl`);
					fs.writeFileSync(jobs, `${lines.join("\n")}\n`);
					parallel.push({ argv: [exe, "engine-batch"], cwd: copy, stdin: jobs, log: path.join(out, `worker-${worker}.log`) });
				}
				const plan = path.join(out, "plan.json");
				fs.writeFileSync(plan, JSON.stringify({ steps: [{ parallel }] }));
				return { argv: [context.node, context.fanout, plan], cwd: copy, env: hermetic(context, state), folders: folders(context, "batch-stress", state, out), out };
			},
			// every job answered with code 0 (a worker prints "@@RUR-BATCH {...}" per job). A job's env replaces every RUR_* variable,
			// so each names the state folder, as the golden check's do
			verify(prepared) {
				let passed = 0, total = 0;
				for (let worker = 0; worker < STRESS.workers; worker++) {
					for (const line of fs.readFileSync(path.join(prepared.out, `worker-${worker}.log`), "utf8").split("\n")) {
						if (!line.startsWith("@@RUR-BATCH ")) continue;
						total++;
						if (JSON.parse(line.slice(12)).code === 0) passed++;
					}
				}
				return passed === STRESS.jobs ? null : `${passed} of ${STRESS.jobs} batch jobs passed (${total} answered)`;
			},
		},
		goldenAll({
			id: "golden-all-cold", title: "Whole golden check, cold state",
			about: "`tools/engine/golden.mjs --all` with 12 batch workers on a new state folder: every corpus over the frozen project (1895 entries when measured), every module compiled and saved to the module store (about 40 MB).",
			fresh: true,
		}),
		goldenAll({
			id: "golden-all-warm", title: "Whole golden check, warm state",
			about: "The whole check again on the state folder the cold run left.",
			fresh: false, requires: ["golden-all-cold"],
		}),
	];
}
