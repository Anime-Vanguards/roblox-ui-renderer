# Rendering from the command line

The commands, `rur render` and its options, frame-by-frame sequences, the other ways to mount a project,
sweeps, recordings of 3D worlds and the text-first inspection of a render.

## Commands

| Command | What it does |
|---|---|
| `rur render --story <file>` | run a story and draw it (also `--module`, `--model`, `--fixture`, `--scene`) |
| `rur sequence --story <file>` | one run captured frame by frame at exact moments: frames, a trace, videos |
| `rur stories [--place <name>\|all]` | list the project's stories |
| `rur sweep` | render every story of the project, report failures grouped by cause |
| `rur compare --story <file>` | render in Studio and offline, diff layout and pixels ([studio.md](studio.md)) |
| `rur diff --git-ref <ref> --story <file>` | render at a commit and in the working tree, diff them |
| `rur record --harness <file.luau>` | run a harness against the project and record the 3D world |
| `rur profile diff <before> <after>` | compare two profiles that `--profile` wrote ([profiles](#profiles---profile)) |
| `rur profile view <dir> [<dir2>]` | explore a profile, or a before/after pair, in the terminal ([profiles](#profiles---profile)) |
| `rur profile render <dir> --view <name\|all> --out <file\|dir\|->` | the viewer's screens as PNG, SVG or text, for screenshots ([profiles](#profiles---profile)) |
| `rur native` | the desktop storybook ([storybook.md](storybook.md)) |
| `rur doctor [--fix]`, `rur clean`, `rur selftest` | toolchain check, output cleanup, the offline checks |

Run them as `node cli/rur.mjs <command>` (or `npm run rur -- <command>`). Global switches: `--state-dir <dir>`
(or `RUR_STATE_DIR`) for caches, builds and logs; `RUR_OUT_DIR` for the default output root; `--json` to get
only the JSON result on stdout; `--isolated` and `--no-service` (see
[How a render runs](live-preview.md#how-a-render-runs)).

## Render

```powershell
node cli/rur.mjs render --story path/to/Button.story.luau --viewport 1200x900 --time 1
```

- Sources: `--story <file.story.luau>` (a UI Labs story; controls become Vide sources, as in UI Labs),
  `--module <file.luau>` (returns an Instance or `function(target)`), `--model <file.rbxm[x]>`,
  `--fixture <file.luau>` (`return function(Target) ... end`), `--scene <scene.json>` (draw a saved or
  hand-edited scene without running Luau). Formats: [stories.md](stories.md).
- The project: `--project <dir>` (or `RUR_PROJECT`; default `examples/project`) and its place (`--place`,
  else the place whose roots hold the story), or `--roots "a;b"`, or a Rojo project (`--rojo`). A story
  outside the mounted roots is mounted on its own, as a ModuleScript in `ReplicatedStorage.RURFixtures`.
  See [projects.md](projects.md).
- `--viewport WxH` logical size (default 1920x1080), `--scale 1.5` device scale, `--time 1` virtual seconds
  to run scripts and tweens before the capture (the engine frame nearest that time). Screenshots are physical
  pixels: `--viewport 1800x1350@physical` divides by `--scale`, and `--viewport-from <reference.png>` takes
  a screenshot's size the same way. `--background r,g,b` or `transparent` (straight alpha in render.png).
- `--out <dir>` (default `out/<name>`), `--tag <name>` (`<out>/<name>/`), `--keep` (a numbered sibling
  instead of replacing an earlier render), `--no-png`, `--offline` (no downloads).
- `--placeholders` draws each image the render could not get as a magenta and black checkerboard with its
  asset id on a black band, so a missing icon shows as missing instead of as a gap. Studio draws nothing
  there, so it is off by default. The placeholder goes into the render's own `assets/`, never the asset
  cache, and the image is still listed in `missingImages`.
- `--controls '{"Group.Name": 2}'` (or a JSON file) starts the story with those control values;
  `node cli/controls.mjs` lists a story's controls.
- `--actions '[{"t":0.5,"type":"click","node":"Stack.Primary"}]'` (or a JSON file) replays input on a
  timeline before the capture ([stories.md](stories.md#timed-input---actions)).
- `--rotation-clipping enabled|disabled` selects Roblox's clipping mode for rotated descendants (default
  enabled, the current Roblox default). `--mode edit|play`, `--device desktop|touch|gamepad`, `--player
  <name>`, `--place-id <n>`, `--topbar-inset <px>`.
- `--snapshot-dir <dir>` loads Studio-only instances exported with `tools/sweep/studio-assets.mjs`.

Output: `render.png` (physical pixels), `scene.json`, `summary.json` (status, script errors, unemulated
APIs, missing images and fonts, seconds per step, and what the render left out: `culled` nodes Studio's
AutomaticSize culling hides, `textLinesDropped`, `missingGlyphs`, the script's `output`),
`dependencies.json` (the source files the run required), `manifest.json` with the glyph atlas
(`glyphs.png`) and `assets/` (the images, hard links into the state dir's cache). `render.png` holds straight
alpha. The diagnostics are also printed on stderr. `node cli/inspect.mjs` reads these files
([Inspect](#inspect-text-first-for-agents)).


How a render runs (the render service, kept runs, `--no-service` and `--isolated`):
[live-preview.md](live-preview.md#how-a-render-runs).

## Sequence: an animation frame by frame

```powershell
node cli/rur.mjs sequence --story <file> --from 0 --to 4 --fps 60 --crop Stage
```

The story is mounted once and captured at exact moments, `from + i / fps` s. When the rate divides 60 those
are the engine's own frames and frame `i` is pixel for pixel what `rur render --time <its moment>` draws.

- Moments: `--from`, then `--to` (excluded), `--frames N` or `--duration s` (default one second); `--fps` (60).
- `--crop <node>` (its box at the first frame, even-sized for the video) or `--crop x,y,w,h`; `--pad px`.
- `--actions` run as the clock reaches them; `--no-png`, `--no-video`, `--scenes` (each frame's whole scene).
- Output (`out/<name>-sequence`): `frames/00000.png`..., `trace.json` (every node's position, size and
  rotation per frame, written when they change), `video.mp4` (H.264, BT.709; ffmpeg from the PATH or
  `RUR_FFMPEG`), `video-quarter.mp4` (a quarter of the speed) and `summary.json`.

## Rojo projects and explicit mounts

`--rojo <file.project.json>` and `--mount Service/Name=<dir|file>` mount sources without a project's
roots: [projects.md](projects.md#rojo-projects-and-explicit-mounts).

## Before and after (`--git-ref`)

`--git-ref <ref>` renders the sources as they are at a commit: the roots (with the project files next to
them), the Rojo project, `--mount` sources and the story are extracted from `<ref>` into
`<state dir>/git-refs/`, reused by later runs, through a temporary index (your index, HEAD and working tree
are untouched). `rur diff --git-ref <ref> --story <file>` renders both and prints the layout changes and
pixel difference (`<out>/diff.png`).

## Studio's `plugin` global (`--plugin`)

Plugin UIs run with `--plugin` (optionally `--plugin '{"Theme": "Dark"}'` or a JSON file to seed the
settings), which defines a `plugin` global (`native/src/engine/story/plugin.luau`): `GetSetting` /
`SetSetting` in memory, `CreateDockWidgetPluginGui` returning a real `DockWidgetPluginGui` the size of the
viewport (exported as layers after the story's own gui), toolbars, actions and menus whose signals never
fire, and a mouse at (0, 0).

## Whole PlayerGuis (`--all-layers`, `--load`)

`--all-layers` draws every enabled ScreenGui the run leaves under the local player's PlayerGui and under
CoreGui, in Roblox's draw order (`native/src/engine/layers.rs`): the PlayerGui pass under CoreGui's, then
DisplayOrder, then, for equal DisplayOrders, the order the ScreenGuis were last parented or enabled. The
rule was checked against a lossless Studio capture of a nine-ScreenGui scene: every overlap matched.
`--load Service/Name=<file.rbxm>` puts a saved model's top-level instances in a Folder at that path before
the story runs (a SerializationService blob of a playtest's PlayerGui, say, for a fixture to clone).

```powershell
node cli/rur.mjs render --fixture clone-playergui.luau --load "ServerStorage/Saved=playergui.rbxm" --all-layers --viewport 844x390 --scale 1
```

## Sweep

`node cli/rur.mjs sweep [--place all] [--jobs 4] [--timeout 90] [--out out/sweep]` renders every story of
the project, each in a process of its own, and writes `report.json` and `summary.md`: status per story,
failure causes grouped across stories, unemulated members, Studio-only instances, missing images and fonts,
the slowest stories. `--failed` re-runs the stories that were not clean.

## Record (3D worlds)

`rur record` runs a harness script against the project in play mode, steps the virtual clock frame by frame
and records everything the code puts in the Workspace (and does to Lighting): the emulator's recorder (`ree
record`; [its command line documentation](https://github.com/Anime-Vanguards/roblox-engine-emulator/blob/main/docs/cli.md)
covers harnesses and outputs) with rur's engines. The output (`scene.rbxm`, `playback.json`, `events.json`,
`camera.json`, `summary.json`) plays back in
[roblox-object-renderer](https://github.com/Anime-Vanguards/roblox-object-renderer).

```powershell
node cli/rur.mjs record --harness path/to/Harness.luau --seconds 3 --fps 60 --out out/record/demo --context '{"Scenario":"rain"}'
```

## Profiles (`--profile`)

`rur render`, `rur sequence` and `rur record` take the emulator's profiling suite: `--profile <kinds>
[--profile-out <dir>]`, with the kinds `work`, `caches`, `alloc`, `heap`, `timeline`, `cpu`, `phases`, `time`,
`boundary` or `all` ([the emulator's documentation](https://github.com/Anime-Vanguards/roblox-engine-emulator/blob/main/docs/cli.md#profiles-and-caches)
says what each one records). The run's real layout and text are in it: their cost lands on the script lines that
asked for them, and `caches` counts the layout's records and AutomaticSize sizes and the text engine's laid-out
text, shaped paragraphs and rich text beside the engine's own caches. The profile goes to `<out>/profile`
(`summary.txt`, also printed on stderr, `profile.json`, `report.html` and folded stacks) and the summary's `profile`
names it. Its phases are `startup` (the mount) and `frames` (the run after it), as `ree run` marks them, so the mount's
cost reads apart from the frames'. A profiled render or sequence runs in a process of its own, never through the render service; like every
run it interprets every chunk, as the emulator's `record` does ([how scripts run](how-it-works.md#how-scripts-run)),
so the instruction profile sees all of the scripts' Luau. A render that runs again with the cloud fonts it lacked is profiled once, in its
last run; a sequence's profile is its engine half's (the mount and the frames, not the drawing).

```powershell
node cli/rur.mjs render --story path/to/Shop.story.luau --time 2 --profile all
node cli/rur.mjs sequence --story path/to/Shop.story.luau --to 4 --profile heap,alloc --no-video
node cli/rur.mjs profile diff out/Shop/before/profile out/Shop/after/profile
```

`rur profile diff <before> <after>` compares two profiles (the emulator's `ree profile diff`: the headline figures,
the functions and lines that moved, caches, memory by type); `rur profile report <dir>` rewrites a profile's page.

`rur profile view <dir> [<dir2>]` is the emulator's profile viewer: a keyboard-driven terminal UI over a profile (or a
before/after pair) with views a key each (Overview, Flame, Calls, Source, Allocations, Heap, Timeline, Caches, Issues,
Diff), `m` for the metric and `?` for its keys; the Caches view lists the layout's and text's caches with the engine's.
`rur profile render <dir> [<dir2>] --view <name|all> --out <file.png|.svg|.txt|dir|->` draws the same screens headless,
for screenshots: `--view all --out shots` writes a PNG a view and prints each with its Markdown image line, and
`--out -` prints a view as text ([the emulator's documentation](https://github.com/Anime-Vanguards/roblox-engine-emulator/blob/main/docs/cli.md#the-profile-viewer)
has the views, keys, issues and every option).

```sh
node cli/rur.mjs profile view out/Shop/after/profile
node cli/rur.mjs profile render out/Shop/after/profile --view all --out out/Shop/after/views
node cli/rur.mjs profile render out/Shop/after/profile --view issues --out -
```

## Inspect (text first, for agents)

```powershell
node cli/inspect.mjs tree out/Button --depth 6            # indented tree: class, name, rect, visuals, text
node cli/inspect.mjs tree out/Button --filter Label       # matches with their ancestry
node cli/inspect.mjs node out/Button "Stack.Primary"      # full JSON of matching nodes
node cli/inspect.mjs at out/Button 60,280                 # what is drawn at a logical point, topmost first
node cli/inspect.mjs crop out/Button "Primary" --zoom 3   # zoomed PNG crop of a node from render.png
node cli/inspect.mjs diff out/before out/after            # layout and visual changes plus a pixel diff
```

Rects are logical screen pixels (`AbsolutePosition` / `AbsoluteSize`; with `--topbar-inset`, scripts read a
ScreenGui's content `AbsolutePosition` the inset higher, as in Roblox). Tree lines show what is not drawn as it
reads (`[culled, not drawn: AutomaticSize, centre outside clip]`, `[2 wrapped lines dropped]`, `[missing glyph
...]`). A typical loop: render, look at `render.png` or a crop, query the tree for the element that looks
wrong, edit the source, render to a second directory and `diff` the two.
