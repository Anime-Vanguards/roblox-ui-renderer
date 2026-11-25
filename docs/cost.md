# Deterministic UI cost (`rur cost`)

`rur cost` runs a deterministic, CPU-only accounting pass over a Roblox UI scene (or a multi-frame sequence) to count quads, glyphs, clipped overdraw, Studio `CanvasGroup` render-target sizes, hidden DOM subtrees, and layout churn without initializing a GPU device or recording host wall-clock timings.

Every `cost.json` output contains zero wall-clock fields and zero output-directory paths, so repeated runs of the same input are bit-identical even under heavy host load.

## Commands

```powershell
# Single-moment UI cost (fixture, story, module, or model)
node cli/rur.mjs cost --story path/to/Hud.story.luau --viewport 1920x1080 --scale 1.5 --out out/hud-cost

# Full PlayerGui + CoreGui cost across every enabled ScreenGui
node cli/rur.mjs cost --fixture path/to/hud.luau --all-layers --out out/hud-all

# Multi-frame sequence layout churn (CPU-only, no GPU/PNG/video)
node cli/rur.mjs cost --story path/to/Anim.story.luau --from 0.5 --frames 60 --fps 60 --out out/anim-cost

# Capture frames/video AND deterministic cost.json in one pass (pass --mode play for per-frame layoutCpuMs; default edit mode refuses it on screen)
node cli/rur.mjs sequence --story path/to/Anim.story.luau --mode play --from 0 --frames 60 --fps 60 --cost --out out/anim-seq

# Analyze a saved scene.json (note: hiddenInShown.complete is false without a live DOM)
node cli/rur.mjs cost --scene out/hud/scene.json --out out/hud-cost

# Deterministic A/B diff between two cost.json reports
node cli/rur.mjs cost compare out/before/cost.json out/after/cost.json --json
node cli/rur.mjs cost --story path/to/Hud.story.luau --compare out/before/cost.json --out out/after
```

`rur cost` delegates directly to `rur-native cost`, so standalone release binaries support every flag without Node. Without `--json`, stdout prints a compact one-screen summary (stating the two defaults `single=edit, seq=play`, labeling sequence headlines as `headline=last sample` with peak quads/glyphs/RT/overdraw printed beside them, noting on static sequences that opening/full-tree cost is on the single-moment screen, suggesting a later `--time` when a single-moment `--time 0` run is empty, disambiguating repeating sibling paths with `#index @x,y`, naming the changed objects behind per-frame layout cost in `top layout` with each driver's attributed ancestor/descendant share and the listed share of total churn, and pointing to `cost.json`) while the full deterministic JSON is written to `<out>/cost.json`; pass `--json` to print the exact pretty-printed JSON bytes written to `cost.json`. `--out` accepts either a directory (writes `<dir>/cost.json`) or a `.json` file path.

## What `rur cost` counts

### 1. Quads, glyphs, overdraw, and the unmeasured draw/batch model

- **Quads (`totals.quads`, `totals.quadsByKind`):** non-glyph UI quads broken down by kind (`solid`, `styled_fill`, `border`, `image`, `styled_image`, `shadow`, `stroke`, `scrollbar`, `group_composite`, `path2d`).
- **Glyph quads (`totals.glyphQuads`):** visible non-space fill glyph quads (`fillGlyphQuads`) plus stroke glyph quads (`strokeGlyphQuads`: 4 offset copies per glyph for legacy `TextStrokeTransparency < 1`, plus 1 copy per glyph for each enabled `ApplyStrokeMode.Contextual` `UIStroke` on a text object). Clipped-out lines are excluded; `MaxVisibleGraphemes` cuts are honored.
- **Overdraw (`totals.screenOverdrawViewports`, `totals.offscreenOverdrawViewports`):** clipped physical pixel area summed over drawn quads and visible text line boxes, divided by the physical viewport area (`physicalWidth * physicalHeight`), separated into main-screen vs. `CanvasGroup` offscreen render-target overdraw. Rotated quads and rotated text lines are clipped as exact transformed polygons (Sutherland-Hodgman against the clip box and target bounds, then shoelace area) rather than axis-aligned bounding boxes, and descendants inside rotated `CanvasGroup`s are unturned into the group's own flat texture coordinates before measuring offscreen area (so a 100x100 element at 45° counts 10,000 px instead of 20,000 px, a 200x200 element at 15° counts 40,000 px instead of 60,000 px, a rotated `CanvasGroup` composited onto a 0.25x-scaled outer group counts its exact rotated area, and spinning elements keep constant overdraw across angles).
- **Unmeasured draw/batch model (`totals.drawCalls`, `totals.batchBreaks`, `totals.batchBreaksByCause`):** `rur`'s own wgpu renderer uses an uber-shader that merges dissimilar primitives into very few batches. `rur cost` instead tracks a simple state-transition model per render target keyed by `(layerPass, shader, texture, effectiveScissor, rotated)`, where a scissor covering the whole render target normalizes to unclipped. **No client capture backs these draw-call or batch-break counts**: every distinct image URI counts as a `texture` break (ignoring client UI image atlasing), and the shader split (`solid`, `styled_fill`, `image`, `styled_image`, `text`, `stroke`, `shadow`, `path2d`, `group_composite`) is uncalibrated. Both fields are marked `"unmeasured_state_transition_model"` in `fidelity`, listed in `unmodeled`, printed last in the summary line as `model draws=... breaks=... (unmeasured)`, and **excluded from `rur cost compare` verdicts** (informational only).
  - `layerPass`: switching between `PlayerGui` and `CoreGui` passes.
  - `renderTarget`: compositing a `CanvasGroup` offscreen texture onto its parent target.
  - `clip`: changing the active scissor clip rectangle inside the target (`ClipsDescendants` / scroll window / text self-clip).
  - `shader`: switching between modeled primitive shader classes.
  - `texture`: changing `Image` URI between adjacent image quads.
  - `rotation`: switching between axis-aligned and rotated elements.

### 2. `CanvasGroup` offscreen render targets

Offscreen target sizing uses the exact Studio rules established in [images.md](images.md) (`native/src/render/geometry.rs::group_texture_dims`):

- Rectangular RGBA8 allocation `(nw, nh)` from the group's unrotated snapped device rectangle and float32 logical size (`nw * nh * 4` bytes).
- Large groups scale uniformly so neither side exceeds `2048` device px and area stays within `2048 x 1024` px.
- Groups that are `culled`, `Visible = false`, zero-sized, or have quantized `alpha8(1 - GroupTransparency) == 0` allocate `0` bytes and perform `0` RT switches.
- Each drawn target counts `2` render-target switches (`totals.canvasRtSwitches`) and tracks nesting depth (`totals.maxCanvasGroupDepth`).
- **Offscreen texture clipping and texel area rules:** Matching `native/src/render/groups.rs::texture_clip_node` (Studio rule measured in [images.md](images.md), round 7), a `CanvasGroup`'s own background and its descendants are rendered into its offscreen texture clipped only by the group's bounds and any clipping frames *inside* the group — not by the group's ancestors (live runs resolve interior clips directly from the DOM hierarchy rather than heuristic edge-stripping, so nested `CanvasGroup`s and interior clips sharing an edge with an outer clip remain exact). The ancestor clip applies when compositing the group's texture onto its parent target. Inside a size-capped `CanvasGroup`, offscreen overdraw is scaled by `(nw / flat_w) * (nh / flat_h)` so it measures actual RT texels written rather than unscaled logical pixels.
  - **Non-comparability note:** Reports generated before this rule unified background and descendant clipping do not compare on `offscreenOverdrawViewports` (or `quads` / `clip` batch breaks) for `CanvasGroup`s clipped by an ancestor (including overhanging nested `CanvasGroup`s). Earlier builds either clipped both background and children to the ancestor or left the background unclipped while clipping children.

### 3. Hidden-in-shown DOM subtrees

Scene JSON omits descendants below `Visible = false` nodes. When `rur cost` runs a live story/fixture/module/model, it walks the live DOM under every enabled exported `ScreenGui` before engine teardown:

- `totals.hiddenInShown.roots`: top-most `Visible = false` `GuiObject`s inside enabled `ScreenGui`s.
- `totals.hiddenInShown.guiObjects`: total `GuiObject`s inside those hidden subtrees (including the hidden roots).
- `totals.hiddenInShown.instances`: total instances inside those hidden subtrees (including `Folder`s and UI modifiers).
- `totals.disabledScreenGuis` and `totals.disabledScreenGuiInstances`: `Enabled = false` `ScreenGui`s under `PlayerGui` / `CoreGui`, kept separate from `hiddenInShown` because disabled `ScreenGui`s are skipped by Roblox's layout and draw passes.
- `hiddenSubtrees`: the largest hidden-in-shown subtrees sorted deterministically by descendant count.

When invoked with `--scene <scene.json>`, `totals.hiddenInShown.complete` is `false` because descendants below hidden nodes are absent from `scene.json`.

### 4. Multi-frame sequence layout churn

In sequence mode (`rur cost --from/--to/--frames/--duration/--fps` or `rur sequence --cost`), each captured frame records:

- Incremental `Layout.Stats` counters for that frame (`passes`, `computed`, `memo`, `measures`, `stored`, `skipped`, `moved`, `texts`, `scrollPasses`, `scrollMoved`). Static UI incurs `0` layout churn on frames `1..N`; animated or resizing UI reports the exact objects laid out each frame, and scrolling reports `scrollPasses` and `scrollMoved` separately from non-scroll `moved` records.
- `unchanged`, `changedNodes`, `removedNodes`, and full per-frame `totals` matching single-moment `rur cost --time <t>` when `fps` divides `60`.
- `sequence.summary`: `mean`, `peak`, and `p95` across all frames, plus steady-state (`frames[1..]`) layout churn mean and extrapolated layout CPU ms.
- `runMode` and `runModeInfo`: every `cost.json` records the engine run mode (`"play"`, `"edit"`, or `"scene"`) and its cadence.
  - **`rur cost` sequence mode defaults to `--mode play` at 60 Hz client cadence** (`--client-hz` defaults to `60`; 1 `Heartbeat` per client frame). `--fps` controls only how often samples are captured: steady-state layout counters and `layoutCpuMs` are always normalized per 1 client frame at `--client-hz` (for example, `--fps 30` steps 2 client frames per sample and divides by 2, and `--fps 120` alternates 0 and 1 client frames per sample and divides by the completed client frames). Any individual sample spanning `0` or `>1` client frames — or frame 0, or a run where `0` steady client frames completed — refuses `layoutCpuMs` rather than printing a non-client-frame figure as one client frame. Pass `--client-hz <Hz>` to simulate another client refresh rate explicitly (the report label prints the cadence that ran).
  - **Explicit `--mode edit`** (and `rur sequence --cost` without `--mode play`) runs Studio's 240 Hz edit-mode `Heartbeat` (4 beats per 60 Hz frame), which can fire Heartbeat-driven work up to 4x a client's rate. Because of this, `rur cost` **refuses** per-frame `layoutCpuMs` in `--mode edit` (`status: "refused"`, `ms: null`) and directs you to re-run with `--mode play`. Single-moment `rur cost` defaults to `--mode edit` (matching `rur render`) and already refuses per-frame `layoutCpuMs` in both modes.
  - **Failing, yielding, or empty-mount runs:** If a story errors, yields without returning, or mounts nothing (`status: "error"`, `"yielded"`, or `"empty"`), `rur cost` prints the unwrapped failure cause (skipping nested wrapper lines such as `error while running root():` / `effect error:` to surface the root error and user frame) or the exact yield location (`waitingAt`, with file and line) on stdout/stderr and in `cost.json`, refuses `layoutCpuMs` (never reporting `0.0000 ms` as a valid UI cost), and exits `2`. If the story drew UI and then yielded inside mount (for example, an inline `while true do task.wait() end` loop), the screen names the drawn node count and tells you to wrap the loop in `task.spawn` or `Heartbeat:Connect` rather than claiming zero counts. When a story mounts in `--mode edit` but not `--mode play`, the play screen states that per-frame `layoutCpuMs` is unavailable as-is and points to `--mode edit` for static counts, while the `--mode edit` sequence probes play mode and prints that play-mode wait/error as the single fix rather than sending you back to `--mode play` in a circle. Conversely, when a default edit-mode single-moment (or `--mode edit` sequence) run fails or mounts empty, `rur cost` probes `--mode play` and notes `mounts in play mode, not edit (pass --mode play)` on the failing screen when play mode mounts it.

### 5. Attribution, extrapolation, and explicit refusals

- **Attribution:** `screenGuis` attributes draw calls, batch breaks, quads, glyph quads, CanvasGroup RT bytes/switches, overdraw, and hidden-in-shown counts per `ScreenGui` (enabled ScreenGuis sum conservatively to `totals`; disabled ScreenGuis appear with `enabled: false`). `objects` lists the heaviest drawn objects and `CanvasGroup`s.
- **Single-moment vs. per-frame layout CPU ms:**
  - Single-moment `rur cost` (without `--from`) only observes the export's full-tree layout pass (`fullTreeLayout`). It **refuses** `predicted.*.layoutCpuMs` (`status: "refused"`, `ms: null`) and directs you to run with `--from` and `--frames` for per-frame cost, while reporting the one-shot full-tree price separately under `predicted.*.fullTreeLayoutCpuMs` (`scope: "full_tree_export_pass"`).
  - `--scene` reports carry no layout counters and refuse `predicted.*.layoutCpuMs`.
- **Extrapolated layout CPU ms (`predicted.*.layoutCpuMs` in sequence mode, `fullTreeLayoutCpuMs` in single mode):**
  - `desktop-ref` (`basis: "extrapolated"`): extrapolates the Studio 0.744 script-triggered lazy-layout probe (`native/src/engine/layout/install.rs`: `2.3 us` per pass + `0.312 us` per `stored + moved + scrollMoved` record + `62 us` per scroll pass; 100 cards / 301 records = `155.9 us`, 1,000 cards / 3,001 records = `998.3 us`) to frame or full-tree layout passes, naming the recording in `extrapolatedFrom`. On deep UI trees, most `stored` records come from ancestor-chain invalidation (`rur`'s layout bookkeeping); whether a Roblox client pays the same per-object cost for every invalidated ancestor is not yet verified against a client capture, so the one-screen summary prints a `caveat:` line whenever extrapolated ms is shown (absolute ms on deep trees is unchecked against a client; before-vs-after comparisons on one story remain valid).
  - `phone-mid` (`basis: "derived_extrapolation"`, `ratioBasis: "derived"`): scales `desktop-ref` by a derived ratio rather than a recorded layout ratio — `4.09` (range `2.65..8.0`, recorded whole-UI captured phone/desktop ratio) × `1.25..2.0` (assumed profiler capture-overhead factor, never measured for layout) × `0.77` (assumed live frame weight from mobile live-gameplay GUI captures, `assumed_from_match_gui_weight`) = central range **`3.9x..6.3x`** (`rangeMs`, `ratioRange: [3.9366, 6.2986]`) and full band **`2.55x..12.32x`** (`bandMs`, `bandRatio: [2.5506, 12.32]`). Reports `factors` (each with its basis) and `settledBy` (paired captured vs. uncaptured desktop `UpdateUILayouts` timings and mobile mid-tier UI layout timings) instead of a single false-precision point number, and prints `2/3 factors assumed` on the one-screen summary line beside the range.
- **Explicit refusals (`status: "refused"`, `ms: null`):** `drawPassCpuMs` and `gpuUiMs` are explicitly refused on both tiers with `reason` and `settledBy` because no per-element Studio or mobile GPU calibration backs them yet. `unmodeled` enumerates all excluded cost dimensions.

## A/B comparison (`rur cost compare`)

`rur cost compare <before> <after>` runs an **equivalence gate** before emitting any verdict:

- **Input validation:** Both inputs must be well-formed `rur-cost/1` cost reports. Passing a non-cost file (such as `scene.json`, `summary.json`, `trace.json`, or a previous `rrr-ab/1` diff) exits non-zero with an error naming what the file is and what to run instead.
- **Equivalence checks:** `schema`, `status` (both `"ok"`), `source` (matching story/fixture/module/model target), `mode` (`single` vs. `sequence`, plus matching `from`, `frameCount`, `fps`, and `clientHz` for sequences), `runMode` (`play` vs. `edit` vs. `scene`), `input` (`live` vs. `scene`) and `totals.hiddenInShown.complete`, `viewport` (`width` and `height`), `scale`, `device` (`desktop`/`touch`/`gamepad`), and `controls`. Reports saved by earlier builds that lack `source`, `device`, `controls`, or single-moment `time` are never refused for a missing field alone; they compare and print a one-line `note:` naming which fields were not checked because the older report predates them. Any mismatch between present fields returns `verdict: "refused"` with `equivalence.passed: false`, lists every mismatch and remediation command, and exits non-zero. Unknown CLI options and unexpected positional arguments exit `64` immediately rather than being silently ignored, and `--time` rejects lists with an error noting that `rur cost` takes a single moment (use `--from` and `--frames`/`--to`/`--duration` with `--fps` for multiple moments).
- **Comparable arms (`rrr-ab/1`):**
  - `verdict`: `improved`, `regressed`, `mixed`, or `unchanged` (with `measurementNoise: 0.0`), driven only by measured counts (`quads`, `glyphQuads`, `canvasRtBytes`, screen/offscreen overdraw, `hiddenGuiObjects`, and sequence layout churn; unmeasured `drawCalls`/`batchBreaks` are informational only).
  - `equivalence`: `{ "passed": true, "checks": [...] }`.
  - `counts`: `before`, `after`, and `delta` across quads, glyph quads, CanvasGroup RT bytes/targets/switches, screen/offscreen overdraw, hidden-in-shown `GuiObject`s, and informational model draw calls / batch breaks.
  - `layout` and `predicted`: before/after/delta layout counters and extrapolated layout CPU ms.
  - `screenGuiMovers`: per-`ScreenGui` deltas for every layer whose counts changed.
