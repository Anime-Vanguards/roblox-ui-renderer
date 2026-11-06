# Documentation

Short references for using and extending rur. The [README](../README.md) is the front page: what rur
does, a quick start and the status.

| Document | What it covers |
|---|---|
| [setup.md](setup.md) | Requirements, the native build, `rur doctor`, the state dir and output root |
| [render.md](render.md) | The commands; `rur render` and its options and output; sequences, `--git-ref`, `--plugin`, whole PlayerGuis, sweeps, `rur record`, `inspect` |
| [storybook.md](storybook.md) | The desktop storybook (`rur native`), headless captures and benchmarks, memory and leaks |
| [web.md](web.md) | The browser build (WebAssembly): building, bundles, frame identity |
| [assets.md](assets.md) | Where textures, fonts and `rbxassetid://` images come from; the Roblox cookie |
| [how-it-works.md](how-it-works.md) | The parts and the source layout; the engine dependency and changing both at once |
| [projects.md](projects.md) | Projects, places and roots: what a run mounts beside the story, `rur.json` |
| [stories.md](stories.md) | Story, module, model and fixture formats; controls; timed input files |
| [layout.md](layout.md) | Layout rules measured in Studio: AutomaticSize, lists, flex, scrolling, text boxes |
| [text.md](text.md) | How text is measured and drawn: fonts, FreeType, placement, strokes, rich text |
| [images.md](images.md) | Compositing: alpha, blending, edge snapping, images, CanvasGroups, gradients, strokes, shadows |
| [interaction.md](interaction.md) | Input: hover, clicks, keys, the wheel spring, scrolling, TextBox, drag |
| [live-preview.md](live-preview.md) | How a render runs (the render service, kept runs), the live session protocol and the service's HTTP API |
| [studio.md](studio.md) | `rur compare`: comparing with Roblox Studio through a Studio MCP bridge |
| [viewports.md](viewports.md) | ViewportFrames: the 3D viewport worker protocol |
| [testing.md](testing.md) | The checks, and what each one needs (Studio, a GPU, the network) |

## How the rules were found

Every rule here was measured, not copied: a fixture in `fixtures/` (or a probe in `tools/studio/`) builds
a scene from scratch, Studio renders it at device scale 1.5, `rur compare` captures Studio's viewport
losslessly and diffs it with rur's render, and the fitting scripts in `tools/analysis/` look for the
arithmetic that reproduces Studio's numbers. The fixture names in these documents are the scenes that
show each rule.

Code comments sometimes cite the maintainers' measurement notes (`notes/*.md`, rounds 1 to 11). Those
notes are internal working logs and are not published; these documents summarize what they established.
A few fixtures the notes and comments mention are internal too.
