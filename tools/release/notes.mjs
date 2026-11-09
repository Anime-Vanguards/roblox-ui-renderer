// The release notes: the public commit subjects since the previous release tag, then how to
// install and verify the release and where its source is.
//
//   node tools/release/notes.mjs --tag v0.1.0 --out release-notes.md
import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { ROOT, isMain, options, version } from "./version.mjs";
import { source } from "./package.mjs";

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

if (isMain(import.meta.url)) {
	const args = options(process.argv.slice(2));
	fs.writeFileSync(args.out, notes(ROOT, args.tag));
}
