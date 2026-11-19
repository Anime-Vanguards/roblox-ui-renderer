# Browser build (WebAssembly)

`rur` compiles the same engine and wgpu renderer to `wasm32-unknown-emscripten` (`--features web`, `native/src/web.rs`), rendering single-threaded onto a WebGL2 canvas via wgpu's GLES backend. Output frames are pixel-identical to native headless replays of the same input.

## Commands

```powershell
node tools/native-app/build.mjs --wasm                                    # Build to <state dir>/native-app/web/
node tools/web/bundle.mjs --story path/to/Button.story.luau --out ..\my-bundle
node tools/web/serve.mjs ..\my-bundle                                     # Serve at http://127.0.0.1:4600/
node tools/web/frame-identity.mjs --story path/to/Button.story.luau --replay clicks=path/to/clicks.json:300
```

## Requirements and bundling

- **Toolchain:** Emscripten SDK (`EMSDK` env var or a sibling `emsdk` checkout; tested with 6.0.10) and `rustup target add wasm32-unknown-emscripten`. Uses the `web` Cargo profile (fat LTO, `opt-level = 3`).
- **Self-contained bundles (`tools/web/bundle.mjs`):** Executes each story natively (plus optional `--explore` timelines) to discover required modules, fonts, hinted metrics, and images, then packs them alongside the project tree and `data/` into gzipped WebAssembly custom sections with `rur-host.js` (`native/web/host.js`) and `index.html`.
- **Frame-identity verification (`tools/web/frame-identity.mjs`):** Replays timed input natively and in headless Chrome/Edge and asserts every single frame hash matches.

> [!CAUTION]
> Web bundles embed fonts copied from your Roblox Studio installation. Keep bundles strictly local—never commit, publish, or redistribute them. `bundle.mjs` refuses to write output inside this repository.

