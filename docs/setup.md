# Setup: requirements, the build and the state dir

## Release binaries

Each release on the [Releases page](https://github.com/Anime-Vanguards/roblox-ui-renderer/releases) has a flat
zip per platform: `rur-native-<version>-windows-x86_64.zip`, `rur-native-<version>-linux-x86_64.zip` and
`rur-native-<version>-macos-aarch64.zip`. Extract the executable, or install it with the README's `rokit.toml`
entry. Rokit 1.0+ picks `rur-native` on Linux and macOS by its executable bit. Rokit 0.2.x cannot, because the
executable's name differs from the repository's.

### What it needs

A release binary needs no Rust, C++ compiler or Node. The desktop app and PNG renders need a graphics driver;
`render --no-png` exports a scene without a GPU. Fonts and textures come from a Roblox Studio install, or from
a copy of its `content` folder through `RUR_ROBLOX_CONTENT`. FreeType and egui's own UI fonts are compiled in,
and no Roblox fonts or textures ship.

### Without a checkout

`rur-native` opens the desktop storybook on the current project. The project is `--project <dir>`, else
`RUR_PROJECT`, else the current directory. Relative project and explicit output paths are relative to the
current directory. The desktop Open field resolves relative paths in the selected project. A project's
`rur.json` declares its places, as for the Node CLI. A Rojo project works too: `--rojo <file.project.json>`,
`--project <file.project.json>`, or a `default.project.json` in the project folder or above the story when no
project option is given ([projects](projects.md#rojo-projects-and-explicit-mounts)).

The native `engine`, `render`, `sequence`, `serve`, `record` and `profile` commands work on local inputs you
name. `render` and `sequence` need `--dir <output>`. Sequence videos also need ffmpeg.

Output you do not name goes under the state directory, so a release binary never writes into whichever
folder you started it in. The desktop app and `--headless` captures write to `<state dir>/out/native` and
report the path. Where a profile goes depends on the command and on how profiling is turned on. In the
table below, the first output that is set is used. A path you name is used as given, and a relative one is
relative to the current directory.

| Command | Profiling is turned on by | Profile output, in order |
| --- | --- | --- |
| `render` | `--profile <kinds>`, including `boundary` | `--profile-out`, else `<output>/profile` |
| `sequence` | `--profile <kinds>`, including `boundary` | `--profile-out`, else `<output>/profile` |
| `render` or `sequence` | `REE_PROFILE=<kinds>` with no `--profile` value | `REE_PROFILE_OUT`, else `<output>/profile`. `--profile-out` is ignored. |
| `record` | `--profile <kinds>` other than `boundary` | `--profile-out`, else `REE_PROFILE_OUT`, else `<output>/profile` |
| `record` | `REE_PROFILE=<kinds>` with no `--profile` value | `REE_PROFILE_OUT`, else `--profile-out`, else `<output>/profile` |
| `record` | `--profile boundary` or `RUR_BOUNDARY_PROFILE=<file>`, with no engine profile | `--profile-out`, else `RUR_BOUNDARY_PROFILE`, else `<output>/profile` |

For `render` and `sequence`, a `--profile` option means `REE_PROFILE_OUT` and `RUR_BOUNDARY_PROFILE` are
ignored. `RUR_BOUNDARY_PROFILE` alone does not turn profiling on. A boundary profile from these two
commands is the file `boundary.json` in the profile directory.

For `record`, a boundary profile is one JSON file, even when its default name is `profile`. The other
profile kinds write a directory. When you name neither a profile output nor a recording output, the profile
goes to `<state dir>/out/record/profile`.

The table describes release builds. Checkout builds follow the same rules for `render` and `sequence`. For
`record` they differ in these ways:

- With `--profile <kinds>`, a checkout build ignores `REE_PROFILE_OUT`. With only `REE_PROFILE`, it ignores
  `--profile-out`.
- An engine profile you do not name goes to `<output>/profile` when you give `--out` or `--dir`. Otherwise
  it goes to `ree-profile` in the current directory, which is also where `--dump-scene` puts it.
- A boundary file you do not name is `ree-boundary.json` in the current directory.

In a release build, `profile diff` and `profile report` write to `<state dir>/out/profile-diff` and
`<state dir>/out/profile-report`. Use `--out <dir>` to name another directory; a relative one is relative
to the current directory. In a checkout build, `profile diff` accepts `--out` and otherwise writes to
the second input's directory. `profile report` has no output option: it writes `report.html` next to
the `profile.json` it reads.

Images from Studio's content folder, cached assets and `file://` files resolve locally. An uncached asset ID,
a cloud font or a ViewportFrame model needs the rur CLI from a checkout, with Node and its npm packages
installed. Each missing image is reported in the render diagnostics, and each missing cloud font is reported
once per asset. Set `RUR_ASSET_REPO` to that checkout to use its helper; `--repo <checkout>` works too.
Neither changes the project or the state directory. Studio comparison, asset downloads, build tools,
fixture collections and the other Node CLI commands need the checkout.

### State directory

The state directory is `%LOCALAPPDATA%\rur` on Windows, `$XDG_CACHE_HOME/rur` on Linux (else `~/.cache/rur`)
and `~/Library/Caches/rur` on macOS. Set `RUR_STATE_DIR` (or its older alias `RUR_CACHE_DIR`) to move or
share it. Services and helper processes inherit the choice. No `.env`, fixtures or scripts are read from the
binary's folder or from the project.

A build runs in one of two modes, and its environment variables cannot change which. A release build
uses the standalone defaults described above. An ordinary build, such as the one the Node CLI makes,
keeps using the checkout Cargo built it from: its `.cache` and the example project. It does so when the
executable is copied somewhere else (for example into the state directory's `native-app` folder), when it
is started without cache variables, and when that checkout no longer exists or lacks the CLI. Only the
compile-time release marker selects standalone mode. The Node CLI's own defaults and build are unchanged.

A root's settings (place id, streaming radius, client GUI) come from `rur.json` in the folder
that contains the root, just as in a checkout build. With no places declared, the project folder is
the root, so its settings are read from the folder above the project. Roots keep their declared order,
and explicit `--mount` arguments apply after them.

### How releases are built

`.github/workflows/release.yml` builds each release from a public version tag on GitHub's runners, with
Rust 1.90.0, the public default features and `cargo build --profile dist --locked --target <triple>`, from a
clean target with no build cache. The cargo step sets `RUR_RELEASE_BUILD=1`. The build reads it at compile
time, so a release binary always uses standalone defaults, even when it runs inside the checkout it was built
in. Cargo rebuilds when the variable changes. Windows links the C runtime in with
`-C target-feature=+crt-static`. Pushes to main that touch release files, and manual runs, build the zips
without publishing (a dry run). A tag publishes the zips of the dry run that passed on main at the tagged
commit, or, when there is none, builds and tests them itself first.

The release workflow builds and smoke-tests Windows x86_64, Linux x86_64 (Ubuntu 22.04) and macOS
arm64. A release is published only after every platform passes. The maintainers develop on Windows.

The smoke test unpacks each zip outside any checkout, checks `--version` and renders a story from
`examples/project`. The Windows runners have no GPU, so there it exports the scene without drawing;
Linux uses software Vulkan and macOS uses Metal, and both check a PNG as well. The runners have no Studio,
so the smoke test does not cover Studio's fonts and textures.

The native integration tests (`native/tests/standalone.rs`) cover both builds. The checkout tests use Cargo's
ordinary build and run in every `cargo test --release`. The installed tests are ignored by default: build a
release binary with `RUR_RELEASE_BUILD=1` set, set `RUR_TEST_STANDALONE_BINARY` to its absolute path and run
`cargo test --release --test standalone -- --ignored`. Only the test code reads this variable. The installed
tests copy the release binary into temporary installations with redirected user folders.
`tools/release/README.md` has the full commands.

### Verifying a download

Check each file's SHA-256 against `SHA256SUMS` (`Get-FileHash <zip> -Algorithm SHA256` in PowerShell, or
`sha256sum -c SHA256SUMS` on Linux). Verify the zip's build provenance with:

```sh
gh attestation verify <zip> -R Anime-Vanguards/roblox-ui-renderer
```

The zip holds the GPL-3.0-or-later terms, the notices, the third-party license texts for its platform and
`SOURCE.md`, which names the exact tag and commit, the pinned Cargo dependencies and how to rebuild. The
release also attaches the SHA-256-checked FreeType source archive the build used. Tool managers may install
only the executable, so keep the zip if you need the notices or source details.

## Requirements

- **Rust 1.90+** and a C++ build toolchain (on Windows the Visual Studio C++ build tools), Git and tar. The
  first command that runs a story builds the native app (`node tools/native-app/build.mjs`: it downloads the
  pinned FreeType source into `<state dir>/native-app/source`, then runs `cargo build --release`, which
  fetches the engine at the commit `native/Cargo.toml` pins). Later edits to `native/` rebuild it on the next
  run, incrementally. On Windows run the build from PowerShell or cmd (Git Bash's GNU tar misreads `C:\...`
  archive paths).
- **Node 18+** with `npm install` run here (Node 22+ for the browser checks).
- **A Roblox Studio install.** Fonts and `rbxasset://` textures are read from it; rur ships none.
  `RUR_ROBLOX_CONTENT` can point at a copy of Studio's `content` folder instead (for example on Linux or
  macOS).
- Optional: a Roblox account cookie for `rbxassetid://` images that Roblox's public asset delivery refuses
  ([assets.md](assets.md)); ffmpeg for `rur sequence` videos (`RUR_FFMPEG`); Python 3 with Pillow for WebP
  thumbnails and the fitting scripts in `tools/analysis`; Lune 0.10.4 (`rokit`, or `RUR_LUNE`) for
  `tools/sweep/studio-assets.mjs`.

`node cli/rur.mjs doctor` checks all of the above and reports the state dir, npm packages the
checkout lacks (`npm install` was not run), the native build, the render service, the Studio lock and leftovers
in the state dir (`doctor --fix` removes those).


## State and output

Everything rur writes besides a command's output goes to the state dir: `RUR_STATE_DIR` (or `--state-dir
<dir>`), default `.cache`: asset and font caches, hinted metrics, the native app build, Studio snapshots,
git-ref extractions and `serve.log`. `RUR_OUT_DIR` replaces `out/` as the default output root. To share a
checkout's caches with a worktree, point the worktree's `RUR_STATE_DIR` at them; never junction or symlink
`.cache` folders into it (removing the worktree deletes through the links). `rur clean` shrinks the output
root without losing anything; `rur clean --older-than <days>` also removes old frame sequences.

Other environment variables: [projects.md](projects.md#environment).
