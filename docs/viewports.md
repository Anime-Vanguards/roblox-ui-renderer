# ViewportFrames

rur exports what a ViewportFrame shows (its models, camera, lighting, its fill and the place's materials) as a
content-addressed snapshot and asks a separate 3D renderer to draw it. The picture goes through the normal
image path. WorldModels are traversed as containers. While a new image is pending, the last one stays visible.

Roblox Studio (0.747, measured with `fixtures/viewport-probe-fade.luau` and `viewport-probe-layer.luau`) draws
a ViewportFrame's world into a target cleared to the frame's fill (BackgroundColor3 at 1 -
BackgroundTransparency), at the frame's logical size whatever the device scale, and then draws that target as
the frame's image:

- the fill and the world are one layer, the world replacing the fill where it covers it; ImageColor3 tints
  the fill too, ImageTransparency fades it (at 1 the frame shows nothing but its border), and a UIGradient,
  UICorner and Rotation apply to the layer as to an image;
- the border and a UIStroke are drawn apart, over the layer, neither tinted nor faded; GUI children as usual;
- a part's anti-aliased edge averages its samples as they are, so the fill's colour enters at full strength
  whatever its opacity (a dark fringe round a model at BackgroundTransparency 1), and the mean is clipped;
- at 1.5x the picture is stretched over the frame, so a part's edge ramps over about three device pixels;
- the parts take the place's built-in materials: a place saved without MaterialService.Use2022Materials (a
  new place file without a MaterialService) shows the pre-2022 materials in its frames as in its world
  (`fixtures/viewport-probe-materials.luau`; scripts cannot read the property, so a project says which its
  places use with the rur.json key `use2022Materials`, docs/projects.md).

rur draws it so: the snapshot asks for the picture at the frame's logical size, carries the frame's fill
(`background`), and the renderer draws a ViewportFrame whose picture is ready as that picture plus its border.

Without ror, a ViewportFrame draws only its own background (what Studio shows for an empty viewport), and
the asset diagnostics say how to get ror.

## Setting up ror

[roblox-object-renderer](https://github.com/Anime-Vanguards/roblox-object-renderer) (ror, 0.5.0 or later)
draws the 3D content: `ror viewport-worker` draws Roblox models the way Studio does. rur starts it by itself,
from the desktop app, headless renders and the CLI alike; it needs no checkout of rur and no Node.js. Install
ror any one way:

- `rokit add --global Anime-Vanguards/roblox-object-renderer ror` (rokit puts it on your `PATH`);
- a zip from [ror's releases](https://github.com/Anime-Vanguards/roblox-object-renderer/releases), with
  `ror` on your `PATH` or `RUR_VIEWPORT_WORKER` set to its path;
- a ror checkout built beside a rur checkout (`cargo build --release` in `../roblox-object-renderer`).

rur looks for `RUR_VIEWPORT_WORKER`, then (in a checkout build) `../roblox-object-renderer/target/release/ror`,
then `ror` on the `PATH`. A ror too old to have the command is named in the diagnostics with its version.

## The worker protocol

rur (`native/src/viewports.rs`) starts the worker as `<executable> viewport-worker [--offline]` and keeps it
running; it exits when its stdin closes. Any program that answers these requests works in ror's place. The
protocol is one JSON request per stdin line and one JSON reply per stdout line, one request at a time:

| Request | Reply |
|---|---|
| `{ "viewport": <snapshot>, "out": "<file.png>" }` | `{ "file": "<file.png>", "background": true }` once `out` holds the picture, or `{ "error": "..." }` |
| `{ "command": "catalog", "av": "<project dir>" }` | `{ "bundles": ["<key>", ...] }`: the model bundles a project's manifest offers |
| `{ "command": "bundle", "av": "<project dir>", "path": "<key>", "out": "<file.rbxm>" }` | `{ "file": "<file.rbxm>" }` |

A snapshot holds the size to draw (the frame's logical size), its CurrentCamera (CFrame, FieldOfView),
`background` (the frame's fill: `color` 0..1, `transparency`), and the instances: the ViewportFrame first
(with its Ambient, LightColor and LightDirection) and those under it with their properties (MeshParts keep
`InitialSize`, so meshes are scaled by `Size / InitialSize`), then the place's MaterialService as another
root (`Use2022MaterialsXml` and its MaterialVariants). The worker draws the world over `background` as
Studio's target does and says so with `"background": true`; from a worker that returns a transparent picture
rur lays the picture over the fill itself (a texel covered in part, at coverage c, takes the straight mean of
its colour and the fill's at opacity c + b (1 - c)). Images
are capped at 2048 px per axis; moving viewports are coalesced and each keeps one image. A request that takes
over 90 seconds stops the worker, and the next request starts it again.

One-shot renders keep each snapshot's image in `<state dir>/viewports-persisted/` (the newest 512), keyed by
the snapshot and the worker build, so a restarted render service doesn't start the worker for images it
drew before.

ror's worker answers the `viewport` requests (and an `asset` command, ror's docs/viewport-worker.md); it has
no model bundles, which are optional: a worker with them lets a project mount models lazily, and
`RUR_BUNDLE_MANIFEST` names the manifest module whose text keys the cached catalog.


Smoke test, with ror installed:

```powershell
node cli/rur.mjs render --fixture fixtures/viewport-models.luau --viewport 820x360 --scale 1 --out out/viewport-models --no-service
node tools/tests/viewports.mjs
```
