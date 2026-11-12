# Comparing with Roblox Studio

`rur compare` mounts the same UI in a running Studio (a temporary GUI under CoreGui, removed afterwards),
captures Studio's viewport losslessly, renders it offline at the same scale and writes the difference. It
needs Windows, a running Roblox Studio and a Studio MCP bridge.

```powershell
$env:RUR_STUDIO_MCP_PORT = "<port>"
node cli/rur.mjs compare --story <file> --viewport 1200x900 --time 1
node cli/rur.mjs compare --fixture fixtures/corners.luau --viewport 800x480 --time 0.2 --background 20,20,26
```

Output in `out/compare/<name>/`: `studio.png`, `rur/` (a normal render), `diff.png`, `side-by-side.png` and
`report.json`: pixel agreement (`pixels.exactPercent`: the share of pixels whose largest channel difference
is at most 2 of 255), `layout` (every GuiObject's rect matched by path) and `propMismatches` (visual
properties that differ).

`pixels.over2` is the number of pixels more than 2 levels apart and `pixels.counted` how many of them count.
A fixture that Studio does not draw the same way twice says so in its header comment, for all of it or for
rects in logical px, and those pixels are compared and reported (`pixels.unstable`: the reason and their
number) but not counted:

```lua
-- compare-unstable: Studio draws strokes of equal ZIndex in an order that changes from mount to mount
-- compare-unstable 0,408,1200,90: the last row's strokes have one ZIndex
```

`node tools/analysis/compare_total.mjs [name ...]` adds up the reports in `out/compare/`: a line per
comparison, the unstable ones with their reason, and the total of what counts.

## Starting Studio

Start the Studio a comparison uses with the launcher. It runs on a Windows desktop of its own, so none of its
windows (splash, main window, dialogs) is ever on a monitor or takes the keyboard focus:

```powershell
node tools/studio/launch.mjs --place <file.rbxl>      # prints the pid; --studio <file name> selects it
node tools/studio/launch.mjs --list                   # the Studios it started that still run
node tools/studio/launch.mjs --close <pid>            # when the task is done
```

A running Studio of the same build, place and scale is reused instead of starting another (`--new` starts one
anyway): keep one per task, reuse it across capture batches and close it at the end. Options: `--version` (an
installed build, default 0.747.0.7471481) or `--exe`, `--scale 1` (display scale 1; otherwise the primary
monitor's).

Windows composes only the desktop the user sees, so PrintWindow has nothing to read from such a Studio. `rur compare`
captures it through the engine instead: `CaptureService:CaptureScreenshot`, read back losslessly through an
EditableImage. The screenshot leaves CoreGui out, so for this Studio the compare mounts under StarterGui (which edit
mode draws) instead of CoreGui. Measured 2026-11-12 on Studio 0.747.0.7471481: 24 of 24 captures at 1.5x and 10 at 1x
gave the same pixels and the same device scale as captures of an on-screen Studio (rects, corners, strokes,
gradients, text, images, rotation, Path2D, CanvasGroup); a frame takes 0.2 to 0.3 s. `RUR_STUDIO_CAPTURE=engine` or
`window` overrides the choice.

Limits: play-capture cannot capture a play session there (a play client refuses to read its screenshot back, and
play mode has no edit datamodel); use `--user-desktop` for it, or the sim backend. A Studio there renders uncapped:
idle it held 3.5 GB private memory and 0.65 of a core.

`--user-desktop` starts Studio on the user's desktop for PrintWindow captures: minimized, every window parked above
the primary monitor with a 4 px strip on screen as it appears (a window entirely off the monitors is not composed
and 5 of 16 captures came out stale) and the foreground handed back. Its splash still shows on a monitor for about
10 s: while Studio loads, its UI thread applies no move from outside, and no start option (`SW_SHOWMINNOACTIVE`,
`STARTF_USEPOSITION`, `-qwindowgeometry`) places the splash. Nothing in rur brings Studio to the front;
`--focus` / `RUR_FOCUS` are ignored.

## The Studio MCP bridge

rur talks to Studio through an MCP server on a local TCP port: newline-delimited JSON-RPC 2.0, an
`initialize` handshake, then `tools/call` with two tools:

| Tool | Arguments | Result |
|---|---|---|
| `list_roblox_studios` | none | text content holding `{ "studios": [{ "id", "name" }] }`, the open Studio windows |
| `execute_luau` | `studio_id`, `datamodel_type` (`"Edit"`), `code` | text content holding what the chunk returned |

Any bridge that serves these over TCP works; the maintainers run their own. Settings:

| Variable | Meaning |
|---|---|
| `RUR_STUDIO_MCP_PORT` | the bridge's port on 127.0.0.1 (required) |
| `RUR_STUDIO` | the Studio to use: its id or part of its window title (default: the only one open; `--studio` overrides) |
| `RUR_STUDIO_LOCK` | the lock file every Studio user takes (default `<temp>/rur/studio.lock`) |

`node tools/studio-mcp.cjs list` lists the open Studios through the bridge; `node tools/studio-mcp.cjs luau
<file.luau>` runs a probe in one.

## What a compare does

A report (`report.json`) is the result of the run that wrote it or nothing: a compare removes the old one first,
so a run that stops (a reuse refused, a Studio error) leaves no numbers behind. It names the engine that drew the
rur side (`engine`: the binary's sha256, when it was built, the checkout's commit) and the capture (`capture`:
when, whether it was reused, Studio's `FFlagLegacyTextSubpixelRendering`). A capture in the other text state than
rur's text mode is refused with the switch to use (`--text-subpixel on|off`). Tools that add reports up read them
through `readReport` (cli/compare.mjs), which refuses a report another engine drew. For each capture Studio's mouse
icon is hidden, so the system cursor over Studio's window does not end up in the image.

- Everything it mounts lives under `CoreGui` and is removed afterwards; nothing is saved and no script is
  edited.
- Studio's frame clock and the order it iterates siblings are recorded and replayed offline, so animation
  and table iteration order don't show up as rendering differences (`--no-clock`, `--ideal`).
- Studio's viewport is nudged to whole logical pixels (a fractional viewport blurs captures), using
  `tools/wincap/` (PowerShell and PrintWindow).
- Captures are reused while everything they depend on is unchanged (`studio-capture.json` beside
  `studio.png`: the mount, the Studio instance and DPI, and a hash of every source the story loaded).
  `--recapture` always captures; `--reuse-only` never touches Studio.
- Only one capture can use Studio at a time: wrap other tools that mount GUI in Studio with
  `node tools/studio-lock.mjs <command...>`.

## Parity work

To measure a rule: write a fixture that shows it ([stories.md](stories.md#writing-a-fixture)), compare it,
look at `diff.png` and `node tools/analysis/compare_crop.mjs`, change the renderer or layout, and compare
again (the capture is reused). `tools/sweep/attribute.mjs <compare dir>` attributes differing pixels to
causes (text, strokes, images, shadows, ...). Re-record the goldens only after Studio shows the change is
right.
