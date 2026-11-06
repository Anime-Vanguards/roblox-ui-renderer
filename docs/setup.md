# Setup: requirements, the build and the state dir

## Requirements

- **Rust 1.90+** and a C++ build toolchain (on Windows the Visual Studio C++ build tools), Git and tar. The
  first command that runs a story builds the native app (`node tools/native-app/build.mjs`: it downloads the
  pinned FreeType source into `<state dir>/native-app/source`, then runs `cargo build --release`, which
  fetches the engine at the commit `native/Cargo.toml` pins). Later edits to `native/` rebuild it on the next
  run, incrementally. On Windows run the build from PowerShell or cmd (Git Bash's GNU tar misreads `C:\...`
  archive paths).
- **Node 18+** with `npm install` run here (Node 22+ for the browser checks).
- **A Roblox Studio install.** Fonts and `rbxasset://` textures are read from it; rur ships none.
  `RUR_ROBLOX_CONTENT` can point at a copy of Studio's `content` folder instead (for example on Linux or
  macOS, where rur builds and renders headless but is less tested).
- Optional: a Roblox account cookie for `rbxassetid://` images that Roblox's public asset delivery refuses
  ([assets.md](assets.md)); ffmpeg for `rur sequence` videos (`RUR_FFMPEG`); Python 3 with Pillow for WebP
  thumbnails and the fitting scripts in `tools/analysis`; Lune 0.10.4 (`rokit`, or `RUR_LUNE`) for
  `tools/sweep/studio-assets.mjs`.

`node cli/rur.mjs doctor` checks all of the above and reports the state dir, the native build, the render
service, the Studio lock and leftovers in the state dir (`doctor --fix` removes those).


## State and output

Everything rur writes besides a command's output goes to the state dir: `RUR_STATE_DIR` (or `--state-dir
<dir>`), default `.cache`: asset and font caches, hinted metrics, the native app build, Studio snapshots,
git-ref extractions and `serve.log`. `RUR_OUT_DIR` replaces `out/` as the default output root. To share a
checkout's caches with a worktree, point the worktree's `RUR_STATE_DIR` at them; never junction or symlink
`.cache` folders into it (removing the worktree deletes through the links). `rur clean` shrinks the output
root without losing anything; `rur clean --older-than <days>` also removes old frame sequences.

Other environment variables: [projects.md](projects.md#environment).
