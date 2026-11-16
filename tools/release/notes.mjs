// The release notes: the public commit subjects since the previous release tag, then how to
// install and verify the release and where its source is. For the canary, a build of main: the
// commit, when it was built, and the commits since the latest version release.
//
//   node tools/release/notes.mjs --tag v0.1.0 --out release-notes.md
//   node tools/release/notes.mjs --tag canary --run <id> --run-date <time> --out release-notes.md
import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { ROOT, isMain, options, version } from "./version.mjs";
import { source } from "./package.mjs";

const REPOSITORY = "https://github.com/Anime-Vanguards/roblox-ui-renderer";
// The rolling prerelease's tag, moved to newer commits of main by the export
export const CANARY = "canary";
// The canary lists at most this many commit subjects; the comparison link shows them all
const CANARY_SUBJECTS = 100;

export function notes(root, tag) {
	version(root, tag);
	const git = args => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
	const commit = git(["rev-parse", "HEAD"]);

	// Starts from HEAD's parent, so a tag on HEAD itself is not "the previous release". The
	// first release has no previous tag and lists every commit.
	let previousTag;
	try {
		previousTag = git(["describe", "--tags", "--match", "v[0-9]*", "--abbrev=0", "HEAD^"]);
	} catch {}
	// The subjects come straight from git's output, never through a shell
	const range = previousTag ? `${previousTag}..HEAD` : "HEAD";
	const subjects = git(["log", "--format=%s", range]).split("\n").filter(Boolean);

	return `# rur-native ${tag}

${subjects.map(subject => `- ${subject}`).join("\n")}

## Installing

Download the zip for your platform from this release, or put this entry in rokit.toml:

rur-native = "Anime-Vanguards/roblox-ui-renderer@${tag.slice(1)}"

Rokit 1.0+ is required on Linux and macOS; 0.2.x cannot select this binary's name there.
The executable runs on local content; downloads require the rur CLI from a checkout with Node.js.
A root's settings come from rur.json in the folder that contains the root, as in a checkout.
With no places declared, the project folder is the root and settings are read from the folder above it.
See docs/setup.md at this tag for requirements and state paths.

## Verifying

Check the asset's SHA-256 against SHA256SUMS, then run:

gh attestation verify <zip> -R Anime-Vanguards/roblox-ui-renderer

To check how the executable behaves, prepare FreeType in a checkout of this tag, set
RUR_TEST_STANDALONE_BINARY to the absolute path of the extracted executable, and run this from native/:

cargo test --release --locked --test standalone -- --ignored

See tools/release/README.md at this tag for the setup and the GPU requirement.

${source(root, tag, commit)}`;
}

// The canary's notes, for the checked-out commit (HEAD), whose zips release run `runId` built
// starting at `runDate`
export function canaryNotes(root, { runId, runDate }) {
	if (!/^[1-9]\d*$/.test(runId ?? "") || !/^\d{4}-\d\d-\d\dT\d\d:\d\d/.test(runDate ?? "")) {
		throw new Error("the canary notes need the id of the run that built the zips (--run) and when it started (--run-date)");
	}
	const git = args => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
	const commit = git(["rev-parse", "HEAD"]);

	// The latest version release: the highest version tag the commit contains, which can be on the
	// commit itself. Only exact vMAJOR.MINOR.PATCH tags count, never the canary's own or any other.
	const latest = git(["tag", "--merged", "HEAD", "--list", "v*", "--sort=-version:refname"])
		.split("\n")
		.find(tag => /^v\d+\.\d+\.\d+$/.test(tag));
	const subjects = git(["log", "--format=%s", latest ? `${latest}..HEAD` : "HEAD"]).split("\n").filter(Boolean);
	const count = `${subjects.length} commit${subjects.length === 1 ? "" : "s"}`;
	const listed = subjects.slice(0, CANARY_SUBJECTS).map(subject => `- ${subject}`);
	if (subjects.length > listed.length) listed.push(`- and ${subjects.length - listed.length} more`);
	const changes = !latest
		? `## Commits

${count}, before the first version release:

${listed.join("\n")}`
		: subjects.length === 0
			? `## Since ${latest}

No commits since ${latest}: this is the commit it was released from.`
			: `## Since ${latest}

${count} since ${latest} ([compare](${REPOSITORY}/compare/${latest}...${commit})):

${listed.join("\n")}`;

	return `# rur-native canary

This is a build of main, not a release, and it may break: for a release, see [Releases](${REPOSITORY}/releases).

Commit: [${commit.slice(0, 10)}](${REPOSITORY}/commit/${commit}) on main
Built: ${runDate.slice(0, 10)} ${runDate.slice(11, 16)} UTC, by [release run ${runId}](${REPOSITORY}/actions/runs/${runId}), which tested every platform

${changes}

## Downloading

Download the zip for your platform from this release and check its SHA-256 against SHA256SUMS. The
assets keep their names when the canary moves to a newer commit of main, so a link such as
${REPOSITORY}/releases/download/${CANARY}/rur-native-canary-windows-x86_64.zip always gets the newest one.
\`rur-native --version\` prints the version in main's manifests; SOURCE.md in the zip names the commit.

## Source

This rur-native binary is distributed under GPL-3.0-or-later. Its source is the commit above:
${REPOSITORY}/tree/${commit}
The exact SHA-256-checked freetype-2.14.2.tar.gz used for the build is attached to this release.
SOURCE.md in each zip lists the git dependencies and how to rebuild.
`;
}

if (isMain(import.meta.url)) {
	const args = options(process.argv.slice(2));
	fs.writeFileSync(args.out, args.tag === CANARY ? canaryNotes(ROOT, { runId: args.run, runDate: args["run-date"] }) : notes(ROOT, args.tag));
}
