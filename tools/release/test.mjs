// Tests of the release scripts: node --test tools/release/test.mjs
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { ROOT, isMain, version } from "./version.mjs";
import { crc32, inspect, packageRelease } from "./package.mjs";
import { legalFiles, licenses, normalPackages } from "./licenses.mjs";
import { canaryNotes, notes } from "./notes.mjs";
import { fileStates, outsideFileStates } from "./file-states.mjs";
// The choice of the dry run a tag publishes from has tests of its own, run with these
import "./dry-run.test.mjs";

// Runs `run` with an empty temporary folder and removes it afterwards
function temporary(run) {
	const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-release-test-"));
	try {
		return run(dir);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
}

// The four files version.mjs reads, all at 0.1.0
function writeManifests(dir) {
	fs.mkdirSync(path.join(dir, "native"), { recursive: true });
	fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify({ version: "0.1.0" }));
	fs.writeFileSync(path.join(dir, "package-lock.json"), JSON.stringify({ version: "0.1.0", packages: { "": { version: "0.1.0" } } }));
	fs.writeFileSync(path.join(dir, "native/Cargo.toml"), 'version = "0.1.0"\n');
	fs.writeFileSync(path.join(dir, "native/Cargo.lock"), 'name = "rur-native"\nversion = "0.1.0"\n');
}

// Lists the zip with a reader that is not ours (tar on Windows, unzip elsewhere)
function listWithSystemReader(zipFile) {
	if (process.platform === "win32") execFileSync(path.join(process.env.SystemRoot, "System32/tar.exe"), ["-tf", zipFile]);
	else execFileSync("unzip", ["-t", zipFile]);
}

test("version checks all manifests and rejects non-exact tags", () => temporary(dir => {
	writeManifests(dir);
	assert.equal(version(dir, "v0.1.0"), "0.1.0");
	assert.throws(() => version(dir, "v0.1.0-rc1"));
	fs.writeFileSync(path.join(dir, "native/Cargo.toml"), 'version = "0.2.0"\n');
	assert.throws(() => version(dir));
	assert.match(version(ROOT), /^\d+\.\d+\.\d+$/);
}));

test("release scripts start through a linked folder", () => temporary(dir => {
	const link = path.join(dir, "linked");
	fs.symlinkSync(ROOT, link, process.platform === "win32" ? "junction" : "dir");
	const result = execFileSync(process.execPath, [path.join(link, "tools/release/version.mjs")], { encoding: "utf8" });
	assert.equal(result.trim(), version(ROOT));
	// This file is the entry point of its own test run, so it is its own main
	assert.equal(isMain(import.meta.url), true);
}));

test("smoke file inventory excludes a sandbox through linked temporary roots", () => temporary(dir => {
	// The temporary folder can itself be a link (macOS), so start from its real location
	const base = fs.realpathSync(dir);
	const real = path.join(base, "real");
	const alias = path.join(base, "linked");
	const sandbox = path.join(real, "sandbox");
	fs.mkdirSync(sandbox, { recursive: true });
	fs.symlinkSync(real, alias, process.platform === "win32" ? "junction" : "dir");
	const watched = path.join(real, "watched.txt");
	fs.writeFileSync(watched, "before");

	// Reached through the link, the folder lists its files by their real paths, without the sandbox
	const before = fileStates([alias], sandbox);
	assert.deepEqual([...before.keys()], [real, watched]);
	// A root named twice, and a sandbox named through the link, change nothing
	assert.deepEqual(fileStates([real, alias], path.join(alias, "sandbox")), before);
	// What the run writes into the sandbox is not a change
	fs.writeFileSync(path.join(sandbox, "capture.png"), "new capture");
	assert.deepEqual(fileStates([alias], sandbox), before);
	// Changes outside the sandbox must still be detected
	fs.writeFileSync(watched, "changed outside the sandbox");
	assert.notDeepEqual(fileStates([alias], sandbox), before);
}));

test("smoke watches inputs and real rur locations without walking runner bookkeeping", t => temporary(dir => {
	// A made-up runner. Each real location is its own folder, so a write is traced to the one it hit.
	const base = fs.realpathSync(dir);
	const folder = name => {
		const file = path.join(base, name);
		fs.mkdirSync(file, { recursive: true });
		return file;
	};
	const checkout = folder("checkout");
	const workspace = folder("workspace");
	const home = folder("home"); // HOME
	const profile = folder("profile"); // USERPROFILE
	const systemHome = folder("system-home"); // what os.homedir() returns
	const local = folder("local");
	const xdg = folder("xdg");
	const temp = folder("temp");
	const otherTemp = folder("other-temp");
	const sandbox = folder("temp/rur-release-smoke-test");
	const bin = folder("temp/rur-release-smoke-test/bin");
	const project = folder("temp/rur-release-smoke-test/project");
	const runnerLogs = folder("home/runners/version/_diag");
	const unrelatedCache = folder("local/another-tool");
	const env = { HOME: home, USERPROFILE: profile, LOCALAPPDATA: local, XDG_CACHE_HOME: xdg, TMPDIR: otherTemp, TMP: temp, TEMP: temp };
	const inputs = [checkout, workspace, bin, project];
	const snapshot = () => outsideFileStates(inputs, sandbox, { env, home: systemHome, temp });
	const before = snapshot();

	// Reading either noisy folder recursively would fail, even if none of its files changed.
	const readDirectory = fs.readdirSync;
	t.mock.method(fs, "readdirSync", (file, ...args) => {
		assert.ok(![runnerLogs, path.dirname(runnerLogs), unrelatedCache].includes(file), `walked unrelated folder: ${file}`);
		return readDirectory(file, ...args);
	});
	fs.writeFileSync(path.join(runnerLogs, "Worker.log"), "runner bookkeeping changed");
	fs.writeFileSync(path.join(unrelatedCache, "state"), "another tool wrote here");
	fs.writeFileSync(path.join(temp, "runner.log"), "unrelated temporary file");
	fs.writeFileSync(path.join(sandbox, "output.png"), "allowed output");
	assert.deepEqual(snapshot(), before);

	// A file written in `root` must show up as new, then as changed, then as removed
	function assertFileChangesAreSeen(root) {
		const previous = snapshot();
		fs.mkdirSync(root, { recursive: true });
		const file = path.join(root, "state");
		fs.writeFileSync(file, "first");
		const created = snapshot();
		assert.notDeepEqual(created, previous, root);
		assert.ok(created.has(file), root);
		fs.writeFileSync(file, "changed contents");
		const changed = snapshot();
		assert.notDeepEqual(changed, created, root);
		fs.unlinkSync(file);
		assert.notDeepEqual(snapshot(), changed, root);
	}

	// An empty folder, then a file, directly in `root` must show up and leave no trace once removed.
	// In a temporary folder only an entry with a rur name counts.
	function assertStrayEntriesAreSeen(root) {
		const [folderName, fileName] = root === temp ? ["rur-empty", "rur-spool.txt"] : [".stray-folder", ".stray-file"];
		const previous = snapshot();
		const empty = path.join(root, folderName);
		fs.mkdirSync(empty);
		assert.notDeepEqual(snapshot(), previous);
		assert.equal(snapshot().get(empty), "directory");
		fs.rmdirSync(empty);
		assert.deepEqual(snapshot(), previous);
		const file = path.join(root, fileName);
		fs.writeFileSync(file, "stray output");
		assert.notDeepEqual(snapshot(), previous);
		fs.unlinkSync(file);
		assert.deepEqual(snapshot(), previous);
	}

	// Every input root and each real fallback cache must catch a file's creation, change and removal
	const caches = [
		...[home, profile, systemHome].flatMap(file => [path.join(file, ".cache/rur"), path.join(file, "Library/Caches/rur")]),
		path.join(local, "rur"), path.join(xdg, "rur"),
	];
	const rurTemporaryEntries = [path.join(temp, "rur-spool"), path.join(otherTemp, "RUR-worker")];
	for (const root of [...inputs, ...caches, ...rurTemporaryEntries]) assertFileChangesAreSeen(root);
	// Each real home, and the temporary folder, also catch entries made directly in them
	for (const root of [home, profile, systemHome, temp]) assertStrayEntriesAreSeen(root);
}));

test("an entry point that cannot be resolved is compared as written", () => temporary(dir => {
	const original = process.argv[1];
	try {
		// The file does not exist, so only the identical path matches
		process.argv[1] = path.join(dir, "missing.mjs");
		assert.equal(isMain(pathToFileURL(process.argv[1])), true);
		assert.equal(isMain(import.meta.url), false);
	} finally {
		process.argv[1] = original;
	}
}));

test("smoke ignores only Windows profile bookkeeping in the real homes", t => temporary(dir => {
	const base = fs.realpathSync(dir);
	const homes = ["home", "profile", "system-home"].map(name => path.join(base, name));
	const sandbox = path.join(base, "sandbox");
	const temp = path.join(base, "temp");
	for (const folder of [...homes, sandbox, temp]) fs.mkdirSync(folder);
	const env = { HOME: homes[0], USERPROFILE: homes[1] };
	const snapshot = () => outsideFileStates([], sandbox, { env, home: homes[2], temp, platform: "win32" });
	const before = snapshot();
	const hive = "NtUsEr.DaT{00000000-0000-0000-0000-000000abcdef}";
	const names = [
		"NTUSER.DAT", "ntuser.dat.LOG", "ntuser.dat.LOG1", "ntuser.dat.LOG2", "NtUsEr.InI",
		`${hive}.TM.blf`, `${hive}.TMContainer00000000000000000001.regtrans-ms`,
		`${hive}.tmcontainer00000000000000000002.REGTRANS-MS`,
	];
	const bookkeeping = new Set(homes.flatMap(home => names.map(name => path.join(home, name))));
	// A hive can also be locked by Windows. The inventory must skip it before trying to stat it.
	const stat = fs.statSync;
	t.mock.method(fs, "statSync", (file, ...args) => {
		assert.ok(!bookkeeping.has(file), "read Windows profile bookkeeping");
		return stat(file, ...args);
	});
	for (const file of bookkeeping) fs.writeFileSync(file, "registry created");
	assert.deepEqual(snapshot(), before);
	for (const file of bookkeeping) fs.appendFileSync(file, " and flushed");
	assert.deepEqual(snapshot(), before);
	for (const file of bookkeeping) fs.unlinkSync(file);
	assert.deepEqual(snapshot(), before);

	// Similar names and arbitrary log files still count, in every home, including changes/removals.
	const watchedNames = [
		".stray-file", "ntuser.dat.extra", "ntuser.dat.LOG3", "other.blf", "other.regtrans-ms",
		"ntuser.dat{not-a-guid}.TM.blf", `${hive}.TMContainer1.regtrans-ms`, `${hive}.TM.blf.extra`,
	];
	for (const home of homes) {
		for (const name of watchedNames) {
			const file = path.join(home, name);
			fs.writeFileSync(file, "stray output");
			const created = snapshot();
			assert.notDeepEqual(created, before, name);
			fs.appendFileSync(file, " changed");
			const changed = snapshot();
			assert.notDeepEqual(changed, created, name);
			fs.unlinkSync(file);
			assert.notDeepEqual(snapshot(), changed, name);
			assert.deepEqual(snapshot(), before);
		}
		// Even an exact registry name is suspicious when it is a directory.
		const folder = path.join(home, "NTUSER.DAT");
		bookkeeping.delete(folder);
		fs.mkdirSync(folder);
		assert.equal(snapshot().get(folder), "directory");
		fs.rmdirSync(folder);
	}
}));

test("smoke keeps registry filenames watched on other platforms and in recursive roots", () => temporary(dir => {
	const base = fs.realpathSync(dir);
	const sandbox = path.join(base, "sandbox");
	const home = path.join(base, "home");
	const temp = path.join(base, "temp");
	const input = path.join(base, "input");
	const cache = path.join(home, ".cache/rur");
	const spool = path.join(temp, "rur-spool");
	for (const folder of [sandbox, home, temp, input, cache, spool]) fs.mkdirSync(folder, { recursive: true });
	for (const [platform, roots, folder] of [
		["linux", [], home], ["darwin", [], home],
		["win32", [input], input], ["win32", [], cache], ["win32", [], spool], ["win32", [home], home],
	]) {
		const snapshot = () => outsideFileStates(roots, sandbox, { env: {}, home, temp, platform });
		const before = snapshot();
		const file = path.join(folder, "NTUSER.DAT");
		fs.writeFileSync(file, "watched contents");
		assert.notDeepEqual(snapshot(), before);
		fs.unlinkSync(file);
	}
}));

// Each hook removes an entry exactly when the inventory reads it, without depending on timing.
for (const [name, method, relative, directory] of [
	["listed input file", "statSync", "input/state", false],
	["listed home file", "statSync", "home/state", false],
	["listed input directory", "statSync", "input/empty", true],
	["listed home directory", "statSync", "home/empty", true],
	["input root before stat", "statSync", "input", true],
	["input root before resolution", "realpathSync", "input", true],
	["input directory before listing", "readdirSync", "input/empty", true],
	["input root before listing", "readdirSync", "input", true],
	["home root before resolution", "realpathSync", "home", true],
	["home root before listing", "readdirSync", "home", true],
	["temporary entry before resolution", "realpathSync", "temp/rur-entry", false],
	["temporary root before listing", "readdirSync", "temp", true],
	["cache root before stat", "statSync", "home/.cache/rur", true],
]) {
	test(`smoke treats a vanished ${name} as absent and preserves other errors`, t => temporary(dir => {
		const base = fs.realpathSync(dir);
		const sandbox = path.join(base, "sandbox");
		const input = path.join(base, "input");
		const home = path.join(base, "home");
		const temp = path.join(base, "temp");
		for (const folder of [sandbox, input, home, temp]) fs.mkdirSync(folder);
		const file = path.join(base, relative);
		function createEntry() {
			if (directory) fs.mkdirSync(file, { recursive: true });
			else fs.writeFileSync(file, "temporary contents");
		}
		createEntry();
		const read = fs[method];
		let removed = false;
		let failure;
		t.mock.method(fs, method, (target, ...args) => {
			if (target === file) {
				if (failure) throw failure;
				if (!removed) {
					if (directory) fs.rmdirSync(file);
					else fs.unlinkSync(file);
					removed = true;
				}
			}
			return read(target, ...args);
		});
		const snapshot = () => outsideFileStates([input], sandbox, { env: {}, home, temp, platform: "linux" });
		assert.ok(!snapshot().has(file), "the vanished entry must not leave a directory marker either");
		assert.ok(removed, "the deterministic removal hook ran");
		createEntry();
		for (const code of ["EACCES", "EIO", "ENOTDIR"]) {
			failure = Object.assign(new Error("inventory read failed"), { code });
			assert.throws(snapshot, error => error === failure);
		}
	}));
}

test("installer ZIP is flat, binary first, only one executable, with license and exact source", () => temporary(dir => {
	assert.equal(crc32(Buffer.from("123456789")), 0xcbf43926); // the standard CRC-32 check value
	const binary = path.join(dir, "binary");
	const licenseFile = path.join(dir, "legal");
	fs.writeFileSync(binary, "MZtest-binary");
	fs.writeFileSync(licenseFile, "test license text");

	for (const [platform, arch] of [["windows", "x86_64"], ["linux", "x86_64"], ["macos", "aarch64"]]) {
		const zipFile = packageRelease({ binary, os: platform, arch, out: dir, licenses: licenseFile, commit: "a".repeat(40) });
		const entries = inspect(fs.readFileSync(zipFile));
		assert.equal(path.basename(zipFile), `rur-native-${version(ROOT)}-${platform}-${arch}.zip`);
		const executable = "rur-native" + (platform === "windows" ? ".exe" : "");
		assert.deepEqual(entries.map(entry => entry.name), [executable, "LICENSE", "LICENSE-GPL-3.0.txt", "NOTICE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_LICENSES.txt", "SOURCE.md"]);
		assert.equal(entries[0].offset, 0);
		assert.equal(entries[0].mode & 0o777, 0o755);
		assert.ok(entries.slice(1).every(entry => (entry.mode & 0o111) === 0));
		assert.equal(entries[0].bytes.toString(), "MZtest-binary");
		assert.ok(entries.at(-1).bytes.includes(Buffer.from("a".repeat(40))));
		assert.ok(entries.at(-1).bytes.includes(Buffer.from("RUR_RELEASE_BUILD=1")));
		// An independent reader checks the central directory and the payloads we wrote
		listWithSystemReader(zipFile);
	}
}));

test("license graph excludes dev and build edges, includes root texts and a standard appendix", () => temporary(dir => {
	// rur-native depends on "normal" (which depends on "missing-file"), builds with "build-only"
	// and tests with "dev-only". "missing-file" has no license file, so its MIT text comes from the appendix.
	const names = ["rur-native", "normal", "build-only", "dev-only", "missing-file"];
	const packages = names.map(name => {
		const folder = path.join(dir, name);
		fs.mkdirSync(folder);
		if (name !== "missing-file") fs.writeFileSync(path.join(folder, "LICENSE"), "Permission is hereby granted, free of charge, to test this license.");
		return { id: name, name, version: "1.0.0", license: "MIT", manifest_path: path.join(folder, "Cargo.toml"), source: null };
	});
	const dependency = (pkg, kind) => ({ pkg, name: pkg, dep_kinds: [{ kind, target: null }] });
	const dependencies = {
		"rur-native": [dependency("normal", null), dependency("build-only", "build"), dependency("dev-only", "dev")],
		"normal": [dependency("missing-file", null)],
	};
	for (const pkg of packages) pkg.dependencies = (dependencies[pkg.id] ?? []).map(edge => ({
		name: edge.pkg, kind: edge.dep_kinds[0].kind, target: null, optional: false,
	}));
	const metadata = {
		packages,
		workspace_members: ["rur-native"],
		resolve: { nodes: packages.map(pkg => ({ id: pkg.id, deps: dependencies[pkg.id] ?? [] })) },
	};

	assert.deepEqual(normalPackages(metadata).map(pkg => pkg.name), ["missing-file", "normal", "rur-native"]);
	const output = licenses(metadata, { root: dir });
	assert.match(output, /Standard license appendix: MIT/);
	assert.doesNotMatch(output, /build-only|dev-only/);

	// A license with no standard text anywhere in the graph is an error
	packages[4].license = "Unknown-License";
	assert.throws(() => licenses(metadata, { root: dir }), /no license text/);
}));

test("release notes use literal public subjects since the previous tag", () => temporary(dir => {
	writeManifests(dir);
	const git = args => execFileSync("git", args, { cwd: dir, encoding: "utf8" });
	git(["init", "-q"]);
	git(["config", "user.name", "Release Test"]);
	git(["config", "user.email", "release-test"]);
	git(["add", "."]);
	git(["commit", "-q", "-m", "First public change"]);
	assert.match(notes(dir, "v0.1.0"), /First public change/);

	git(["tag", "v0.0.9"]);
	git(["commit", "-q", "--allow-empty", "-m", "Literal $(never-run) `text`"]);
	const output = notes(dir, "v0.1.0");
	assert.match(output, /Literal \$\(never-run\) `text`/);
	assert.doesNotMatch(output, /First public change/);
	assert.match(output, /gh attestation verify/);

	// The canary tag on a later commit is never taken for the previous release
	git(["tag", "canary"]);
	git(["commit", "-q", "--allow-empty", "-m", "After the canary"]);
	assert.match(notes(dir, "v0.1.0"), /\n- After the canary\n- Literal \$\(never-run\) `text`\n\n/);
}));

test("canary notes link the commit and the comparison with the latest version release", () => temporary(dir => {
	writeManifests(dir);
	const git = args => execFileSync("git", args, { cwd: dir, encoding: "utf8" }).trim();
	git(["init", "-q"]);
	git(["config", "user.name", "Release Test"]);
	git(["config", "user.email", "release-test"]);
	git(["add", "."]);
	git(["commit", "-q", "-m", "Before any release"]);
	const built = { runId: "123", runDate: "2026-11-15T01:51:09Z" };

	// No version release yet: every commit is listed, with no comparison
	let output = canaryNotes(dir, built);
	assert.match(output, /1 commit, before the first version release:\n\n- Before any release\n/);
	assert.doesNotMatch(output, /compare/);

	// The newest version tag the commit contains is the latest release; the canary tag and a
	// later or non-version tag are not
	git(["tag", "v0.0.9"]);
	git(["commit", "-q", "--allow-empty", "-m", "Released"]);
	git(["tag", "v0.1.0"]);
	git(["commit", "-q", "--allow-empty", "-m", "First change on main"]);
	git(["tag", "v0.2.0-rc1"]);
	git(["commit", "-q", "--allow-empty", "-m", "Literal $(never-run) `text`"]);
	git(["tag", "canary"]);
	const commit = git(["rev-parse", "HEAD"]);
	output = canaryNotes(dir, built);
	const repository = "https://github.com/Anime-Vanguards/roblox-ui-renderer";
	assert.match(output, new RegExp(`^This is a build of main, not a release, and it may break: for a release, see \\[Releases\\]\\(${repository}/releases\\)\\.$`, "m"));
	assert.ok(output.includes(`Commit: [${commit.slice(0, 10)}](${repository}/commit/${commit}) on main`));
	assert.ok(output.includes(`Built: 2026-11-15 01:51 UTC, by [release run 123](${repository}/actions/runs/123)`));
	assert.ok(output.includes(`2 commits since v0.1.0 ([compare](${repository}/compare/v0.1.0...${commit})):\n\n- Literal $(never-run) \`text\`\n- First change on main\n`));
	assert.doesNotMatch(output, /Released|Before any release/);
	assert.match(output, /releases\/download\/canary\/rur-native-canary-windows-x86_64\.zip/);
	assert.match(output, /freetype-2\.14\.2\.tar\.gz/);

	// On the released commit itself there is nothing to compare
	git(["checkout", "-q", "v0.1.0"]);
	assert.match(canaryNotes(dir, built), /No commits since v0\.1\.0: this is the commit it was released from\./);

	// Without the run that built the zips there are no notes
	assert.throws(() => canaryNotes(dir, { runId: "", runDate: built.runDate }), /--run/);
	assert.throws(() => canaryNotes(dir, { runId: "123", runDate: "" }), /--run-date/);
}));

test("canary notes list at most 100 subjects and count the rest", () => temporary(dir => {
	writeManifests(dir);
	const git = args => execFileSync("git", args, { cwd: dir, encoding: "utf8" }).trim();
	git(["init", "-q"]);
	git(["config", "user.name", "Release Test"]);
	git(["config", "user.email", "release-test"]);
	git(["add", "."]);
	git(["commit", "-q", "-m", "Released"]);
	git(["tag", "v0.1.0"]);
	for (let i = 1; i <= 103; i++) git(["commit", "-q", "--allow-empty", "-m", `Change ${i}`]);
	const output = canaryNotes(dir, { runId: "1", runDate: "2026-11-15T01:51:09Z" });
	assert.match(output, /103 commits since v0\.1\.0/);
	assert.match(output, /- Change 103\n/);
	assert.doesNotMatch(output, /- Change 3\n/);
	assert.match(output, /- Change 4\n- and 3 more\n/);
}));

test("the canary tag publishes the passing run's zips under fixed names and never builds or moves a tag", () => {
	const workflow = fs.readFileSync(path.join(ROOT, ".github/workflows/release.yml"), "utf8").replace(/\r\n/g, "\n");
	const job = name => workflow.split(new RegExp(`^  ${name}:\\n`, "m"))[1].split(/^  \S/m)[0];
	// Triggers: the canary tag besides version tags, and the nightly run
	assert.match(workflow, /^    tags: \['v\[0-9\]\+\.\[0-9\]\+\.\[0-9\]\+', canary\]$/m);
	assert.match(workflow, /^  schedule:\n    - cron: "51 1 \* \* \*"$/m);
	// Nightly and canary runs have groups of their own, and nothing cancels a run in progress
	assert.match(workflow, /^  group: release-\$\{\{ github.event_name == 'schedule' && 'nightly' \|\| github.event_name == 'push' && github.ref == 'refs\/tags\/canary' && 'canary' \|\| github.event_name == 'push' && github.ref_type == 'branch' && github.ref \|\| github.run_id \}\}\n  cancel-in-progress: false$/m);
	// The version check of a tag skips the canary, which is not a version
	assert.match(job("version"), /if \[ "\$GITHUB_REF_NAME" = canary \]; then\n\s+# [^\n]+\n\s+VERSION=\$\(node tools\/release\/version.mjs\)\n\s+else\n\s+# [^\n]+\n\s+VERSION=\$\(node tools\/release\/version.mjs --tag "\$GITHUB_REF_NAME"\)/);
	// A version release is never published for the canary tag
	assert.match(job("publish"), /^    if: \$\{\{ !cancelled\(\) && !github.event.repository.private && github.event_name == 'push' && github.ref_type == 'tag' && github.ref != 'refs\/tags\/canary' && /);

	const canary = job("canary");
	assert.match(canary, /^    if: \$\{\{ [^\n]*github.ref == 'refs\/tags\/canary' && needs.version.result == 'success' && needs.notes.result == 'success' && needs.version.outputs.dry-run != '' \}\}$/m);
	assert.match(canary, /^    needs: \[version, notes\]$/m);
	assert.match(canary, /^    permissions:\n      contents: write\n      id-token: write\n      attestations: write\n      actions: read[^\n]*\n    env:/m);
	// The zips are attested, as publish does
	assert.match(canary, /uses: actions\/attest-build-provenance@[0-9a-f]{40} [^\n]*\n        with:\n          subject-path: assets\/\*\.zip$/m);
	// Nothing from the repository runs there, and every zip comes from the passing run
	assert.doesNotMatch(canary, /actions\/checkout|node |tools\/|npm |cargo /);
	assert.equal(canary.match(/run-id: \$\{\{ needs.version.outputs.dry-run \}\}/g).length, 2);
	assert.doesNotMatch(canary, /upload-artifact/);
	// Fixed asset names
	assert.match(canary, /for platform in windows-x86_64 linux-x86_64 macos-aarch64; do\n\s+mv -- "built\/rur-native-\$VERSION-\$platform.zip" "assets\/rur-native-canary-\$platform.zip"/);
	assert.match(canary, /mv -- built\/freetype-2\.14\.2\.tar\.gz assets\/\n\s+rmdir built\n/);
	// A prerelease, never latest, created only for the existing tag; never a target or a tag change
	assert.match(canary, /gh release create canary -R "\$GITHUB_REPOSITORY" --verify-tag --prerelease --latest=false --title "\$title"/);
	assert.match(canary, /gh release edit canary -R "\$GITHUB_REPOSITORY" --prerelease --latest=false --title "\$title"/);
	assert.match(canary, /title="Canary \(main \$\{GITHUB_SHA:0:10\}\)"/);
	assert.doesNotMatch(canary, /--target|--tag |git push|git tag|refs\/tags\/canary"? -/);
	// Upload first, then delete what the new set no longer has, then the title and notes
	const order = ["gh release upload canary -R \"$GITHUB_REPOSITORY\" --clobber assets/*", "gh release delete-asset", "gh release edit"].map(text => canary.indexOf(text));
	assert.ok(order.every(index => index > 0) && order[0] < order[1] && order[1] < order[2], order.join());
	// The workflow itself never writes a ref (cargo's --target, a platform, is not a release target)
	assert.doesNotMatch(workflow, /git push|git tag |git\/refs|target_commitish|gh release [^\n]*--target/);
});

test("license graph follows only enabled optional normal dependencies", () => {
	// rur-native declares seven dependencies; its default feature switches on some of the optional ones:
	//   always        normal, not optional
	//   enabled       optional and renamed "optional-alias", switched on by `dep:optional-alias`
	//   configured    optional, switched on by `configured/fast`
	//   inactive      optional, no feature names it
	//   weak          optional, named only by `weak?/fast`, which does not switch it on
	//   dev-only      a dev dependency
	//   other-target  a normal dependency declared for another target than its edge
	const declaration = (name, optional = false, more = {}) => ({ name, kind: null, target: null, optional, ...more });
	const resolvedEdge = (pkg, name = pkg, kind = null, target = null) => ({ pkg, name, dep_kinds: [{ kind, target }] });
	const packages = ["rur-native", "always", "enabled", "configured", "inactive", "weak", "dev-only", "other-target"].map(name => ({
		id: name, name, version: "1.0.0", dependencies: [],
	}));
	const root = packages[0];
	root.dependencies = [
		declaration("always"),
		declaration("enabled", true, { rename: "optional-alias" }),
		declaration("configured", true),
		declaration("inactive", true),
		declaration("weak", true),
		declaration("dev-only", false, { kind: "dev" }),
		declaration("other-target", false, { target: "cfg(unix)" }),
	];
	// "default" names "nested", which names "default" again
	root.features = { default: ["nested"], nested: ["default", "dep:optional-alias", "configured/fast", "weak?/fast"] };
	const rootEdges = [
		resolvedEdge("always"), resolvedEdge("enabled", "optional_alias"), resolvedEdge("configured"),
		resolvedEdge("inactive"), resolvedEdge("weak"),
		resolvedEdge("dev-only", "dev_only", "dev"), resolvedEdge("other-target", "other_target"),
	];
	const metadata = {
		packages,
		workspace_members: [root.id],
		resolve: { nodes: packages.map(pkg => ({
			id: pkg.id,
			features: pkg === root ? ["default"] : [],
			deps: pkg === root ? rootEdges : [],
		})) },
	};
	assert.deepEqual(normalPackages(metadata).map(pkg => pkg.name), ["always", "configured", "enabled", "rur-native"]);

	// A plain `dep:weak` switches the weak dependency on
	root.features.nested.push("dep:weak");
	assert.ok(normalPackages(metadata).some(pkg => pkg.name === "weak"));
});

// Makes any `localeCompare` call fail the test, so only a code unit comparison can pass it
const forbidLocaleCompare = t => t.mock.method(String.prototype, "localeCompare", () => { throw new Error("locale-dependent sort"); });

test("license files are ordered by code units regardless of locale", t => temporary(dir => {
	// Locale collation orders these differently: case and punctuation sort between letters
	const files = ["LICENSE-Z", "LICENSE-a", "LICENSE_A"];
	for (const name of files) fs.writeFileSync(path.join(dir, name), name);
	forbidLocaleCompare(t);
	assert.deepEqual(legalFiles(dir).map(file => path.basename(file)), files);
}));

test("packages are ordered by code units regardless of locale", t => {
	// Package 0 is the root. Packages 1 and 2 are its dependencies and are named like it ("a-b"
	// sorts before "a_b" by code units); 1 and 2 differ only in their versions.
	const packages = [
		{ id: "0", name: "a_b", version: "1.0.0+Z" },
		{ id: "1", name: "a-b", version: "1.0.0+a" },
		{ id: "2", name: "a-b", version: "1.0.0+Z" },
	].map(pkg => ({ ...pkg, dependencies: [] }));
	packages[0].dependencies = packages.slice(1).map(pkg => ({ name: pkg.name, kind: null, target: null, optional: false }));
	const rootEdges = packages.slice(1).map(pkg => ({ pkg: pkg.id, name: pkg.name, dep_kinds: [{ kind: null, target: null }] }));
	const metadata = {
		packages,
		workspace_members: ["0"],
		resolve: { nodes: packages.map(pkg => ({ id: pkg.id, deps: pkg.id === "0" ? rootEdges : [] })) },
	};
	forbidLocaleCompare(t);
	assert.deepEqual(normalPackages(metadata, "a_b").map(pkg => pkg.id), ["2", "1", "0"]);
});

test("the license appendix is ordered by code units regardless of locale", t => temporary(dir => {
	// The root needs the standard texts of bzip2 and MIT. The donors supply them without being
	// linked dependencies.
	const donorDir = path.join(dir, "donor");
	fs.mkdirSync(donorDir);
	fs.writeFileSync(path.join(donorDir, "LICENSE"), "test text");
	const empty = path.join(dir, "empty");
	fs.mkdirSync(empty);
	const donors = ["bzip2", "MIT"].map(license => ({ name: license, license, manifest_path: path.join(donorDir, "Cargo.toml") }));
	forbidLocaleCompare(t);
	const output = licenses(metadataWithRoot(empty, "bzip2 AND MIT", donors), { root: dir });
	assert.ok(output.indexOf("appendix: MIT") < output.indexOf("appendix: bzip2"));
}));

// Cargo metadata for a workspace of the one package rur-native, whose folder is `dir`, plus any
// `others` that it does not depend on (they only supply license texts)
function metadataWithRoot(dir, license, others = []) {
	const root = { id: "root", name: "rur-native", version: "1.0.0", license, manifest_path: path.join(dir, "Cargo.toml") };
	return { packages: [root, ...others], workspace_members: [root.id], resolve: { nodes: [{ id: root.id, deps: [] }] } };
}

test("vendored license ordering does not depend on directory enumeration", t => temporary(dir => {
	for (const name of ["vendor-a", "vendor-z"]) {
		fs.mkdirSync(path.join(dir, name));
		fs.writeFileSync(path.join(dir, name, "LICENSE"), name);
	}
	fs.writeFileSync(path.join(dir, "LICENSE"), "root");
	const metadata = metadataWithRoot(dir, "MIT");

	// Make the file system list entries in the order asked for, whatever `readdirSync` would give
	const readDirectory = fs.readdirSync;
	let direction = 1;
	t.mock.method(fs, "readdirSync", (...args) => readDirectory(...args).sort((a, b) => direction * (a.name < b.name ? -1 : 1)));
	const listedAscending = licenses(metadata, { root: dir });
	direction = -1;
	const listedDescending = licenses(metadata, { root: dir });
	assert.equal(listedDescending, listedAscending);
}));

test("an unreadable standard license fails instead of trying another license", t => temporary(dir => {
	// The root's "Unknown-License OR MIT" has no text of its own, so the MIT text comes from the donor
	const donorDir = path.join(dir, "donor");
	fs.mkdirSync(donorDir);
	const donorLicense = path.join(donorDir, "LICENSE");
	fs.writeFileSync(donorLicense, "Permission is hereby granted, free of charge");
	const donor = { id: "donor", name: "donor", license: "MIT", manifest_path: path.join(donorDir, "Cargo.toml") };
	const metadata = metadataWithRoot(dir, "Unknown-License OR MIT", [donor]);

	// Reading the donor's license file fails, and that error must reach the caller
	const readFile = fs.readFileSync;
	t.mock.method(fs, "readFileSync", (file, ...args) => {
		if (file === donorLicense) throw Object.assign(new Error("license access denied"), { code: "EACCES" });
		return readFile(file, ...args);
	});
	assert.throws(() => licenses(metadata, { root: dir }), { code: "EACCES" });
}));

// Needs the real `cargo metadata` output (see tools/release/README.md); no build
test("real target metadata self-check, when supplied", { skip: !process.env.RUR_RELEASE_METADATA }, () => {
	const metadata = JSON.parse(fs.readFileSync(process.env.RUR_RELEASE_METADATA, "utf8"));
	const output = licenses(metadata, { freetype: process.env.RUR_FREETYPE_SRC });
	assert.ok(output.includes(`rur-native ${version(ROOT)}`));
	assert.match(output, /FreeType 2\.14\.2/);
	assert.match(output, /luau\/LICENSE/);
	assert.match(output, /epaint_default_fonts/);
	console.log(`real target licenses: ${normalPackages(metadata).length} packages, ${Buffer.byteLength(output)} bytes`);
});
