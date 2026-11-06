# ViewportFrames

rur exports what a ViewportFrame shows (its models, camera, lighting) as a content-addressed snapshot and
asks a separate 3D renderer to draw it. The resulting transparent image goes through the normal image path:
tint, transparency, clipping, rounded corners and CanvasGroups apply as for any image. WorldModels are
traversed as containers. While a new image is pending, the last one stays visible.

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

A snapshot holds the ViewportFrame's size, its CurrentCamera (CFrame, FieldOfView), Ambient, LightColor,
LightDirection and the instances under it with their properties (MeshParts keep `InitialSize`, so meshes
are scaled by `Size / InitialSize`). Images are capped at 2048 px per axis; moving viewports are coalesced
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
