# Release scripts

The release workflow (`.github/workflows/release.yml`) runs these scripts. They use Node's standard
library only. Run them from the repository root:

```sh
node tools/release/version.mjs                  # the version, if every manifest agrees
node tools/release/version.mjs --tag v0.1.0     # also fails unless the tag is v<version>
node tools/release/licenses.mjs --target x86_64-pc-windows-msvc --freetype .cache/native-app/source/freetype-2.14.2 --out release/THIRD_PARTY_LICENSES.txt
node tools/release/package.mjs --binary native/target/x86_64-pc-windows-msvc/dist/rur-native.exe --os windows --arch x86_64 --licenses release/THIRD_PARTY_LICENSES.txt --out release
node tools/release/notes.mjs --tag v0.1.0 --out release-notes.md
node --test tools/release/test.mjs
```

- `version.mjs` reads `package.json`, `package-lock.json`, `native/Cargo.toml` and `native/Cargo.lock`.
- `licenses.mjs` writes the license texts of the crates the target compiles in, with the vendored C and
  C++ notices, the embedded fonts and FreeType.
- `package.mjs` writes `SOURCE.md` and the zip.
- `notes.mjs` writes the release notes.

For `linux x86_64` or `macos aarch64`, use that platform's target triple and binary path. `package.mjs`
checks that the binary is the first entry with mode 0755 and that no other file is executable, because
installers pick the executable from the zip. The tests also open each zip with the platform's own archive
reader.

## Checking the real license graph

The test above uses a small made-up graph. To check this repository's real one, which needs Cargo's
metadata but no build:

1. Save the output of `cargo metadata --locked --format-version 1 --filter-platform <target>` from `native/`
   as a JSON file.
2. Set `RUR_RELEASE_METADATA` to that file and `RUR_FREETYPE_SRC` to the prepared FreeType source.
3. Run `node --test tools/release/test.mjs`.

That run covers the VM, the embedded fonts and FreeType. `licenses.mjs --metadata <json>` can use the
same saved metadata instead of running Cargo. Without the environment variable, the test uses a made-up
graph that checks the exclusion of dev and build dependencies, normal transitive dependencies, the
appendix of standard license texts, and the failure when a required text cannot be found.

## Smoke test

`smoke.mjs --zip <zip> --version <version> --png true|false` unpacks a zip into a temporary folder
outside the checkout and runs it as an installed copy would run. The workflow uses it on fresh runners.
`--png false` still runs the story, lays it out and exports the scene and local assets, but skips GPU
drawing, which the Windows runners cannot do.

Before and after the commands, the smoke inventories the checkout, `GITHUB_WORKSPACE`, the extracted
binary's folder and the copied project recursively. These are the input checks used by the other
release smoke scripts too. It excludes the sandbox when walking an outside root, while still checking
the binary and project explicitly inside it.

To catch leaked redirection, it also watches the real user's `.cache/rur`, `Library/Caches/rur`,
`$XDG_CACHE_HOME/rur` and `%LOCALAPPDATA%/rur` recursively. Home means `os.homedir()`, `HOME` and
`USERPROFILE` before redirection, with duplicates removed. Their top-level files and directory names
are checked without descending into unrelated folders. Finally, entries whose names start with `rur`
(case insensitive) in the real temporary folders (`os.tmpdir()`, `TMPDIR`, `TMP`, `TEMP`) are checked
recursively, except this smoke's sandbox. The list is rebuilt after the run to catch newly created
cache and temp entries, including empty directories. Directory timestamps are not compared.

On Windows only, the shallow home listings skip `NTUSER.DAT` (the user registry hive),
`ntuser.dat.LOG`, `ntuser.dat.LOG1` and `ntuser.dat.LOG2` (hive recovery logs), `ntuser.ini`
(profile metadata), and the hive's transaction files: `NTUSER.DAT{GUID}.TM.blf` and
`NTUSER.DAT{GUID}.TMContainer<20 digits>.regtrans-ms`. These names are case insensitive; Windows
updates these files itself. Other home entries, including similarly named files and directories,
remain watched. The exclusions do not apply to recursive input, cache or temporary roots.

An entry removed while a snapshot resolves its path, reads its metadata or lists it is absent from
that snapshot.
Only `ENOENT` is ignored; permission, I/O and other errors still stop the smoke.

This catches writes to rur's fallback locations and new home dotfiles or folders without walking the
runner's diagnostic logs under `~/runners/<version>/_diag` or other unrelated user data. It does not
claim to monitor every arbitrary path on the runner.

## Installed release validation

`native/tests/standalone.rs` has two kinds of tests:

- The `checkout_*` tests use Cargo's ordinary build, so they run in `cargo test --release` and in
  `npm run native:check` with no extra setup.
- The `installed_*` tests need a binary built with `RUR_RELEASE_BUILD=1`, which Cargo cannot build
  in the same test run. They are ignored by default. Run explicitly without
  `RUR_TEST_STANDALONE_BINARY`, they fail.

### Running the installed tests

Prepare FreeType with `node tools/native-app/build.mjs --prepare`, then, from `native/` (Bash), build
the marked binary and run the ignored tests against it:

```sh
RUR_RELEASE_BUILD=1 cargo build --profile dist --locked --bin rur-native --target <target>
RUR_TEST_STANDALONE_BINARY="<absolute path to the marked binary>" cargo test --release --locked --test standalone -- --ignored
```

The variable can also name the executable extracted from a release zip. Keep `RUR_RELEASE_BUILD`
unset when you build and run the ordinary checkout tests.

The installed tests check cache and project defaults, place selection through project links, local
assets, saved frames, single and batch workers, asset helpers and the service. A batch job runs in
the runner when its variables match the runner's and in a child process otherwise; the tests cover
both, including job errors and a child that stops when its runner is killed. The headless tests
need a GPU or software Vulkan.

### Path validation

A release binary refuses an unusable path before it starts any work. Without that check an empty
value would become the current directory and a flag in a value position would become a path, so a
command could write outside the state directory and the paths the user named, and a profiler
started from the arguments would already be writing there.

These options take a path, and each must be followed by a value:

- Inputs: `--repo`, `--project`, `--place`, `--story`, `--scene`, `--batch`, `--fixture`,
  `--module`, `--model`, `--harness`, `--actions`, `--plugin`, `--load`, `--animations`,
  `--frame-times`, `--snapshot`, `--snapshot-dir`, `--mount` and `--instance`.
- Outputs: `--out`, `--dir`, `--profile-out`, `--dump-scene` and `--dependencies`.
- File or inline JSON: `--controls` and `--context`. Inline JSON stays accepted, because the same
  value can be a file name or the JSON itself.
- `--roots`, where an empty value is valid. Leaving `--roots` out takes the roots of the project's
  place, while `--roots ""` means no roots at all, with or without explicit mounts.

A missing, empty, blank, flag-like or NUL-containing value is refused with a plain error such as
`--out needs a nonempty value`.

Positional paths follow the same rule: the input of `profile report`, both inputs of `profile diff`,
and the first and optional second input of `profile view` and `profile render`. A required input
must be present. An empty input is refused because it would select the `profile.json` in the
current folder, a profile the user never named. Ordinary checkout builds keep the pinned engine's
handling of these arguments.

The same check runs on every route into the binary that does not come through the command line
parser:

- The arguments of engine batch jobs, both for jobs that run in the runner and for jobs that run in
  a child process.
- The `argv` of render and sequence service requests.
- The required `out` of each service request, and the optional `project` of `/stories?project=`.
- The `scene` and `out` of each headless batch entry. The whole manifest is checked before the first
  entry runs, so a bad later entry cannot leave the outputs of the earlier ones.

Starting the service may already have created its own state and log. A refused request creates no
render outputs.

### Keeping the path check complete

A hand-kept list of path options kept missing ones, so the ordinary standalone test target
(`native/tests/support/path_contract.rs`) finds them in the source instead. It reads the real CLI
parsers, Clap's generated options, and the launch, recorder and profile parsers of the pinned engine,
which it finds through locked Cargo metadata. Every option it finds must be in the production path
table or in a short list of values that are not paths (numbers, booleans, selectors and labels). It
also reads the request and entry fields that the service, the engine batch, the headless batch and
the sequence options use, and the service's endpoints, so that a new field or endpoint fails the
test until someone classifies it.

A positional input has no option name to search for. The inventory instead lists the profile
commands' inputs and fingerprints the bodies of the argument parsers and command dispatchers it
reviewed, the pinned engine's included. A change to one fails the test until someone reviews its
positional inputs and updates their validation, the inventory and the fingerprint. This catches a
new positional path even in a parser that used to accept only named options. Clap's generated
positional arguments are checked separately.

The installed tests take their option list from the production table, never from a copy. They try
incomplete values through commands, batch jobs and both work endpoints, and the service and the
profiled recording and render tests assert that nothing was written. These rejection tests also run
in the Windows workflow subset, because they stop before GPU startup. The release workflow runs the
inventory explicitly (`path_contract::`) before the installed tests.

### Where the workflow runs them

The `build` job runs the installed tests before it packages, against the marked `dist` binary it
just built.

The project comparison tests also run a checkout binary, which `cargo test --release` builds for the
target without `RUR_RELEASE_BUILD`. It has to be unmarked: if both binaries were standalone, the
comparison would prove nothing. The `release` profile keeps it apart from the `dist` binary that
gets packaged. The tests first check that each binary is in the mode its name says, by looking at
where it writes its cache. They then compare the two binaries on roots in both orders, linked roots,
data roots, place names and recordings with a client GUI.

- Linux uses software Vulkan and macOS uses Metal.
- Windows skips the headless capture and sequence tests because the runner has no GPU. The
  project comparisons and render-service place checks run on every platform.
- The `smoke` job then checks the packaged executable on a fresh runner of each platform: scene
  export on Windows, PNG drawing on Linux and macOS.
- No runner has Studio's fonts or textures.
