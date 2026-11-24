# Deterministic UI cost (`rur cost`)

`rur cost` runs a deterministic, CPU-only accounting pass over a Roblox UI scene (or a multi-frame sequence) to count what Roblox's 2D UI renderer would do, without initializing a GPU device or recording host wall-clock timings.

Every `cost.json` output contains zero wall-clock fields and zero output-directory paths, so repeated runs of the same input are bit-identical even under heavy host load.

## Commands

```powershell
# Single-moment UI cost (fixture, story, module, or model)
node cli/rur.mjs cost --story path/to/Hud.story.luau --viewport 1920x1080 --scale 1.5 --out out/hud-cost

# Full PlayerGui + CoreGui cost across every enabled ScreenGui
node cli/rur.mjs cost --fixture path/to/hud.luau --all-layers --out out/hud-all

# Multi-frame sequence layout churn (CPU-only, no GPU/PNG/video)
node cli/rur.mjs cost --story path/to/Anim.story.luau --from 0.5 --frames 60 --fps 60 --out out/anim-cost

# Capture frames/video AND deterministic cost.json in one pass
node cli/rur.mjs sequence --story path/to/Anim.story.luau --from 0 --frames 60 --fps 60 --cost --out out/anim-seq

# Analyze a saved scene.json (note: hiddenInShown.complete is false without a live DOM)
node cli/rur.mjs cost --scene out/hud/scene.json --out out/hud-cost

# Deterministic A/B diff between two cost.json reports
node cli/rur.mjs cost compare out/before/cost.json out/after/cost.json --json
node cli/rur.mjs cost --story path/to/Hud.story.luau --compare out/before/cost.json --out out/after
```

`rur cost` delegates directly to `rur-native cost`, so standalone release binaries support every flag without Node. Pass `--json` for single-line JSON on stdout; `--out` accepts either a directory (writes `<dir>/cost.json`) or a `.json` file path.

## What `rur cost` counts

### 1. Roblox `Pass2d` draw model

`rur`'s own wgpu renderer uses an uber-shader that merges dissimilar primitives into very few batches. `rur cost` intentionally models **Roblox's** 2D UI batching state rather than `rur`'s:

- **Batch state key (per render target):** `(layerPass, shader, texture, scissorRect, rotated)`
- **Batch break causes (`totals.batchBreaksByCause`):**
  - `layerPass`: switching between `PlayerGui` and `CoreGui` passes.
  - `renderTarget`: compositing a `CanvasGroup` offscreen texture onto its parent target.
  - `clip`: changing the active scissor clip rectangle (`ClipsDescendants` / scroll window / text self-clip).
  - `shader`: switching between `solid`, `styled_fill` (`UICorner` or `UIGradient`), `image`, `styled_image`, `text`, `stroke`, `shadow`, `path2d`, and `group_composite`.
  - `texture`: changing `Image` URI between adjacent image quads.
  - `rotation`: switching between axis-aligned and rotated elements.
- **Quads (`totals.quads`, `totals.quadsByKind`):** non-glyph UI quads broken down by kind (`solid`, `styled_fill`, `border`, `image`, `styled_image`, `shadow`, `stroke`, `scrollbar`, `group_composite`, `path2d`).
- **Glyph quads (`totals.glyphQuads`):** visible non-space fill glyph quads (`fillGlyphQuads`) plus stroke glyph quads (`strokeGlyphQuads`: 4 offset copies per glyph for legacy `TextStrokeTransparency < 1`, plus 1 copy per glyph for each enabled `ApplyStrokeMode.Contextual` `UIStroke` on a text object). Clipped-out lines are excluded; `MaxVisibleGraphemes` cuts are honored.
- **Overdraw (`totals.screenOverdrawViewports`, `totals.offscreenOverdrawViewports`):** clipped physical pixel area summed over drawn quads and visible text line boxes, divided by the physical viewport area (`physicalWidth * physicalHeight`), separated into main-screen vs. `CanvasGroup` offscreen render-target overdraw.

### 2. `CanvasGroup` offscreen render targets

Offscreen target sizing uses the exact Studio rules established in [images.md](images.md) (`native/src/render/geometry.rs::group_texture_dims`):

- Rectangular RGBA8 allocation `(nw, nh)` from the group's unrotated snapped device rectangle and float32 logical size (`nw * nh * 4` bytes).
- Large groups scale uniformly so neither side exceeds `2048` device px and area stays within `2048 x 1024` px.
- Groups that are `culled`, `Visible = false`, zero-sized, or have quantized `alpha8(1 - GroupTransparency) == 0` allocate `0` bytes and perform `0` RT switches.
- Each drawn target counts `2` render-target switches (`totals.canvasRtSwitches`) and tracks nesting depth (`totals.maxCanvasGroupDepth`).

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

- Incremental `Layout.Stats` counters for that frame (`passes`, `computed`, `memo`, `measures`, `stored`, `skipped`, `moved`, `texts`, `scrollMoved`). Static UI incurs `0` layout churn on frames `1..N`; animated or resizing UI reports the exact objects laid out each frame.
- `unchanged`, `changedNodes`, `removedNodes`, and full per-frame `totals` matching single-moment `rur cost --time <t>` when `fps` divides `60`.
- `sequence.summary`: `mean`, `peak`, and `p95` across all frames, plus steady-state (`frames[1..]`) layout churn mean and calibrated layout CPU ms.

### 5. Attribution, calibration, and explicit refusals

- **Attribution:** `screenGuis` attributes draw calls, batch breaks, quads, glyph quads, CanvasGroup RT bytes/switches, overdraw, and hidden-in-shown counts per `ScreenGui` (enabled ScreenGuis sum conservatively to `totals`; disabled ScreenGuis appear with `enabled: false`). `objects` lists the heaviest drawn objects and `CanvasGroup`s.
- **Calibrated layout CPU ms (`predicted.*.layoutCpuMs`):**
  - `desktop-ref` (`basis: "measured"`): Studio 0.744 client lazy-layout calibration (`native/src/engine/layout/install.rs`): `2.3 us` per pass + `0.312 us` per `stored + moved` record + `62 us` when scrolling moved records.
  - `phone-mid` (`basis: "anchored"`): anchored to `desktop-ref` using a recorded mid-tier mobile UI main-thread ratio (`3.1493x`, uncertainty band `bandMs: [2.0405x, 6.16x]`).
- **Explicit refusals (`status: "refused"`, `ms: null`):** `drawPassCpuMs` and `gpuUiMs` are explicitly refused on both tiers with `reason` and `settledBy` because no per-element Studio or mobile GPU calibration backs them yet. `unmodeled` enumerates all excluded cost dimensions.

## A/B comparison (`rur cost compare`)

`rur cost compare <before> <after>` outputs a deterministic `rrr-ab/1` report with:

- `verdict`: `improved`, `regressed`, `mixed`, or `unchanged` (with `measurementNoise: 0.0`).
- `counts`: `before`, `after`, and `delta` across draw calls, batch breaks, quads, glyph quads, CanvasGroup RT bytes/targets/switches, screen/offscreen overdraw, and hidden-in-shown `GuiObject`s.
- `layout` and `predicted`: before/after/delta layout counters and calibrated layout CPU ms.
- `screenGuiMovers`: per-`ScreenGui` deltas for every layer whose counts changed.
