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

Start the Studio a comparison uses with the launcher, so it never takes the keyboard focus from whoever is using
the machine:

```powershell
node tools/studio/launch.mjs --place <file.rbxl>      # prints the process id
node tools/studio/launch.mjs --close <pid>             # when the batch is done
```

It starts Studio without activating it (`SW_SHOWNOACTIVATE`), moves its main window off the visible desktop, above
the primary monitor, where it keeps that monitor's display scale and still renders and captures, and for
`--guard-seconds` (45) puts back any Studio window that takes the foreground (a login or recovery dialog). It
also moves Studio's drawn cursor out of the way: until the mouse has been over the viewport, Studio draws it at
the viewport's centre, inside the compared area (`--park <pid>` does that again). Options: `--version` (the
installed build, default 0.747.0.7471481) or `--exe`, `--scale 1` (draw at display scale 1 on any monitor; a
running Studio moved to a monitor of another scale reports a wrong viewport size), `--on-screen`.

Nothing in rur brings Studio to the front: captures render the same unfocused, and play-capture guards the
foreground while its play session runs. `--focus` / `RUR_FOCUS` are ignored. Measured on Windows 11
(2026-11-12): through a launch, eight captures, a play-capture and the close, no Studio window was ever the
foreground. A desktop of its own (CreateDesktop) would keep Studio out of focus entirely, but Studio does not
draw its viewport there.

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
