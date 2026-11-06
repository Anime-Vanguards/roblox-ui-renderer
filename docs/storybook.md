# The desktop storybook (`rur native`)

```powershell
npm run native
npm run native -- --project path/to/project
npm run native -- --story path/to/Button.story.luau
```

On Windows, `start.cmd` installs the npm dependencies when missing and launches the storybook. Stories run
on the engine in process and draw on the window's GPU surface. Pick a story in the searchable catalog (every
place of the project) or enter a path. The preview takes pointer, wheel, keyboard and text input; story
controls update the running story (a Vide source changes, the story is not mounted again). Drag the preview's
borders to resize the viewport. Scrolling is predicted at display rate between engine updates with Studio's
measured wheel spring.

`--actions <file>` plays timed input into the preview from the first drawn frame of the story, then exports
the profiler trace to `--out` one second after the last event (`RUR_REPLAY_EXIT=1` then closes the window).
The file is a list of live session input events with a time, `{"t": 1.2, "type": "down", "button": 1, "x":
..., "y": ...}` ([live-preview.md](live-preview.md#live-sessions-rur-native-engine----session-1)), not
the higher-level actions of `rur render --actions` (no `click`, `drag` or `node` targets). Its coordinates
are the canvas's: logical px from the canvas corner, which sits `--margin` (48 by default) above and to
the left of the story target. A point written for `rur render` lands 48 px up and to the left unless you
add the margin or run with `--margin 0`.

## Headless captures and benchmarks

```powershell
npm run native -- --headless --story path/to/Button.story.luau --viewport 1200x900 --scale 1.5 --out out/native
npm run native -- --benchmark 240 --story path/to/Button.story.luau --actions actions.json --out out/native-bench
npm run native -- --headless --batch captures.json --out out/batch
```

`--batch` takes `[{"scene":"path/scene.json","out":"path/native-output"}]` and draws each saved scene with
the manifest beside it. `--save-frames` saves each frame by content; `--frame-hashes` writes the FNV-1a hash of
every measured frame. The profiler panel records display frames; **Export trace** writes a trace JSON with CPU,
GPU and engine timings. `RUR_LUAU_PROFILE=<file>` samples the Luau VM (`node tools/analysis/luau_profile.mjs`
summarizes it).


## Memory

A story's memory is its instances. What frees them follows Roblox (measured in Studio):

- `Destroy` disconnects every connection of the instance and its descendants; a destroyed tree that nothing
  references is freed whole at the next collection.
- Closing a story, opening another, Reload and viewport changes start a new Luau VM and engine.
- An instance outside the DataModel that scripts no longer reference is freed too, unless one of its
  connected handlers references it: Roblox never collects that cycle, and neither does the engine. UI code
  that removes instances without destroying them (Vide's `show` / `switch` unparent what they replace) leaks
  every such tree, in a game as here.

`node tools/native-app/leaks.mjs --story <file> [--control <Group.Name> --flips 10]` runs a story in a
session, flips a boolean control and prints the leak report: instances, Luau heap and process memory per
flip, the trees outside the DataModel that are still held, and the handlers holding them.
`cargo build --release --features memtrace` builds a binary that counts live heap bytes by phase
(`RUR_MEMTRACE=1`).
