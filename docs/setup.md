# Setup, release binaries, and state directories

This page covers installing prebuilt `rur-native` binaries, building from source, verifying downloads, running diagnostics, and configuring cache/output paths.

## Release binaries

Each [GitHub Release](https://github.com/Anime-Vanguards/roblox-ui-renderer/releases) includes flat zip archives for:
- `rur-native-<version>-windows-x86_64.zip`
- `rur-native-<version>-linux-x86_64.zip`
- `rur-native-<version>-macos-aarch64.zip`

Extract `rur-native` onto your `PATH` or install it via `rokit.toml` (see [README](../README.md)). On Linux and macOS, use Rokit 1.0+; Rokit 0.2.x cannot select `rur-native` because the executable name differs from the repository name.

### Runtime requirements

Release binaries require no Rust, C++ compiler, or Node.js.
- **GPU driver:** Required for PNG rendering and the desktop storybook (`render --no-png` exports `scene.json` without a GPU).
- **Roblox Studio install:** Supplies Roblox fonts and `rbxasset://` textures (`rur` ships none; FreeType and egui's own UI fonts are compiled in). On Linux/macOS or machines without Studio, point `RUR_ROBLOX_CONTENT` at a copied Studio `content` directory.

### Running without a checkout

- **Desktop storybook:** `rur-native` opens the storybook on `--project <dir>`, else `RUR_PROJECT`, else the current working directory. Relative `--project` and explicit output paths resolve against your current directory; the Open field inside the GUI resolves relative to the selected project. Both `rur.json` places and Rojo projects (`--rojo`, `--project <file.project.json>`, or auto-detected `default.project.json` / sole `*.project.json`) are supported ([projects.md](projects.md#rojo-projects-and-explicit-mounts)).
- **Standalone CLI subcommands:** `engine`, `render`, `sequence`, `serve`, `record`, `profile`, `sweep`, and `diff` all run directly without Node or a checkout (`render` and `sequence` require `--dir <output>`; `sequence` video export needs `ffmpeg`; `diff --git-ref` needs `git`). `sweep` and `diff` match `rur sweep` / `rur diff` ([render.md](render.md#sweep)): colored human summary on stderr (`NO_COLOR` disables, `CLICOLOR_FORCE=1` forces in pipes) and single-line JSON on stdout when piped or redirected.
- **Unqualified outputs:** Anything without an explicit output path writes under `<state dir>` so a release binary never litters your current directory (for example, desktop/headless captures go to `<state dir>/out/native`).
- **Assets and 3D viewports:** Local files, Studio `rbxasset://`, cached assets, uncached `rbxassetid://` assets, thumbnails, and cloud fonts resolve natively into `<state dir>/assets` (`--offline` disables HTTP). Release binaries never read `.env`; pass cookie or broker credentials via environment variables ([assets.md](assets.md)). `ViewportFrame`s launch `ror` automatically if installed ([viewports.md](viewports.md)). Reading private images directly from an open Studio still requires a checkout with Node (`RUR_ASSET_REPO` or `--repo <checkout>`); Studio comparisons (`rur compare`), fixtures, and build scripts also require a checkout.

### Profile output paths

Explicit paths are used as given (relative to the current directory). Otherwise, release builds resolve profile outputs using the first matching row:

| Command | How profiling was enabled | Output path (in priority order) |
|---|---|---|
| `render` | `--profile <kinds>` (including `boundary`) | `--profile-out`, else `<output>/profile` |
| `sequence` | `--profile <kinds>` (including `boundary`) | `--profile-out`, else `<output>/profile` |
| `render` / `sequence` | `REE_PROFILE=<kinds>` (no `--profile`) | `REE_PROFILE_OUT`, else `<output>/profile` (`--profile-out` ignored) |
| `record` | `--profile <kinds>` (non-`boundary`) | `--profile-out`, else `REE_PROFILE_OUT`, else `<output>/profile` |
| `record` | `REE_PROFILE=<kinds>` (no `--profile`) | `REE_PROFILE_OUT`, else `--profile-out`, else `<output>/profile` |
| `record` | `--profile boundary` or `RUR_BOUNDARY_PROFILE=<file>` (no engine profile) | `--profile-out`, else `RUR_BOUNDARY_PROFILE`, else `<output>/profile` |

Notes on profile outputs:
- For `render` and `sequence`, passing `--profile` causes `REE_PROFILE_OUT` and `RUR_BOUNDARY_PROFILE` to be ignored; `RUR_BOUNDARY_PROFILE` alone does not enable profiling; boundary profiles write `boundary.json` inside the profile directory.
- For `record`, a boundary profile is a single JSON file (even when defaulted to `profile`), while other profile kinds write a directory. If neither profile nor recording output is specified, it defaults to `<state dir>/out/record/profile`.
- **Source-checkout differences for `record`:** `--profile` ignores `REE_PROFILE_OUT`; `REE_PROFILE` ignores `--profile-out`; unnamed engine profiles default to `<output>/profile` if `--out`/`--dir` is set, else `./ree-profile` (same for `--dump-scene`); unnamed boundary profiles default to `./ree-boundary.json`.
- **`profile diff` and `profile report`:** Release builds write to `<state dir>/out/profile-diff` and `<state dir>/out/profile-report` (override with `--out <dir>`). Checkout builds default `profile diff` to the second input's directory (override with `--out`), while `profile report` always writes `report.html` beside its input `profile.json`.

### When stories fail: `rur-native debug-report`

Run `rur-native debug-report` inside your project folder (optionally naming specific stories, e.g. `rur-native debug-report src/Button.story.luau`) to render each story in an isolated process and generate `rur-report.md`.

The report includes your `rur` version and environment plus each story's status, failure message, traceback, errors/warnings, unemulated Roblox members used, failed images/fonts, draw errors, and logs.

By default, the report is safe to share in public issues: paths are relativized to the project, your username/home/hostname are redacted, and source code, `print` logs, and credentials are excluded. Pass `--source` to include traceback source lines, or `--prints` to include `print` output (review the file before sharing). See `rur-native debug-report --help` for `--out`, `--limit`, `--timeout`, and project flags.

### State directory and build modes

Default state directory for release binaries:
- **Windows:** `%LOCALAPPDATA%\rur`
- **Linux:** `$XDG_CACHE_HOME/rur` (or `~/.cache/rur`)
- **macOS:** `~/Library/Caches/rur`

Set `RUR_STATE_DIR` (or legacy `RUR_CACHE_DIR`) to relocate or share caches; services and child processes inherit it. Release binaries never read `.env`, fixtures, or scripts from the executable folder or project folder.

Standalone mode is baked in at compile time (`RUR_RELEASE_BUILD=1`), not controlled by runtime environment variables. Ordinary source builds always default to the checkout's `.cache` and `examples/project` even if copied elsewhere.

Per-root `rur.json` settings (`placeIds`, `streamingTargetRadius`, `clientGui`) are read from the directory containing each root (if no places are declared, the project folder itself is the root, so settings are read from its parent directory). Explicit `--mount` arguments apply after declared roots.

### How releases are built

`.github/workflows/release.yml` builds Windows `x86_64`, Linux `x86_64` (Ubuntu 22.04), and macOS `arm64` on GitHub runners using Rust `1.90.0`, default public features, `RUR_RELEASE_BUILD=1`, and `cargo build --profile dist --locked --target <triple>` (with `-C target-feature=+crt-static` on Windows).

- Main-branch runs reuse cached compiled dependencies across main runs; tagged releases build clean from scratch (`tools/release/README.md`).
- Every platform runs a smoke test unpacking the zip outside any checkout, checking `--version`, and rendering a story from `examples/project` (Windows exports `scene.json` without a GPU; Linux uses software Vulkan and macOS uses Metal to verify PNG output as well). A release only publishes when all three platforms pass.
- **Standalone integration tests (`native/tests/standalone.rs`):** Checkout tests run in normal `cargo test --release`. Installed release tests are ignored by default—compile with `RUR_RELEASE_BUILD=1`, set `RUR_TEST_STANDALONE_BINARY` to the absolute binary path, and run `cargo test --release --test standalone -- --ignored`.

### Verifying a download

Check SHA-256 hashes against `SHA256SUMS` (`Get-FileHash <zip> -Algorithm SHA256` on Windows, `sha256sum -c SHA256SUMS` on Linux), and verify GitHub build provenance:

```sh
gh attestation verify <zip> -R Anime-Vanguards/roblox-ui-renderer
```

Each zip bundles GPL-3.0-or-later license texts, notices, platform third-party licenses, and `SOURCE.md` (exact commit, version, pinned dependencies, and rebuild instructions). The release also attaches the SHA-256-verified FreeType source archive. Keep the zip if you need license notices, since tool managers like Rokit install only the binary.

## Building from source

Requirements for developing `rur` or running the Node CLI:
- **Rust 1.90+** + C++ toolchain (Visual Studio C++ Build Tools on Windows), **Git**, and **`tar`**. On Windows, run from PowerShell or Command Prompt (Git Bash's GNU `tar` mishandles `C:\...` paths). The first render automatically runs `node tools/native-app/build.mjs`, downloading pinned FreeType sources into `<state dir>/native-app/source` and compiling `rur-native` (including `roblox-engine-emulator` at the commit pinned in `native/Cargo.toml`). Subsequent edits to `native/` trigger incremental rebuilds automatically.
- **Node 18+** (`npm install`; Node 22+ for browser checks).
- **Roblox Studio installed** (or `RUR_ROBLOX_CONTENT` pointing at a copied Studio `content` folder).
- **Optional:** Roblox cookie or account broker for private `rbxassetid://` assets ([assets.md](assets.md)); `ffmpeg` (`RUR_FFMPEG`) for `rur sequence` video output; Python 3 + Pillow for `tools/analysis/`; Lune 0.10.4 (`rokit` or `RUR_LUNE`) for `tools/sweep/studio-assets.mjs`.

Run `node cli/rur.mjs doctor` to check all prerequisites, missing npm packages, native build status, render service health, Studio locks, and stale state files (`doctor --fix` cleans up stale artifacts).


## State and output directories (source checkouts)

In a source checkout, all caches, font/hinted metrics, native builds, Studio snapshots, git-ref extractions, and `serve.log` live in `<state dir>` (default `.cache`, override with `RUR_STATE_DIR` or `--state-dir <dir>`). Default render outputs go to `out/` (override with `RUR_OUT_DIR`).

To share caches with a git worktree, set `RUR_STATE_DIR` instead of symlinking or junctioning `.cache` (removing a worktree deletes through junctions). Run `rur clean` to deduplicate/shrink `out/`, or `rur clean --older-than <days>` to prune old sequences.

See [projects.md](projects.md#environment) for all environment variables.
