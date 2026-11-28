# Testing and golden suites

How to run `rur`'s unit tests, golden engine checks, text corpus verification, and renderer pixel baselines—plus what each suite requires.

```powershell
npm test                                        # Offline selftest (cli/selftest.mjs; needs Studio fonts)
cd native; cargo test --release; cd ..          # Rust unit & integration tests
node tools/engine/golden.mjs --fast             # Engine golden digests (also run by npm test)
node tools/regress/renderer.mjs                 # Renderer frame-hash regression suite (local scenes)
```

> Only update goldens (`--write`) or baselines after `rur compare` proves the change matches Roblox Studio. Pure refactors must produce 0 differences.

## Test requirements matrix

| Suite | Requirements |
|---|---|
| `cargo test --release` (`native/src`, `image_math.rs`, `paint.rs`) | Build only, except: `textbox_focus_caret_and_editing` needs Studio fonts, `automatic_text_measures_in_the_room_it_has` skips without them, `text_bounds_round_up_to_whole_logical_pixels` checks the client's widths only with them (without them, the test font's), and `automatic_size_constraints_bound_the_content_inside_the_padding` then compares only its cells without text; 4 GPU tests (`native_gpu_quad_alpha_texture_groups_and_clipped_glyph`, `partial_redraws_match_whole_frames`, `native_snapped_image_pixels_match_canonical_canvas`, `a_turned_shapes_square_corner_has_hard_edges`) need a GPU; glyph tests use `RUR_TEST_FONT` or a system font (skip if missing; `a_story_on_a_loopback_client_shows_what_the_server_sent`, `load_and_model_read_an_archive_with_its_values` and `truncated_rich_text_tiny_rich_lines_and_placeholders_measure_as_the_client` fail without one). |
| `native/tests/text_studio.rs`, `text_hinted.rs` | Local Roblox Studio fonts (fails without them). |
| `native/tests/text_corpus.rs` | Local artifacts `out/golden/text-corpus.json` and `text-corpus.whole.json` (skips whichever is absent). |
| `tools/tests/asset_studio*.mjs`, `native-build-lock.mjs`, `places.mjs`, `web-bundle-directories.mjs` | Node only (Studio bridge is mocked). |
| `tools/tests/native-assets.mjs` | Compiled `rur-native` (runs offline). |
| `tools/engine/golden.mjs --fast` | Compiled `rur-native`, Studio fonts, and network access for any cloud fonts used by fixtures. |
| `tools/tests/cli.mjs`, `kept-runs.mjs`, `examples.mjs` | Compiled `rur-native` and Studio fonts (`kept-runs` also needs a GPU for the render service). |
| `tools/tests/viewports.mjs` | `ror` installed ([viewports.md](viewports.md)). |
| `tools/regress/renderer.mjs` | Local saved scenes in `out/regress/` and a GPU. |
| `rur compare`, `tools/interaction/*`, `tools/studio/*` | Windows, running Roblox Studio, and a Studio MCP bridge ([studio.md](studio.md)). |

## Engine golden suite (`tools/engine/golden.mjs`)

Runs fixtures and probes and verifies normalized digests (scene JSON, script output, diagnostics, live-session replies) against `tools/engine/golden.json`:
- `--fast`: runs standalone edit-mode fixtures plus three live sessions.
- `--corpus fixtures,live`: runs against a frozen project snapshot (`RUR_PROJECT`, created with `--freeze`). Public users can record their own baselines here.
- **Font version sensitivity:** Text digests match the Studio version recorded in `golden.json` (`fonts.studio`); another Studio build may shift a few text entries. Use `--freeze-fonts` to snapshot active font files into `out/golden/fonts`.
- Unchanged entries are cached across runs (pass `--rerun` to force all). Use `--write` only after Studio verification, and list moved entries in your PR.

## Text corpus (`native/tests/text_corpus.rs`)

Replays **85,399** frozen text cases (layouts, rich-text parses, line pens, sizing helpers) against exact digests for both [text modes](text.md#text-modes):
- Quarter-pixel: `out/golden/text-corpus.json` (`native/tests/text-corpus.sha256`, override path with `RUR_TEXT_CORPUS`).
- Whole-pixel: `out/golden/text-corpus.whole.json` (`native/tests/text-corpus.whole.sha256`, override with `RUR_TEXT_CORPUS_WHOLE`).

To update a corpus after a Studio-verified rule change (`text_studio.rs` / `data/studio/`), target one test explicitly:

```powershell
$env:RUR_TEXT_CORPUS_UPDATE = "out/golden/text-corpus.whole.new.json"
cargo test --release --test text_corpus -- text_engine_matches_frozen_whole_pixel_corpus
```

(Or `-- text_engine_matches_frozen_corpus` for quarter-pixel.) Keep the old corpus beside the new one and report how many cases moved and why. Unexplained diffs are regressions.

## Renderer regression suite (`tools/regress/renderer.mjs`)

Renders saved scenes via `rur-native --batch` and compares output frame hashes. Refactors must maintain 0 mismatches, and `--shuffle <seed>` (randomized scene order) must match identically.


## Continuous integration

GitHub Actions Windows runners have neither a GPU nor Roblox Studio. CI compiles `rur-native`, runs the headless Rust unit tests, and executes the Node-only test scripts; font, GPU, and Studio suites run locally on contributor machines. It then runs the resource budgets (`tools/resources/measure.mjs --suite ci`: the golden check's fast corpus cold and warm and a batch-worker stress, checked for disk writes, disk space, memory and page faults against `tools/resources/baseline.json`; AGENTS.md "Resource budgets").
