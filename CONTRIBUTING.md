# Contributing to roblox-ui-renderer

Thanks for contributing. This guide explains how pull requests are merged, how to build and test locally, and what we can accept.

## How pull requests are merged

This repository is synced automatically from the maintainers' internal repository, so `main` advances only via sync commits.

Open pull requests here as usual. Once approved, a maintainer imports the change internally; the next sync publishes it to public `main` with you preserved as the author and closes your PR with a `"Landed in ..."` link.

## Before you open a PR

- **Small fixes** (typos, clear bugs with a test): open a PR directly.
- **Larger changes** (new features, CLI flags, behavior changes, refactors): open an issue first to agree on the design before writing code.
- **Visual differences from Studio:** use the "Looks different from Studio" issue template with side-by-side Studio output ([Measuring against Studio](#measuring-against-studio)).
- **Security issues:** never file publicly—follow [SECURITY.md](SECURITY.md).

Search existing issues before opening a duplicate.

## Development setup

Requirements:
- **Windows 10 or 11** (primary platform; Linux and macOS can build and render headless but are less tested).
- **Rust 1.90+** with Visual Studio C++ Build Tools on Windows.
- **Node 18+** (Node 22+ for browser checks) and **Git**.
- **Roblox Studio installed locally** (`rur` reads fonts and `rbxasset://` textures from Studio and ships none).

```powershell
git clone https://github.com/Anime-Vanguards/roblox-ui-renderer
cd roblox-ui-renderer
npm install
node tools/native-app/build.mjs          # Downloads FreeType and compiles rur-native
npm test                                 # Selftest: engine goldens, CLI, assets, places, examples
cd native; cargo test --release; cd ..   # Rust engine, renderer, and glyph tests
```

> On Windows, run build commands from PowerShell or Command Prompt. Git Bash's GNU `tar` misparses `C:\...` archive paths.

The core engine lives in [roblox-engine-emulator](https://github.com/Anime-Vanguards/roblox-engine-emulator), pinned by commit in `native/Cargo.toml`. When editing both repositories together, run `node tools/emulator.mjs link` to build against a sibling checkout.

`npm test` checks engine output against digests in `tools/engine/golden.json`. Because text metrics depend on the exact Studio version's fonts (`fonts.studio` in `golden.json`), a few text entries may differ on another Studio build—mention that in your PR instead of re-recording goldens. See [docs/testing.md](docs/testing.md) for per-suite requirements.

Note: Code comments occasionally reference private lab notes (`notes/*.md`). Those notes are not published; `docs/` summarizes every rule they established.

## Pull request guidelines

- **One change per PR.** Keep diffs focused and split unrelated work.
- **Include tests for behavior changes.** Run the full test suite above and report what you ran in the PR.
- **Match existing style.** Tabs in JavaScript, Luau, PowerShell, and GLSL; 4 spaces in Rust and C (rustfmt style). Do not reformat untouched files.
- **Keep docs updated.** Update `README.md` and `docs/` whenever flags, outputs, or behavior change.
- **Plain commit and PR titles.** One clear sentence describing what changed.
- **No generated artifacts.** Never commit builds, renders, logs, or output folders.

### Measuring against Studio

Parity PRs must include:
- The Roblox Studio version and graphics settings compared against.
- A minimal, self-contained scene or script built from scratch (`Instance.new`, small `.rbxm`, etc.).
- Before-and-after measurements or images side by side with Studio.

Never copy code, shaders, lookup tables, or assets out of Roblox binaries or install directories. Fit constants from black-box measurements instead.

## What we cannot accept

- Assets, models, places, images, meshes, fonts, or text you do not own or cannot legally share (including Roblox game assets or Roblox's shipped fonts/textures). Fixtures must build their own scenes.
- Code copied, decompiled, or reverse-engineered from Roblox binaries, or code incompatible with GPL-2.0-or-later.
- Telemetry or network calls to hosts other than the Roblox endpoints already used.
- Credentials or secrets anywhere in code, tests, logs, or screenshots (never paste `.ROBLOSECURITY`).

## AI-assisted contributions

AI tools are welcome—the maintainers use them too—with five rules:
- **You own every line.** Read, understand, and verify all code before submitting.
- **Disclose AI use** in the PR template.
- **No unattended bulk PRs.** Drive-by batch PRs without a backing issue will be closed without review.
- **Point agents at [AGENTS.md](AGENTS.md).**
- **Tests are required.** Generated code without a test that fails before the fix will not be merged.

## Review process

Issues and PRs receive an initial automated triage/review (usually within an hour), escalating to a maintainer when needed. For bugs or feature requests filed as issues, maintainers handle the implementation directly—you don't need to submit a PR.

PRs are screened for security first, then reviewed for quality and fit. Push requested fixes to the same PR branch for re-review. Changes to build scripts, CI, dependencies, or agent instructions always require maintainer sign-off.

## License and conduct

`roblox-ui-renderer` is licensed under GPL-2.0-or-later ([LICENSE](LICENSE)). By contributing, you confirm you have the right to submit your work under those terms. All participants follow the [Code of Conduct](CODE_OF_CONDUCT.md).
