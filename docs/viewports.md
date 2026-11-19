# Rendering 3D ViewportFrames

`rur` renders `ViewportFrame` contents by exporting a content-addressed 3D snapshot (models, camera, lighting, background fill, and place materials) to a persistent 3D worker process, then compositing the resulting PNG through the normal UI image pipeline. `WorldModel` descendants are traversed as containers, and the previous image stays visible while a new snapshot renders.

Without a 3D worker installed, `ViewportFrame`s render only their background fill (matching an empty Studio viewport) and diagnostics explain how to install `ror`.

## Studio compositing rules

Measured in Studio 0.747 (`fixtures/viewport-probe-fade.luau`, `viewport-probe-layer.luau`, `viewport-probe-materials.luau`):
- Studio renders the 3D world at the frame's **logical size** (independent of DPI scale) into a render target cleared to `BackgroundColor3` at opacity `1 - BackgroundTransparency`, then composites that target as the frame's image.
- Because the background fill and 3D scene share one layer, `ImageColor3`, `ImageTransparency`, `UIGradient`, `UICorner`, and `Rotation` apply to both together (at `ImageTransparency = 1`, only the border remains).
- Borders, `UIStroke`, and GUI children render separately on top without being tinted or faded by `ImageColor3`/`ImageTransparency`.
- Anti-aliased part edges average against the full-strength `BackgroundColor3` regardless of `BackgroundTransparency` (producing a dark fringe when `BackgroundTransparency = 1` over a dark fill). At 1.5x DPI, upscaling softens part edges across ~3 device pixels.
- Parts use the place's material generation (places without `MaterialService.Use2022Materials` use pre-2022 materials; configure per root via `use2022Materials` in [rur.json](projects.md#additional-rurjson-settings)).

## Installing `ror`

[roblox-object-renderer](https://github.com/Anime-Vanguards/roblox-object-renderer) (`ror` 0.5.0+) provides `ror viewport-worker`, which `rur` spawns automatically across CLI renders, release binaries, and the desktop storybook (no Node.js or `rur` checkout required).

Install `ror` using any of:
- `rokit add --global Anime-Vanguards/roblox-object-renderer ror` (rokit puts it on your `PATH`);
- Download a release zip from [ror Releases](https://github.com/Anime-Vanguards/roblox-object-renderer/releases) and put `ror` on your `PATH` (or set `RUR_VIEWPORT_WORKER` to its path).
- Build a sibling checkout alongside `rur` (`cargo build --release` in `../roblox-object-renderer`).

Lookup order: `RUR_VIEWPORT_WORKER` → `../roblox-object-renderer/target/release/ror` (source builds only) → `ror` on `PATH`. If an older `ror` binary lacks `viewport-worker`, diagnostics report its version.

## Worker JSON protocol

`rur` (`native/src/viewports.rs`) launches `<executable> viewport-worker [--offline]` and communicates over stdin/stdout using one JSON object per line, one request at a time, shutting down when stdin closes. Any compatible executable can substitute for `ror`:

| Request | Reply |
|---|---|
| `{ "viewport": <snapshot>, "out": "<file.png>" }` | `{ "file": "<file.png>", "background": true }` or `{ "error": "..." }` |
| `{ "command": "catalog", "av": "<project dir>" }` *(optional)* | `{ "bundles": ["<key>", ...] }` |
| `{ "command": "bundle", "av": "<project dir>", "path": "<key>", "out": "<file.rbxm>" }` *(optional)* | `{ "file": "<file.rbxm>" }` |

**Snapshot contents:**
- Logical `width`/`height` (capped at 2048 px per axis), `CurrentCamera` (`CFrame` + `FieldOfView`), and `background` (`color` `0..1`, `transparency`).
- `instances`: `ViewportFrame` first (`Ambient`, `LightColor`, `LightDirection`), followed by descendants (`MeshPart`s include `InitialSize` for `Size / InitialSize` scaling), plus the place's `MaterialService` (`Use2022MaterialsXml` and `MaterialVariant`s) as an additional root.
- If the worker renders over `background`, it returns `"background": true`. If it returns a transparent PNG instead, `rur` composites it over the fill (`opacity = c + b * (1 - c)`).

**Caching & timeouts:**
- Rapid viewport updates coalesce so each slot keeps only its latest render. Requests exceeding 90 seconds terminate the worker and spawn a fresh one on the next job.
- One-shot renders persist the newest 512 rendered viewport PNGs in `<state dir>/viewports-persisted/` keyed by snapshot hash and worker build, avoiding worker startup on repeated renders.
- `catalog`/`bundle` are optional lazy-model hooks (`RUR_BUNDLE_MANIFEST` keys the catalog cache); `ror`'s stock worker handles `viewport` (and `asset`, see `ror`'s `docs/viewport-worker.md`).


## Smoke test

With `ror` installed:

```powershell
node cli/rur.mjs render --fixture fixtures/viewport-models.luau --viewport 820x360 --scale 1 --out out/viewport-models --no-service
node tools/tests/viewports.mjs
```
