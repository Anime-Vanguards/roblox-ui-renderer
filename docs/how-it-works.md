# How it works

Everything that runs or draws a story is native (`native/`, Rust):

- the emulated engine, [roblox-engine-emulator](https://github.com/Anime-Vanguards/roblox-engine-emulator)
  (instances, properties, signals, the scheduler, services, project mounting, the 3D world and a recorder,
  with your Luau running on an embedded, patched Luau VM), plus the GUI layer rur adds to it
  (`native/src/engine`): layout, text, input, the scene export, story mounting and sessions;
- the renderer (`native/src/render`, wgpu): Studio's measured raster rules for fills, borders, UICorner,
  UIStroke, UIGradient, images and 9-slice, text, shadows, CanvasGroups, Path2D strokes, scroll bars and rotation;
- the glyph rasterizer (`native/src/glyphs.rs`, `glyphs/rur_glyph.c`): FreeType 2.14.2, compiled in.

One binary (`rur-native`) is the desktop storybook, the renderer of `rur render` (one-shot or through a warm
render service), the headless exporter and the engine CLI. The Node side (`cli/`) is the command line:
options, projects, git refs, Studio comparisons, asset downloads, inspection and reports.

## Source layout

- [roblox-engine-emulator](https://github.com/Anime-Vanguards/roblox-engine-emulator): the emulated engine
  (instances, properties and defaults from Studio's reflection data, enums, datatypes, deferred signals, a
  virtual-time scheduler, services, `require` over a project's layout, the 3D world and the recorder).
- `native/src/engine`: rur's side, installed into every engine as a layer: `layout/` (UDim2, AutomaticSize,
  list, grid and flex layouts, constraints, scrolling, text measurement) writing `Absolute*` and
  `TextBounds` lazily as the engine does; `text/` (OpenType parsing and shaping with metrics calibrated on
  thousands of Studio samples); `input/` (hit testing, GuiState, scrolling springs, TextBox editing, drag);
  `export.rs` (the scene JSON) and `export/retained.rs` (per-frame deltas); `story.rs` / `session.rs` (story
  mounting and live sessions); `cli.rs` (the engine CLI and the one-shot, standby and in-process hosts).
- `native/src/render`: the wgpu renderer. `scene_draw.rs` turns a scene into draw records in device pixels
  with Studio's snapping and blending rules; `shaders/uber.*.glsl` evaluate them; `coverage.rs`,
  `geometry.rs`, `sampling.rs`, `images.rs`, `groups.rs` and `text.rs` hold the measured raster rules.
- `native/src/glyphs.rs`: glyph atlases and hinted metrics with the compiled-in FreeType; `assets.rs`:
  decoded textures and glyph pages ([native/ASSETS.md](../native/ASSETS.md)); `resolve.rs`: content URIs to files.
- `native/src/story_render.rs`, `serve.rs`: `rur render`'s output and the render service; `headless.rs`:
  captures, benchmarks and batches; `app.rs`: the desktop storybook; `places.rs`: projects and places.
- `cli/`: the command line (`rur.mjs`), compare (`compare.mjs`, `studio.mjs`, `clock.mjs`), inspection,
  assets (`assets.mjs`, `native-assets.mjs`), projects (`places.mjs`, `rojo.mjs`, `gitref.mjs`).
- `fixtures/`: the measurement scenes; `tools/analysis`: the fitting scripts behind the rules;
  `tools/studio`, `tools/interaction`: Studio probes and input experiments; `data/studio`: frozen Studio
  measurements the tests compare with.

The rules themselves: the [documentation index](README.md).

## The engine: roblox-engine-emulator

The engine core is its own repository,
[roblox-engine-emulator](https://github.com/Anime-Vanguards/roblox-engine-emulator). rur depends on it as a
cargo git dependency pinned to a commit (`native/Cargo.toml`: the crate, and under `[patch.crates-io]` its
vendored patched Luau VM and the mlua-sys that reads its bytecode, all at the same commit) and adds its GUI layer
through the engine's layer hooks
(`native/src/engine/mod.rs`: `GuiLayer`). Instances, reflection, values, signals, the scheduler, services,
the Luau API, project mounting, the 3D world and the recorder are the emulator's; text, layout, input, the
scene export, story mounting, sessions and the engine CLI are rur's.

Changing both at once:

```powershell
node tools/emulator.mjs link       # build against ../roblox-engine-emulator (or: link <dir>) instead of the pin
node tools/emulator.mjs status     # the pin, the link, the checkout beside this one
node tools/emulator.mjs pin        # pin the checkout's pushed HEAD (Cargo.toml and Cargo.lock)
node tools/emulator.mjs unlink     # back to the pin
```

A link is an untracked `.cargo/config.toml` at this checkout's root that patches the git dependency with the
local path. The Luau VM stays the pinned commit's even when linked.


### How scripts run

Every run runs its Luau as a playing client and the emulator's `record` run it, whichever host runs it: a
one-shot `rur-native engine` run, the golden check's batch workers, `rur render` and `sequence`, the render
service and the desktop app (`native/src/engine/cli.rs`, `interpret`). Every chunk is interpreted in a script
environment that is safe as a client's is: a module's imports (`game`, `script.Parent.Child`, `math.floor`) are
resolved when it loads, and builtins are fast calls. The scripts' own Luau is charged on the emulator's work clock
at a client's prices, a `--!native` module's instructions at native code's (the emulator's docs/engine.md, "Work
clock"), so `os.clock` moves by the same work in every host, and UI code that budgets its work by `os.clock` takes
the same frames in a one-shot render, the render service and `record`. No host compiles Luau to native code:
natively compiled code would run unseen by the VM's work counters, and compiling every module cost a run more
than it saved (the golden corpus takes about a third less CPU interpreted).
