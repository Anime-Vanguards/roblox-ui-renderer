# Desktop storybook (`rur native`)

The desktop storybook runs stories interactively in-process on a live GPU window—with searchable story browsing, live mouse/keyboard input, reactive UI Labs/Flipbook controls, resizable viewports, and a built-in frame-time profiler.

## Launching

```powershell
# From a source checkout (on Windows, start.cmd also runs npm install if needed)
npm run native
npm run native -- --project path/to/project
npm run native -- --story path/to/Button.story.luau

# With a standalone release binary (auto-detects Rojo projects and rur.json)
rur-native path/to/project
```

The **Stories** panel lists stories across every place in the project, notes which project file was mounted, warns about missing Rojo `$path`s, and reports if discovery hits safety limits (e.g., an accidentally huge folder). If a story errors or hangs in `WaitForChild`, the preview banner displays the error alongside any project-file [hint](projects.md#rojo-projects-and-explicit-mounts).

Drag the preview borders to resize the viewport. Control changes update live without remounting the story. Mouse wheel scrolling predicts at display refresh rate between engine ticks using Studio's measured scroll spring.

## Replaying input (`--actions`)

Pass `--actions <file>` to feed timed input into the desktop preview starting from frame 0, exporting a profiler trace to `--out` 1 second after the final event (set `RUR_REPLAY_EXIT=1` to exit automatically afterward).

> [!NOTE]
> The desktop storybook's `--actions` expects raw [live-session input events](live-preview.md#live-sessions-rur-native-engine----session-1) with timestamps (`{"t": 1.2, "type": "down", "button": 1, "x": ..., "y": ...}`), **not** `rur render --actions` high-level actions (`click`, `drag`, `node`). Coordinates are canvas-relative, which starts `--margin` px (default `48`) above and left of `StoryTarget`—either offset coordinates by 48 px or launch with `--margin 0`.

## Headless captures and benchmarks

```powershell
npm run native -- --headless --story path/to/Button.story.luau --viewport 1200x900 --scale 1.5 --out out/native
npm run native -- --benchmark 240 --story path/to/Button.story.luau --actions actions.json --out out/native-bench
npm run native -- --headless --batch captures.json --out out/batch
```

- `--batch <file>`: reads `[{"scene": "path/scene.json", "out": "path/out"}]` and rasterizes each saved scene with its sibling manifest.
- `--save-frames`: saves unique frames by content hash; `--frame-hashes`: writes per-frame FNV-1a hashes.
- **Profiler panel:** Click **Export trace** to dump CPU, GPU, and engine frame timings as JSON. Set `RUR_LUAU_PROFILE=<file>` to sample the Luau VM (summarize with `node tools/analysis/luau_profile.mjs`).


## Memory and leak detection

Instance lifetime matches Roblox Studio:
- `Instance:Destroy()` disconnects all connections on the instance and its descendants; unreferenced destroyed trees free on the next GC cycle.
- Closing/switching stories, clicking Reload, or resizing starts a fresh Luau VM and engine.
- Unparented instances are collected only if nothing references them—if a connected handler captures a reference to its own unparented instance, neither Roblox nor `rur` collects that cycle (e.g., UI code that unparents without destroying).

Tools for hunting leaks:
- `node tools/native-app/leaks.mjs --story <file> [--control <Group.Name> --flips 10]`: flips a boolean control repeatedly in a live session and reports per-flip instance counts, Luau heap, RSS, retained unparented trees, and the handlers keeping them alive.
- `cargo build --release --features memtrace` with `RUR_MEMTRACE=1`: tracks live Rust heap bytes by phase.
