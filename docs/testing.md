# Tests and what they need

```powershell
npm test                                        # the selftest: the checks in cli/selftest.mjs (no Studio needed, but Studio's fonts)
cd native; cargo test --release; cd ..          # GUI-side engine, renderer, glyph and asset unit tests
node tools/engine/golden.mjs --fast             # engine output against recorded digests (part of npm test)
node tools/regress/renderer.mjs                 # renderer pixel hashes of saved scenes (local artifacts)
```

An intended change to engine output is re-recorded with `--write` only after `rur compare` shows it is
right in Studio. Refactors of the renderer must keep 0 mismatches in the regression baseline.

| Check | Needs |
|---|---|
| `cargo test --release` (unit tests in `native/src`, `native/tests/image_math.rs`, `paint.rs`) | nothing beyond the build, except `engine::input::tests::textbox_focus_caret_and_editing`, which needs Studio's fonts; glyph tests use `RUR_TEST_FONT` or a system font and skip without one |
| `native/tests/text_studio.rs`, `text_hinted.rs` | Studio's fonts (they fail without them) |
| `native/tests/text_corpus.rs` | `out/golden/text-corpus.json`, a local artifact; skips without it |
| `tools/tests/asset_*.mjs`, `native-assets.mjs`, `native-build-lock.mjs`, `places.mjs`, `web-bundle-directories.mjs`, `tools/analysis/image_assets_test.mjs` | Node only (HTTP is mocked) |
| `tools/engine/golden.mjs --fast` | the built engine and Studio's fonts; the network for cloud fonts a fixture uses |
| `tools/tests/cli.mjs`, `kept-runs.mjs`, `examples.mjs` | the built engine and Studio's fonts; `kept-runs` also a GPU (it renders through the render service) |
| `tools/tests/viewports.mjs` | a viewport worker ([viewports.md](viewports.md)) |
| `tools/regress/renderer.mjs` | saved scenes in `out/regress/` (local artifacts) and a GPU |
| `rur compare`, `tools/interaction/*`, `tools/studio/*` | Windows, a running Studio and a Studio MCP bridge ([studio.md](studio.md)) |

## Golden engine output

`tools/engine/golden.mjs` runs every fixture and probe and compares a digest of the engine's normalized
output (scene, script output, diagnostics, live session replies) with `tools/engine/golden.json`. `--fast`
mounts each fixture alone, in edit mode, plus three live sessions. The other corpora (`--corpus fixtures,live`)
mount the fixtures beside a frozen copy of a project (`RUR_PROJECT`, frozen with `--freeze`); their recorded
digests come from the maintainers' project, so outside it they are for your own recordings.

- Text output depends on the fonts it was measured with. The digests were recorded with the fonts of the
  Studio version in `golden.json`'s `fonts.studio`; another Studio version can change a few text entries.
  `--freeze-fonts` copies the font files the engine reads into `out/golden/fonts` so later runs read those.
- An entry already found identical is not run again while nothing it reads changed (`--rerun` runs all).
- `--write` records; do it only after `rur compare` shows the change is right in Studio, and list what moved.

## Renderer regressions

`tools/regress/renderer.mjs` draws saved scenes with `rur-native --batch` and compares hashes of the frames.
Refactors must keep 0 mismatches; `--shuffle <seed>` draws in another order and must agree.


## Continuous integration

GitHub's Windows runners have no GPU and no Studio. CI builds the binary, runs the Rust unit tests that need
neither, and the Node checks that only need Node; the font, GPU and Studio checks run on contributors'
machines.
