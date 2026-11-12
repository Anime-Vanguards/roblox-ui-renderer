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

Without a 3D renderer, a ViewportFrame draws only its own background (what Studio shows for an empty
viewport) and the summary says why.

## The viewport worker

`cli/viewports.mjs` starts the worker as `<executable> viewport-worker [--offline]` and keeps it running.
The executable is `RUR_VIEWPORT_WORKER`, else `../roblox-object-renderer/target/release/ror(.exe)` beside
this checkout. The protocol is one JSON request per stdin line and one JSON reply per stdout line, one
request at a time:

| Request | Reply |
|---|---|
| `{ "viewport": <snapshot>, "out": "<file.png>" }` | `{ ... }` once `out` holds the image, or `{ "error": "..." }` |
| `{ "command": "catalog", "av": "<project dir>" }` | `{ "bundles": ["<key>", ...] }`: the model bundles a project's manifest offers |
| `{ "command": "bundle", "av": "<project dir>", "path": "<key>", "out": "<file.rbxm>" }` | `{ "file": "<file.rbxm>" }` |

A snapshot holds the size to draw (the frame's logical size), its CurrentCamera (CFrame, FieldOfView),
`background` (the frame's fill: `color` 0..1, `transparency`), and the instances: the ViewportFrame first
(with its Ambient, LightColor and LightDirection) and those under it with their properties (MeshParts keep
`InitialSize`, so meshes are scaled by `Size / InitialSize`), then the place's MaterialService as another
root (`Use2022MaterialsXml` and its MaterialVariants). A worker draws the world over `background` as Studio's
target does and replies `"background": true`; from a worker that returns a transparent picture rur lays the
picture over the fill itself (a texel covered in part, at coverage c, takes the straight mean of its colour
and the fill's at opacity c + b (1 - c)). Images are capped at 2048 px per axis; moving viewports are coalesced
and each keeps one image.

One-shot renders keep each snapshot's image in `<state dir>/viewports-persisted/` (the newest 512), keyed by
the snapshot and the worker build, so a restarted render service doesn't start the worker for images it
drew before.

[roblox-object-renderer](https://github.com/Anime-Vanguards/roblox-object-renderer) draws Roblox models
the way Studio does. The maintainers' worker is built on it and is not published, and roblox-object-renderer
does not have the `viewport-worker` command yet, so for now a worker is something you bring: any program
that answers the requests above works. Model bundles are optional: `RUR_BUNDLE_MANIFEST` names the manifest
module whose text keys the cached catalog.


Smoke test, with a worker built:

```powershell
node cli/rur.mjs render --fixture fixtures/viewport-models.luau --viewport 820x360 --scale 1 --out out/viewport-models --no-service
node tools/tests/viewports.mjs
```
