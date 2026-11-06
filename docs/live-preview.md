# Live preview: the session protocol and the render service

Two ways to drive rur from another program: a live session over stdio (a story that keeps running and
takes input frame by frame), and the render service (warm one-shot renders over local HTTP), which is also
how `rur render` runs.

## Live sessions (`rur-native engine ... --session 1`)

```
rur-native engine --repo <checkout> --story <file> --roots "<dir>;<dir>" --viewport 1200x900 --scale 1.5 --time 0 --session 1
```

The story mounts and keeps running. The program sends one JSON command per stdin line and gets one reply
per command on stdout, on a line that starts with `@@RUR ` (other stdout lines are not replies). The
first reply is `ready`; it lists the story's controls and the mount's diagnostics.

| Command | Effect | Reply |
|---|---|---|
| `{"cmd":"frame","dt":0.0167,"events":[...]}` | dispatches the input events, then runs whole engine frames covering `dt` seconds, lays out and exports | a frame: the scene delta (changed nodes, `removed` ids, draw lists, fonts and header when they changed), or `unchanged` |
| `{"cmd":"frame","dt":0.0083,"cadence":"display",...}` | the same at display cadence: the Heartbeats that elapsed and one render step, as a high refresh rate display steps | as above |
| `{"cmd":"control","path":["Group","Name"],"value":...}` | sets a UI Labs control (its Vide source, or the plain value) and runs the frame it causes | a frame |
| `{"cmd":"scene"}` | the full export of the current state, as a one-shot render writes `scene.json` | a scene |
| `{"cmd":"leaks"}` | what the story keeps alive of UI it removed (trees outside the DataModel and the handlers holding them) | a report |
| `{"cmd":"quit"}` | ends the session | none |

Input events in a frame command, in logical viewport px:

```json
{"type":"move","x":600,"y":450}
{"type":"down","button":1,"x":600,"y":450}
{"type":"up","button":1,"x":600,"y":450}
{"type":"wheel","dx":0,"dy":120,"x":600,"y":450}
{"type":"leave"}
{"type":"key","key":"A","down":true}
{"type":"text","text":"x"}
```

A wheel's `dy` is in logical px at 100 per notch (`notches` works too). Events of one frame run in order
before the frame's movement update, as Studio does ([interaction.md](interaction.md)). A frame command
with a `presentation` packet asks for predicted scrolling: the reply carries every ScrollingFrame's
scroll state so a client can move scrolled content at display rate between engine frames.

`tools/engine/golden.mjs` drives sessions this way for its `live` entries; read it for a complete client.
The desktop storybook runs the same session in process.

## How a render runs

`rur render` goes through the **render service**, `rur-native serve` (native/src/serve.rs), which
`cli/client.mjs` starts in the background on first use. It keeps warm what a render would otherwise start
every time:

- a **standby VM** for the last startup configuration (roots, viewport, scale, mode, snapshot dir,
  mounts): an engine thread with the Luau VM and the project already mounted, so a render only mounts its
  story, runs its virtual time, lays out and exports. Module sources and bytecode are kept between VMs;
- the headless **GPU device** with the renderer's pipelines, the **glyph rasterizer** (fonts) and decoded
  images;
- the **image resolver** (local files, cached assets and thumbnails; a Node helper, `cli/native-assets.mjs`,
  starts only for downloads and cloud fonts).

So a render of a known project costs the story's own Luau plus a draw. A standby is used only while what it
read at load is unchanged (the renderer's `data/`, the project's directory structure and the files read at
mount, Studio snapshots, the cloud font index). On Windows the service watches those trees
(`native/src/watch.rs`) instead of listing them for every render. Script sources are read at require time,
so an edit is always seen. The service exits 30 s after its last render.

**Kept runs.** The service keeps the last render's run: its VM with the story mounted and the clock at the
captured moment. A render of the same story with the same options at that moment or a later one goes on from
it, so a loop of `rur render --time t` over an animation costs a frame, a layout, an export and a draw per
moment instead of a mount. It is let go when the new render can't go on exactly (another story or options,
an earlier moment, a file it read changed, an action that would have to run in the past).

Its GPU device prefers a discrete GPU through Vulkan. Between bursts of renders it gives memory back
(`GET /memory` reports where it is). There is one service per checkout and state dir. Log:
`<state dir>/serve.log`.

Without the service (`--no-service`, `--isolated`, the sweep's child processes, `rur compare`),
`rur-native render --dir <out> <engine arguments>` does the same work in a process of its own, with the same
files and pixels. `--isolated` (or `RUR_ISOLATED=1`) also keeps the engine binary frozen while `native/` is
being edited: it runs the state dir's last build even when the sources changed since.


## The render service's HTTP API

The service listens on 127.0.0.1 only, on a port derived from the checkout and state dir (4180-4579;
`RUR_SERVE_PORT` pins it). Every request needs the header `X-RUR: 1` and a local `Host`.

| Route | Body | Answer |
|---|---|---|
| `GET /health` | | `{ service, root, stateDir, pid, port, code, executable, renders }` |
| `GET /stories?project=<dir>` | | the project's stories with their places: `[{ file, place, key, slug }]` |
| `POST /render` | `{ argv, out, png, offline, env }` | `{ result }`, the files of a render written to `out` |
| `POST /render` with `"pixels": "rgba"` | as above | one JSON line (`result`, `width`, `height`, `summary`), a newline, then the straight-alpha RGBA8 rows render.png would hold; `Content-Type: application/x-rur-pixels` |
| `POST /sequence` | `{ argv, out, offline, env, sequence: { from, to, frames, duration, fps, crop, pad, png, scenes, video } }` | `{ result }`: frames captured at exact moments |
| `GET /memory[?collect=1]` | | where the process's memory is |
| `POST /shutdown` | | |

`argv` holds engine arguments (`--story`, `--roots`, `--viewport`, ...; `cli/rur.mjs runtimeArgs` builds
them). A render whose engine environment (`env`: `RUR_CLOCK`, `RUR_ORDER`, `RUR_PLAYER`, ...) differs
from the service's is refused with 409; the client then runs it in a process of its own.
