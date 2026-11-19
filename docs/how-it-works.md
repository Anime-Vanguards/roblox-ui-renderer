# How rur works

Everything that executes Luau, lays out UI, rasterizes glyphs, and draws pixels runs natively in Rust (`native/`). A single binary (`rur-native`) powers the desktop storybook, headless CLI renders, the background render service, and the engine CLI. The Node layer (`cli/`) handles CLI argument parsing, project/Rojo helpers, Studio comparisons, inspection commands, and reports.

## Architecture and source layout

- **Core engine ([roblox-engine-emulator](https://github.com/Anime-Vanguards/roblox-engine-emulator)):** Instances, Studio reflection properties/defaults, enums, datatypes, deferred signals, virtual-time scheduler, services, `require` over project layouts, the 3D world, and the 3D recorder—running Luau in an embedded, patched Luau VM.
- **GUI layer (`native/src/engine/`):** Installed onto every engine instance via `GuiLayer` (`engine/mod.rs`):
  - `layout/`: UDim2, `AutomaticSize`, list/grid/flex/page/table layouts, constraints, scrolling, and lazy `Absolute*` / `TextBounds` computation.
  - `text/`: OpenType parsing and text shaping calibrated against Studio datasets.
  - `input/`: hit testing, `GuiState`, scroll springs, `TextBox` editing, and dragging.
  - `export.rs` & `export/retained.rs`: full `scene.json` export and incremental per-frame deltas.
  - `story.rs`, `session.rs`, `cli.rs`: story mounting, live stdio sessions, and one-shot/standby/in-process hosts.
- **GPU renderer (`native/src/render/`):** wgpu renderer with `shaders/uber.*.glsl`, `scene_draw.rs`, and Studio-measured raster rules (`coverage.rs`, `geometry.rs`, `sampling.rs`, `images.rs`, `groups.rs`, `text.rs`).
- **Glyphs and assets:** `native/src/glyphs.rs` + `glyphs/rur_glyph.c` (compiled-in FreeType 2.14.2), `assets.rs` (decoded texture/atlas store, see [native/ASSETS.md](../native/ASSETS.md)), `resolve.rs` (URI resolution), and `download.rs` (HTTP downloads, auth, `rur-native assets`).
- **App, service, and CLI commands:** `story_render.rs`, `serve.rs`, `headless.rs`, `app.rs` (desktop storybook), `places.rs`, `sweep.rs`, `diff.rs`, `story_args.rs`, `summary.rs`, and `gitref.rs`.
- **Node CLI & test tooling:** `cli/` (`rur.mjs`, `compare.mjs`, `studio.mjs`, `clock.mjs`, `inspect.mjs`, `assets.mjs`, `places.mjs`, `rojo.mjs`, `gitref.mjs`), `fixtures/` (measurement scenes), `tools/analysis/` (curve fitting), `tools/studio/` & `tools/interaction/` (Studio probes), and `data/studio/` (frozen Studio measurements).

## The `roblox-engine-emulator` dependency

`rur` pulls [roblox-engine-emulator](https://github.com/Anime-Vanguards/roblox-engine-emulator) as a pinned Cargo git dependency in `native/Cargo.toml` (including `[patch.crates-io]` entries for its vendored patched Luau VM and `mlua-sys` at the same commit).

To develop changes across both repositories simultaneously:

```powershell
node tools/emulator.mjs link       # Point Cargo at ../roblox-engine-emulator (or: link <dir>)
node tools/emulator.mjs status     # Show current pin, link state, and sibling checkout
node tools/emulator.mjs pin        # Update Cargo.toml + Cargo.lock to the sibling's pushed HEAD
node tools/emulator.mjs unlink     # Remove local path override
```

`link` writes an untracked `.cargo/config.toml` patching the git source to a local path. (The vendored Luau VM stays at the pinned commit even while linked.)


### How scripts run

Every host—one-shot `rur-native engine`, golden batch workers, `rur render`, `rur sequence`, the render service, and the desktop storybook—executes Luau identically (`native/src/engine/cli.rs`, `interpret`):

- Every chunk runs in the Luau **interpreter** with client-safe globals, resolved module imports (`game`, `script.Parent.Child`, `math.floor`), and fastcall builtins.
- Luau execution advances the emulator's virtual work clock (`os.clock`) at Roblox client prices (charging `--!native` modules at native-code prices; see `ree`'s `docs/engine.md`). Because every host uses the same interpreter and clock, frame budgets driven by `os.clock` behave identically across renders, services, and `record`.
- Native codegen is intentionally disabled: native code bypasses the VM's instruction work counters, and compiling every module costs more CPU than it saves (the golden corpus uses ~1/3 less CPU interpreted).
