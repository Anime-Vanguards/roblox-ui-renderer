# Rendering from the command line

CLI reference for `node cli/rur.mjs` (or `npm run rur -- <command>`), standalone `rur-native` subcommands, frame-by-frame sequences, Git diffs, sweeps, profiling, and text-first inspection.

## Command overview

| Command | Purpose |
|---|---|
| `rur render --story <file>` | Render a story (or `--module`, `--model`, `--fixture`, `--scene`) to PNG and `scene.json` |
| `rur sequence --story <file>` | Capture an animation frame by frame as PNGs, layout trace, and MP4 videos |
| `rur stories [--place <name>\|all]` | List stories discovered in the project |
| `rur sweep` | Render every story in the project and group failures by root cause |
| `rur compare --story <file>` | Capture in Roblox Studio and offline, diffing layout rects and pixels ([studio.md](studio.md)) |
| `rur diff --git-ref <ref> --story <file>` | Render a story at `<ref>` vs. your working tree and diff layout + pixels |
| `rur record --harness <file.luau>` | Run a Luau harness in play mode and record the 3D `Workspace` for `ror` |
| `rur profile diff <before> <after>` | Compare two engine profiles ([Profiles](#profiles---profile)) |
| `rur profile view <dir> [<dir2>]` | Interactive terminal viewer for a profile or diff ([Profiles](#profiles---profile)) |
| `rur profile render <dir> --view <name\|all> --out <path>` | Export profile viewer screens to PNG, SVG, or text |
| `rur native` | Launch the interactive desktop storybook ([storybook.md](storybook.md)) |
| `rur doctor [--fix]` / `rur clean` / `rur selftest` | Check environment, prune output dir, run offline selftests |

`rur sweep` and `rur diff` delegate directly to `rur-native sweep` and `rur-native diff`, so release binaries support them identically without Node.

**Global flags & environment:** `--state-dir <dir>` (`RUR_STATE_DIR`), `RUR_OUT_DIR`, `--json` (JSON-only stdout), `--isolated`, and `--no-service` ([live-preview.md](live-preview.md#how-a-render-runs)).

## Render

```powershell
node cli/rur.mjs render --story path/to/Button.story.luau --viewport 1200x900 --time 1
```

### Key options

| Option | Description |
|---|---|
| `--story`, `--module`, `--model`, `--fixture`, `--scene` | Input source ([stories.md](stories.md)). `--scene` renders a saved `scene.json` without running Luau. |
| `--project`, `--place`, `--roots`, `--rojo`, `--mount` | Project mounting ([projects.md](projects.md)). Defaults to `RUR_PROJECT` or `examples/project`; stories outside mounted roots mount under `ReplicatedStorage.RURFixtures`. |
| `--viewport WxH`, `--scale N` | Logical viewport size (default `1920x1080`) and DPI scale (default `1.5`). Append `@physical` (e.g., `1800x1350@physical`) or use `--viewport-from <ref.png>` to divide physical pixels by `--scale`. |
| `--time S` | Virtual seconds to simulate scripts and tweens before capturing (snaps to nearest 60 Hz frame). |
| `--background r,g,b\|transparent` | Background fill (`transparent` writes straight alpha in `render.png`). |
| `--out <dir>`, `--tag <name>`, `--keep` | Output location (default `out/<name>`, optional `<out>/<name>/` subfolder, or numbered sibling with `--keep`). |
| `--no-png`, `--offline` | Skip PNG rasterization; disallow HTTP asset downloads. |
| `--placeholders` | Draw missing images as magenta/black checkerboards labeled with their asset ID (off by default to match Studio). Placeholders stay inside `<out>/assets/` and never enter the shared cache; images remain listed in `missingImages`. |
| `--controls JSON\|file` | Initial control values (e.g., `'{"Group.Name": 2}'`). List available controls with `node cli/controls.mjs`. |
| `--actions JSON\|file` | Scripted mouse/keyboard input timeline ([stories.md](stories.md#timed-input---actions)). |
| `--rotation-clipping enabled\|disabled` | Toggle modern Roblox clipping of rotated descendants (default `enabled`). |
| `--mode edit\|play`, `--device desktop\|touch\|gamepad`, `--player <name>`, `--place-id <n>`, `--topbar-inset <px>` | Emulated client environment settings. |
| `--run-context loopback`, `--server-harness <file>`, `--latency <frames>` | With `--mode play`: the story runs on the client of a server and client in one run (the engine's "Loopback run context"). The server mounts the project and runs the harness's `Server` (`{ Server = function(Context) ... end }`; without it, the place's own server scripts); the client's DataModel is the server's replicated copy, kept up to date, and remotes cross both ways. `--latency` delays each way by that many frames (default 1). |
| `--snapshot-dir <dir>` | Mount Studio-only instances previously exported via `tools/sweep/studio-assets.mjs`. |

### Output files

Every render writes into `--out` (and prints diagnostics to stderr):
- `render.png`: physical-pixel RGBA8 image (straight alpha).
- `scene.json`: full laid-out tree, visual properties, glyph placements, and draw order.
- `summary.json`: status, script errors, unemulated APIs, missing images/fonts, per-stage timings, `culled` AutomaticSize nodes, `textLinesDropped`, `missingGlyphs`, and script `output`. `missingFonts` lists the cloud fonts that could not be downloaded, then the font families whose text was not drawn at all: neither the family nor the Source Sans Pro fallback had a face, as on a machine without Studio's fonts ([text.md](text.md#fonts-and-fallback)); `scene.json` names those in `diagnostics.unavailableFonts`. Like missing glyphs, missing fonts leave `status` as it is and don't make a sweep's story unclean; the sweep lists them per story and counts them under "Missing fonts".
- `dependencies.json`: every source file required during the run.
- `manifest.json`, `glyphs.png`, and `assets/`: bundled glyph atlas and hard links to cached images.


For details on the background render service, kept runs, `--no-service`, and `--isolated`, see [live-preview.md](live-preview.md#how-a-render-runs).

## Sequence (frame-by-frame animation)

```powershell
node cli/rur.mjs sequence --story <file> --from 0 --to 4 --fps 60 --crop Stage
```

Mounts the story once and captures frames at `from + i / fps`. Whenever `fps` divides 60, frame `i` is pixel-identical to `rur render --time <t>`.

- **Range:** `--from` plus `--to` (exclusive), `--frames N`, or `--duration S` (default 1 s); `--fps` (default 60).
- **Cropping & options:** `--crop <node>` (bounding box on frame 0, rounded to even dimensions for video) or `--crop x,y,w,h`; `--pad px`; `--actions`; `--no-png`; `--no-video`; `--scenes` (save every frame's `scene.json`).
- **Output (`out/<name>-sequence`):** `frames/00000.png`..., `trace.json` (per-frame position/size/rotation changes), `video.mp4` (H.264 BT.709 via `ffmpeg` from `PATH` or `RUR_FFMPEG`), `video-quarter.mp4` (0.25x slow-motion), and `summary.json`.

## Rojo projects and explicit mounts

Use `--rojo <file.project.json>` or repeatable `--mount Service/Name=<dir|file>` without declaring `rur.json` places. Running a story with no project flags auto-detects the nearest `default.project.json` (or sole `*.project.json`). See [projects.md](projects.md#rojo-projects-and-explicit-mounts).

## Git ref renders and before/after diffs (`--git-ref`, `rur diff`)

- **`--git-ref <ref>`:** Extracts project roots, Rojo `$path`s, `--mount` sources, and the story from `<ref>` into `<state dir>/git-refs/` using a temporary Git index (with `core.longpaths` on Windows), leaving your working tree and index untouched. Files missing at `<ref>` (e.g., newly added stories or gitignored Wally `Packages/`) fall back to the working tree with a stderr note. Requires `git` on `PATH` (`native/src/gitref.rs`).
- **`rur diff --git-ref <ref> --story <file>` (`rur-native diff`):** Renders `<ref>` (`<out>/before`) and your working tree (`<out>/after`) concurrently in two processes, printing summaries, up to 200 layout diffs (`- removed`, `+ added`, `~ changed`), and pixel differences into `<out>/diff.png` (unchanged pixels in grayscale; changes shaded yellow to red). Defaults `--out` to `out/<name>-diff`, accepts all `render` flags plus `--json`, and exits with code `2` if either side fails.

## Plugin UIs (`--plugin`)

Pass `--plugin` (or `--plugin '{"Theme":"Dark"}'` / JSON file to seed plugin settings) to expose a `plugin` global (`native/src/engine/story/plugin.luau`) with in-memory `GetSetting`/`SetSetting`, `CreateDockWidgetPluginGui` (returning a real `DockWidgetPluginGui` sized to the viewport and exported as additional layers), stub toolbars/actions/menus, and a mouse at `(0, 0)`.

## Full PlayerGuis (`--all-layers`, `--load`)

- **`--all-layers`:** Renders every enabled `ScreenGui` under `LocalPlayer.PlayerGui` and `CoreGui` in Roblox's exact draw order (`native/src/engine/layers.rs`): `PlayerGui` below `CoreGui`, sorted by `DisplayOrder`, then by most recently parented or enabled (verified against lossless 9-ScreenGui Studio captures).
- **`--load Service/Name=<file.rbxm>`:** Loads a saved model's top-level instances into a `Folder` at `Service/Name` before the story executes (handy for cloning a serialized playtest `PlayerGui`).

```powershell
node cli/rur.mjs render --fixture clone-playergui.luau --load "ServerStorage/Saved=playergui.rbxm" --all-layers --viewport 844x390 --scale 1
```

## Sweep

```powershell
node cli/rur.mjs sweep [--place all] [--jobs 4] [--timeout 90] [--out out/sweep]
```

Renders every story in the project in isolated `rur-native render` child processes (killing hung trees after `--timeout` seconds and retrying crashed stories at the end via `--retries 1`). Writes `report.json`, `summary.md` (status per story, grouped failure causes, unemulated APIs, missing images/fonts, slowest stories), and per-story outputs under `<out>/stories/<story>/`.

**Sweep options:**
- `--filter a|b`: render only stories whose path matches any pipe-separated substring.
- `--limit N`: stop after `N` stories.
- `--failed`: re-run only non-clean stories from the previous report and merge results.
- `--summarize`: regenerate `summary.md` and totals from `report.json` without rendering.
- **Forwarded render flags:** `--viewport` (default `1200x900`), `--time` (`1`), `--scale` (`1.5`), `--mode`, `--player`, `--background`, `--project`, `--roots`, `--place-id`, `--rotation-clipping`, `--device`, `--topbar-inset`, `--rojo`, `--rojo-at`, `--git-ref`, `--plugin`, `--mount`, `--offline`, `--no-png`.

**Exit codes:** `rur sweep` and `rur-native sweep` exit with the same code, which agrees with the report. A story is clean when it rendered with status `ok`, no script errors and no render errors (`clean` in each record of `report.json`; the totals count them).

| Code | Meaning |
|---|---|
| `0` | Every story is clean (also when no story matched). |
| `1` | At least one story is not clean: the totals' `clean` is below their `stories`. The headline is printed on stdout and `report.json`, `summary.md` and each story's output are written, as for a clean sweep. Also used when the sweep could not run (for example, no report for `--summarize`); then stdout is empty and the reason is on stderr. |
| `64` | Bad option: nothing was run or written. |

A caller should read the headline or `report.json` whenever stdout has one: exit `1` with a headline is a sweep with failing stories, not a crash. `--failed` and `--summarize` exit by the merged or rebuilt totals.

## Recording 3D scenes (`rur record`)

`rur record` runs a Luau harness in play mode using the emulator's 3D recorder (`ree record`; see [ree's CLI docs](https://github.com/Anime-Vanguards/roblox-engine-emulator/blob/main/docs/cli.md)) with `rur`'s project setup, producing `scene.rbxm`, `playback.json`, `events.json`, `camera.json`, and `summary.json` ready for [roblox-object-renderer](https://github.com/Anime-Vanguards/roblox-object-renderer).

```powershell
node cli/rur.mjs record --harness path/to/Harness.luau --seconds 3 --fps 60 --out out/record/demo --context '{"Scenario":"rain"}'
```

## Profiles (`--profile`)

`rur render`, `rur sequence`, and `rur record` expose `ree`'s profiling suite via `--profile <kinds> [--profile-out <dir>]` where `<kinds>` is any comma-separated combination of `work`, `caches`, `alloc`, `heap`, `timeline`, `cpu`, `phases`, `time`, `boundary`, or `all` (see [ree's profiling docs](https://github.com/Anime-Vanguards/roblox-engine-emulator/blob/main/docs/cli.md#profiles-and-caches)).

- Real GUI layout and text shaping costs are attributed directly to the Luau lines that triggered them, and `caches` tracks layout records, `AutomaticSize` results, shaped paragraphs, and rich-text caches alongside engine caches.
- Outputs go to `<out>/profile/` (`summary.txt`, `profile.json`, `report.html`, folded stacks) split into `startup` (mount) and `frames` phases.
- Profiled runs always execute out-of-service in their own process with interpreted chunks so instruction profiling sees all Luau ([how-it-works.md](how-it-works.md#how-scripts-run)). If a render retries after downloading missing cloud fonts, only the final run is profiled; `sequence` profiles the engine half only.

```powershell
node cli/rur.mjs render --story path/to/Shop.story.luau --time 2 --profile all
node cli/rur.mjs sequence --story path/to/Shop.story.luau --to 4 --profile heap,alloc --no-video
node cli/rur.mjs profile diff out/Shop/before/profile out/Shop/after/profile
node cli/rur.mjs profile view out/Shop/after/profile
node cli/rur.mjs profile render out/Shop/after/profile --view all --out out/Shop/after/views
node cli/rur.mjs profile render out/Shop/after/profile --view issues --out -
```

- `rur profile diff <before> <after>` diffs headline metrics, changed functions/lines, caches, and heap usage by type. `rur profile report <dir>` regenerates `report.html`.
- `rur profile view <dir> [<dir2>]` opens the interactive terminal viewer (Overview, Flame, Calls, Source, Allocations, Heap, Timeline, Caches, Issues, Diff; press `m` to switch metrics, `?` for keybindings).
- `rur profile render` renders any view headlessly to `.png`, `.svg`, `.txt`, a directory (`--view all`), or stdout (`--out -`). See [ree's profile viewer docs](https://github.com/Anime-Vanguards/roblox-engine-emulator/blob/main/docs/cli.md#the-profile-viewer).

## Inspect (text-first, for agents)

```powershell
node cli/inspect.mjs tree out/Button --depth 6            # Indented hierarchy: class, name, rect, visuals, text
node cli/inspect.mjs tree out/Button --filter Label       # Matching nodes with their ancestor chain
node cli/inspect.mjs node out/Button "Stack.Primary"      # Full JSON for matching nodes
node cli/inspect.mjs at out/Button 60,280                 # Elements at logical coordinate (topmost first)
node cli/inspect.mjs crop out/Button "Primary" --zoom 3   # Zoomed PNG crop of one node from render.png
node cli/inspect.mjs diff out/before out/after            # Layout, visual, and pixel diff of two renders
```

Rects are reported in logical pixels (`AbsolutePosition` / `AbsoluteSize`; with `--topbar-inset`, scripts see `ScreenGui` content shifted up by the inset like Roblox). `Path2D` entries report `GetBoundingRect` and `path=<thickness>#<hex> segments=<n>`. Undrawn or clipped content is flagged inline (`[culled, not drawn: AutomaticSize, centre outside clip]`, `[2 wrapped lines dropped]`, `[missing glyph ...]`).
