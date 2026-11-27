# Live sessions and the render service

`rur` exposes two programmatic interfaces:
1. **Live sessions (stdio):** Mount a story once and step it frame by frame with input events and control updates.
2. **Background render service (local HTTP):** Keeps VMs, GPU pipelines, fonts, and assets warm in memory for fast repeated renders (`rur render` uses this automatically).

## Live sessions (`rur-native engine ... --session 1`)

```sh
rur-native engine --repo <checkout> --story <file> --roots "<dir>;<dir>" --viewport 1200x900 --scale 1.5 --time 0 --session 1
```

Mounts the story and keeps the Luau VM alive. Send one JSON command per line on `stdin`; read replies on `stdout` from lines prefixed with `@@RUR ` (ignore other stdout lines). The initial reply is `ready`, containing the story's control schema and mount diagnostics.

| Command | Action | Reply |
|---|---|---|
| `{"cmd":"frame","dt":0.0167,"events":[...]}` | Dispatch input events, advance whole engine frames covering `dt` seconds, lay out, and export | Scene delta (changed nodes, `removed` IDs, updated draw lists/fonts/header) or `unchanged` |
| `{"cmd":"frame","dt":0.0083,"cadence":"display",...}` | Same at display refresh cadence: elapsed Heartbeats + one render step | Scene delta or `unchanged` |
| `{"cmd":"control","path":["Group","Name"],"value":...}` | Update a UI Labs / Flipbook control and advance one frame | Scene delta |
| `{"cmd":"scene"}` | Export the full current `scene.json` | Full scene |
| `{"cmd":"leaks"}` | Inspect unparented UI trees kept alive by references or connected handlers | Leak report |
| `{"cmd":"quit"}` | Terminate the session | *(none)* |

**Input event shapes** (logical viewport pixels):

```json
{"type":"move","x":600,"y":450}
{"type":"down","button":1,"x":600,"y":450}
{"type":"up","button":1,"x":600,"y":450}
{"type":"down","button":1,"x":300,"y":450,"finger":1}
{"type":"wheel","dx":0,"dy":120,"x":600,"y":450}
{"type":"leave"}
{"type":"key","key":"A","down":true}
{"type":"text","text":"x"}
```

Wheel `dy` is in logical pixels at `100` per notch (`notches` is also accepted). With `--device touch`, `move` / `down` / `up` take a `finger` (0 to 9, default 0), so several fingers can be down at once; an event without one is finger 0. Events follow Studio's per-frame order ([interaction.md](interaction.md)). Including a `presentation` packet in `frame` returns `ScrollingFrame` spring state so a UI host can predict scrolling at display refresh rate.

See `tools/engine/golden.mjs` (`live` entries) for a complete reference client. The desktop storybook drives the same session API in-process.

## How a render runs

By default, `rur render` talks to `rur-native serve` (`native/src/serve.rs`), auto-started in the background by `cli/client.mjs`. Keeping the service warm eliminates startup cost across repeated renders:

- **Standby VM:** Keeps a pre-mounted Luau VM ready for the last startup config (roots, viewport, scale, mode, snapshot dir, mounts), plus cached module sources and bytecode. Invalidated automatically when `data/`, project directory layout, mount-time files, snapshots, or the cloud font index change (using filesystem watches on Windows via `native/src/watch.rs`). Script sources are read on `require`, so edits always take effect immediately.
- **Warm GPU & assets:** Keeps the Vulkan discrete GPU device, compiled shaders, FreeType rasterizer, decoded images, asset resolver, and `ror` viewport worker (started lazily on first `ViewportFrame`) resident in memory.
- **Kept runs:** Retains the last rendered story's live VM at its final timestamp. A follow-up `rur render --time <t2>` on the exact same story/options with `t2 >= t1` resumes directly from `t1` instead of remounting—making animation frame sweeps fast. Dropped automatically if options, story, files, or past actions differ.
- **Lifecycle:** One service runs per checkout + state directory, logging to `<state dir>/serve.log`, reclaiming memory between bursts, and exiting 30 s after its last request.

Pass `--no-service` or `--isolated` (or `RUR_ISOLATED=1`) to run `rur-native render --dir <out>` in a fresh process instead (`rur sweep` and `rur compare` also run out-of-service). `--isolated` additionally pins the last built binary in `<state dir>` so edits to `native/` do not trigger rebuilds mid-run.


## Render service HTTP API

Binds to `127.0.0.1` only on a deterministic port derived from checkout + state directory (`4180–4579`, override with `RUR_SERVE_PORT`). Every request requires a local `Host` header and `X-RUR: 1`.

| Endpoint | Request body | Response |
|---|---|---|
| `GET /health` | — | `{ service, root, stateDir, pid, port, code, executable, renders }` |
| `GET /stories?project=<dir>` | — | `[{ file, place, key, slug }]` |
| `POST /render` | `{ argv, out, png, offline, env }` | `{ result }` (files written to `out`) |
| `POST /render` (`"pixels": "rgba"`) | Same as above + `"pixels": "rgba"` | `Content-Type: application/x-rur-pixels`: one JSON line (`result`, `width`, `height`, `summary`), `\n`, then raw straight-alpha RGBA8 bytes |
| `POST /sequence` | `{ argv, out, offline, env, sequence: { from, to, frames, duration, fps, crop, pad, png, scenes, video } }` | `{ result }` |
| `GET /memory[?collect=1]` | — | Process memory breakdown |
| `POST /shutdown` | — | Stops the service |

`argv` holds engine CLI arguments (`cli/rur.mjs runtimeArgs`). If a request's engine environment (`RUR_CLOCK`, `RUR_ORDER`, `RUR_PLAYER`, etc.) differs from the running service, it returns `HTTP 409` and the client falls back to a one-shot process.
