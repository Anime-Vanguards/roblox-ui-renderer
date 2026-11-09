// The smoke test's file inventory: file sizes and modification times, and directory names.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

// What the inventory records for a directory. Its modification time would change whenever a child is
// written, so a directory only counts as present, which still lets an empty one be noticed.
const DIRECTORY = "directory";

function fileState(stat) {
	return `${stat.size}:${stat.mtimeMs}`;
}

// Runs `read` and returns its result, or nothing when the entry is gone. Other processes on the
// machine (a temporary-file cleanup, the runner) can remove an entry after it was listed and before
// it is resolved, read or listed again. That is not a failure: a snapshot taken a moment later
// would simply not have it, so it is absent from this one. Only ENOENT means that; permission and
// I/O errors still throw, because swallowing them could hide a file the run wrote.
function ifPresent(read) {
	try {
		return read();
	} catch (error) {
		if (error.code !== "ENOENT") throw error;
	}
}

// Returns a map from each file under `roots` to its "size:mtime", leaving out symbolic links and
// the `sandbox` folder, with each folder marked as a directory. A root that is missing or unset is
// skipped, and so is a folder reached twice, for example as two roots that name the same place. A
// root inside the sandbox is still read: only the sandbox met while walking another root is skipped.
// A root may also be a single file. `shallowRoots` are listed one level deep, without descending
// into their folders. `ignoreShallowFile` applies only to files listed in those roots.
//
// Roots and the sandbox are both resolved to their real locations, because temporary folders can
// be reached through links (/var is a link to /private/var on macOS, and /tmp can be one on
// Linux). Compared as written, a sandbox named by its link would never equal the walked path.
export function fileStates(roots, sandbox, shallowRoots = [], ignoreShallowFile) {
	const excludedSandbox = fs.realpathSync(sandbox);
	const walkedFolders = new Set();
	const stateByFile = new Map();

	// Records what is directly inside `folder`, and calls `descend` with each folder found
	function recordEntries(folder, descend) {
		const entries = ifPresent(() => fs.readdirSync(folder, { withFileTypes: true }));
		if (!entries) {
			// The folder vanished after it was marked as a directory, so it is absent, not empty
			stateByFile.delete(folder);
			return;
		}
		const isShallowListing = !descend;
		for (const entry of entries) {
			const file = path.join(folder, entry.name);
			if (file === excludedSandbox || entry.isSymbolicLink()) continue;
			if (!entry.isDirectory() && !entry.isFile()) continue;
			// Skipped before the stat below, because Windows can hold such a file locked. Only files
			// are skipped: a folder with the same name is not the OS's file and stays watched.
			if (isShallowListing && entry.isFile() && ignoreShallowFile?.(entry.name)) continue;
			// The entry's own stat, not the listing's type, which may be stale by now
			const stat = ifPresent(() => fs.statSync(file));
			if (!stat) continue;
			if (stat.isDirectory()) {
				stateByFile.set(file, DIRECTORY);
				descend?.(file);
			} else if (stat.isFile()) {
				stateByFile.set(file, fileState(stat));
			}
		}
	}

	function walk(folder) {
		if (walkedFolders.has(folder)) return;
		walkedFolders.add(folder);
		recordEntries(folder, walk);
	}

	// The real path of `root`, or nothing when it is unset or missing
	function existingRoot(root) {
		return root ? ifPresent(() => fs.realpathSync(root)) : undefined;
	}

	for (const root of roots) {
		const file = existingRoot(root);
		if (!file) continue;
		const stat = ifPresent(() => fs.statSync(file));
		if (!stat) continue;
		if (stat.isDirectory()) {
			stateByFile.set(file, DIRECTORY);
			walk(file);
		} else if (stat.isFile()) {
			stateByFile.set(file, fileState(stat));
		}
	}
	for (const root of shallowRoots) {
		const folder = existingRoot(root);
		if (folder) recordEntries(folder);
	}
	return stateByFile;
}

// The folders where rur keeps its cache when its redirection does not apply: the per-user cache
// location of each platform, under every home folder and the cache variables.
function realCacheFolders(homes, env) {
	const caches = homes.flatMap(home => [path.join(home, ".cache/rur"), path.join(home, "Library/Caches/rur")]);
	for (const folder of [env.XDG_CACHE_HOME, env.LOCALAPPDATA]) {
		if (folder) caches.push(path.join(folder, "rur"));
	}
	return caches;
}

// The entries named "rur…" (any case) directly in the real temporary folders, other than the sandbox
// itself, which the run writes to. Symbolic links are left out, as they are while walking.
function rurTemporaryEntries(sandbox, env, temp) {
	const excludedSandbox = fs.realpathSync(sandbox);
	const entries = [];
	for (const folder of new Set([temp, env.TMPDIR, env.TMP, env.TEMP].filter(Boolean))) {
		const listed = ifPresent(() => fs.readdirSync(folder, { withFileTypes: true })) ?? [];
		for (const entry of listed) {
			// An entry that vanished before it could be resolved is not part of the inventory
			if (entry.isSymbolicLink() || !/^rur/i.test(entry.name)) continue;
			const file = path.join(folder, entry.name);
			const real = ifPresent(() => fs.realpathSync(file));
			if (real && real !== excludedSandbox) entries.push(file);
		}
	}
	return entries;
}

// The user registry hive and its recovery logs and profile metadata: ntuser.dat, ntuser.dat.LOG,
// .LOG1, .LOG2 and ntuser.ini
const HIVE_FILE = /^ntuser\.(?:ini|dat(?:\.log[12]?)?)$/i;

// The hive's transaction files: ntuser.dat{<guid>}.TM.blf and
// ntuser.dat{<guid>}.TMContainer<20 digits>.regtrans-ms
const GUID = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";
const HIVE_TRANSACTION_FILE = new RegExp(
	`^ntuser\\.dat\\{${GUID}\\}\\.(?:tm\\.blf|tmcontainer[0-9]{20}\\.regtrans-ms)$`,
	"i",
);

// Whether `name` is a file Windows keeps in the user profile and updates itself: the registry flushes
// them lazily, so they can change during the run even when the executable wrote nothing, and a
// correct binary would fail the smoke. The names are matched exactly, not by prefix or by a log
// extension, so that a stray file with a similar name is still noticed.
function windowsProfileFile(name) {
	return HIVE_FILE.test(name) || HIVE_TRANSACTION_FILE.test(name);
}

// The smoke test's inventory of everything outside its sandbox that a stray write could change.
// Watched recursively: the input `roots` (checkout, workspace, extracted binary, project), rur's real
// cache folders and the "rur…" entries of the real temporary folders. Watched one level deep: the
// real home folders, which catches a new dotfile or folder in them. They cannot be walked fully,
// because the runner writes its own logs and bookkeeping elsewhere under its home during the job,
// which would fail a correct binary. On Windows their OS-owned registry/profile files are also
// skipped. Everything else on the machine is not watched.
//
// The real locations are the ones before the smoke redirects its child process to the sandbox. The
// options let a test stand in for the runner without reading the real user's folders.
export function outsideFileStates(roots, sandbox, { env = process.env, home = os.homedir(), temp = os.tmpdir(), platform = process.platform } = {}) {
	const homes = [...new Set([home, env.HOME, env.USERPROFILE].filter(Boolean))];
	const watched = [...roots, ...realCacheFolders(homes, env), ...rurTemporaryEntries(sandbox, env, temp)];
	return fileStates(watched, sandbox, homes, platform === "win32" ? windowsProfileFile : undefined);
}
