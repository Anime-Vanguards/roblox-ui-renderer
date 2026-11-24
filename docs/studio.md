# Comparing with Roblox Studio (`rur compare`)

`rur compare` mounts the same story or fixture inside a running Roblox Studio (temporarily under `CoreGui` or `StarterGui`, cleaned up afterward), captures Studio's viewport losslessly, renders it offline in `rur` at the same DPI scale, and diffs both layout rects and pixels.

Requires Windows, Roblox Studio, and a local Studio MCP bridge.

```powershell
$env:RUR_STUDIO_MCP_PORT = "<port>"
node cli/rur.mjs compare --story <file> --viewport 1200x900 --time 1
node cli/rur.mjs compare --fixture fixtures/corners.luau --viewport 800x480 --time 0.2 --background 20,20,26
```

Studio is asked for a story's module where the run's project mounts it, as `rur render` mounts it: below a root
(`<root>/<Service>/...`: `--roots`, a project's places) or at the instance path a Rojo project (`--rojo`,
`--project <file.project.json>`, the one found beside the story) or `--mount` gives its folder. A story none of them
holds is refused before Studio is asked (exit 64). When Studio's side fails (the module is not in that Studio, the
story or a module it requires raises an error there, no capture comes back), nothing is scored: the reason goes to
stderr and `rur compare` exits 2 without a report.

## Outputs and unstable regions

Results write to `out/compare/<name>/`: `studio.png`, `rur/` (standard render), `diff.png`, `side-by-side.png`, and `report.json`:
- `pixels.exactPercent`: percentage of pixels with max channel delta <= 2/255.
- `pixels.over2` / `pixels.counted`: total differing pixels and how many count against parity.
- `layout`: per-path `GuiObject` rect comparison.
- `propMismatches`: differing visual properties.

If Studio itself renders a fixture non-deterministically, declare unstable regions in the fixture header comment so they are still reported (`pixels.unstable`) without failing totals:

```lua
-- compare-unstable: Studio draws strokes of equal ZIndex in an order that changes from mount to mount
-- compare-unstable 0,408,1200,90: the last row's strokes have one ZIndex
```

Run `node tools/analysis/compare_total.mjs [name ...]` to aggregate reports across `out/compare/`.

## Launching a background Studio (`tools/studio/launch.mjs`)

Use the launcher to run Studio on a hidden Windows desktop so splash screens, windows, and dialogs never appear on your monitor or steal keyboard focus:

```powershell
node tools/studio/launch.mjs --place <file.rbxl>      # Prints PID; pass --studio <file name> to select
node tools/studio/launch.mjs --list                   # Show running launcher-managed Studios
node tools/studio/launch.mjs --close <pid>            # Shut down when done
```

Reuses an existing launcher Studio with matching build, place, and scale unless `--new` is passed. Options: `--version` (default `0.747.0.7471481`), `--exe`, `--scale 1` (default: primary monitor scale).

**Engine capture vs. PrintWindow:**
- Because Windows does not composite hidden desktops, `rur compare` captures hidden Studios via `CaptureService:CaptureScreenshot` read back losslessly through an `EditableImage` (mounting under `StarterGui` since `CaptureScreenshot` omits `CoreGui`). Verified on Studio 0.747.0.7471481 (24/24 captures at 1.5x and 10/10 at 1x identical to on-screen captures; 0.2–0.3 s/frame). Override with `RUR_STUDIO_CAPTURE=engine|window`.
- Hidden Studios cannot capture play sessions (play clients reject screenshot readback) and render uncapped (~3.5 GB RAM, ~0.65 CPU core idle).
- Pass `--user-desktop` to launch on your normal desktop for PrintWindow / play-mode captures (minimized, parked off-screen with a 4 px visible strip so Windows keeps compositing; Studio's splash still appears for ~10 s while booting). `rur` never foregrounds Studio (`--focus` and `RUR_FOCUS` are ignored).


## Studio MCP bridge

`rur` communicates with Studio over a local newline-delimited JSON-RPC 2.0 TCP MCP server implementing `initialize` and `tools/call` for two tools:

| Tool | Arguments | Expected return |
|---|---|---|
| `list_roblox_studios` | *(none)* | Text content containing `{ "studios": [{ "id", "name" }] }` |
| `execute_luau` | `studio_id`, `datamodel_type` (`"Edit"`), `code` | Text content containing the chunk's return value |

| Variable | Description |
|---|---|
| `RUR_STUDIO_MCP_PORT` | Localhost (`127.0.0.1`) TCP port of the MCP bridge (required) |
| `RUR_STUDIO` | Target Studio ID or window title substring (default: the only open Studio; `--studio` overrides) |
| `RUR_STUDIO_LOCK` | Mutual-exclusion lock file path (default `<temp>/rur/studio.lock`) |

Helper CLI: `node tools/studio-mcp.cjs list` and `node tools/studio-mcp.cjs luau <file.luau>`.

## Capture details and caching

- **Fresh reports:** `rur compare` deletes any existing `report.json` before starting so aborted runs never leave stale metrics. Reports stamp the `rur` binary SHA-256/build time/commit (`engine`) and Studio's `FFlagLegacyTextSubpixelRendering` flag (`capture`); mismatched text modes fail with the exact `--text-subpixel on|off` switch needed, and `readReport` rejects reports from other builds.
- **Deterministic replay:** Studio's frame clock and sibling iteration order are recorded and replayed offline (`--no-clock`, `--ideal` disable). The Studio mouse cursor is hidden during capture, and `tools/wincap/` nudges the viewport to whole logical pixels to prevent fractional-pixel blur.
- **Capture reuse:** Stored alongside `studio-capture.json` (mount, Studio build/DPI, source file hashes) and reused automatically when inputs match. Pass `--recapture` to force a fresh capture, or `--reuse-only` to run strictly offline.
- **Shared lock:** Wrap any custom script that mounts UI in Studio with `node tools/studio-lock.mjs <command...>`.

## Parity workflow

Write a minimal single-rule fixture ([stories.md](stories.md#writing-a-fixture)) → run `rur compare` → inspect `diff.png`, `node tools/analysis/compare_crop.mjs`, and `tools/sweep/attribute.mjs <compare dir>` → fix layout or renderer → re-run `rur compare` (reuses the cached Studio capture) → re-record goldens only after Studio confirms parity.
