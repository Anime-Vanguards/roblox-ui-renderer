# The browser build (WebAssembly)

The same crate runs in a web page: the engine and the wgpu renderer compiled for `wasm32-unknown-emscripten`
with feature `web` (`native/src/web.rs`), drawing on a WebGL2 canvas through wgpu's GLES backend,
single-threaded. Its frames are pixel-identical to the native headless replay of the same input.

```powershell
node tools/native-app/build.mjs --wasm                                    # <state dir>/native-app/web/
node tools/web/bundle.mjs --story path/to/Button.story.luau --out ..\my-bundle
node tools/web/serve.mjs ..\my-bundle                                     # http://127.0.0.1:4600/
node tools/web/frame-identity.mjs --story path/to/Button.story.luau --replay clicks=path/to/clicks.json:300
```

- Build: needs the Emscripten SDK (`EMSDK`, or an `emsdk` checkout beside this one; built with 6.0.10) and
  `rustup target add wasm32-unknown-emscripten`. The `web` profile is fat LTO at opt-level 3.
- Bundle (`tools/web/bundle.mjs`): runs each story natively for the modules it requires (plus optional
  `--explore` timelines) and packs them, the project tree, `data/`, the fonts, hinted metrics and every image
  the scenes or required sources name, in gzipped parts wrapped as WebAssembly custom sections, with the
  loader, `rur-host.js` (`native/web/host.js`) and a plain `index.html`.
- **A bundle holds Roblox Studio's installed fonts**, which belong to Roblox or are licensed to it: keep
  bundles on your own machine and never commit, publish or share them. The tool refuses to write a bundle
  inside this checkout.
- `tools/web/frame-identity.mjs` replays timed input natively and in headless Chrome or Edge through the
  module, and compares every frame's hash.

