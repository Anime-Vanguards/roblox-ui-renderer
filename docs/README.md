# rur documentation

Reference guides for running `rur`, understanding its Studio parity rules, and extending the codebase. For an overview and quick start, see the [README](../README.md).

| Guide | Covers |
|---|---|
| [setup.md](setup.md) | Release binaries, verification, build requirements, `rur doctor`, state directories, and output roots |
| [render.md](render.md) | CLI commands: `render`, `sequence`, `cost`, `--git-ref`/`diff`, `--plugin`, `--all-layers`, `sweep`, `record`, `profile`, and `inspect` |
| [cost.md](cost.md) | Deterministic Roblox UI cost accounting (`rur cost`, `rur cost compare`, `rur sequence --cost`), CanvasGroup RT bytes, hidden-DOM traversal, layout churn, and calibration/refusal contract |
| [storybook.md](storybook.md) | Desktop storybook (`rur native`), headless captures, benchmarks, and memory leak inspection |
| [web.md](web.md) | WebAssembly browser build, asset bundles, and frame-identity testing |
| [assets.md](assets.md) | Built-in textures, fonts, `rbxassetid://` downloads, Open Cloud API key, cookies, account broker, and Studio image reads |
| [how-it-works.md](how-it-works.md) | Architecture, source layout, `roblox-engine-emulator` dependency, and script execution |
| [projects.md](projects.md) | Projects, `rur.json` places, Rojo auto-detection, explicit mounts, and environment variables |
| [stories.md](stories.md) | UI Labs, Flipbook, module, model, and fixture formats; controls; timed `--actions` |
| [layout.md](layout.md) | Measured Studio layout rules: float32 math, `AutomaticSize`, lists/flex/grids, scrolling, `Path2D`, and text boxes |
| [text.md](text.md) | Text modes (whole-pixel vs. quarter-pixel), fonts, FreeType metrics, shaping, strokes, and rich text |
| [images.md](images.md) | Compositing rules: alpha/blending, edge snapping, images, `CanvasGroup`, gradients, strokes, shadows, `Path2D`, scrollbars |
| [interaction.md](interaction.md) | Input rules: hover, clicks, keyboard, InputActions, wheel scroll spring, `TextBox`, and drag |
| [live-preview.md](live-preview.md) | Background render service, kept runs, live stdio session protocol, and service HTTP API |
| [studio.md](studio.md) | `rur compare`, hidden-desktop Studio launcher, and Studio MCP bridge |
| [viewports.md](viewports.md) | Rendering 3D `ViewportFrame`s via `ror` and the viewport worker JSON protocol |
| [testing.md](testing.md) | Automated tests, golden corpora, renderer baselines, and per-suite requirements |

## How the Studio rules were measured

Every layout, text, input, and rendering rule in `rur` is measured black-box rather than copied: a minimal fixture in `fixtures/` (or probe in `tools/studio/`) builds a scene from scratch, Roblox Studio renders it at 1.5x device scale, `rur compare` captures Studio's viewport losslessly and diffs against `rur`, and scripts in `tools/analysis/` fit the exact arithmetic. Fixture names cited throughout these guides are the scenes verifying each rule.

Code comments occasionally reference private measurement logs (`notes/*.md`, rounds 1–11) and internal fixtures. Those logs are not published; the guides here document the rules they established.
