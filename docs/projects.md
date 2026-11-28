# Projects, places, and mounts

When `rur` renders a story, it mounts your game or plugin source tree into an emulated `DataModel` so `require()` calls resolve to real modules.

Implementation: [cli/places.mjs](../cli/places.mjs), [native/src/places.rs](../native/src/places.rs), [cli/rojo.mjs](../cli/rojo.mjs), and [native/src/rojo.rs](../native/src/rojo.rs).

## Source roots

A **root** is a directory organized by Roblox service name:
- `<root>/ReplicatedStorage/Shared/Button.luau` → `game.ReplicatedStorage.Shared.Button`
- `<root>/StarterPlayer/StarterPlayerScripts/...` → `game.StarterPlayer.StarterPlayerScripts...`

File mappings match `rojo build` 7.6.1: folders become `Folder`s (a folder's first of `init.luau`, `init.server.luau` and `init.client.luau` makes it that script, `init.csv` a `LocalizationTable`, and `init.meta.json` applies to it), `.luau` files become `ModuleScript`s (`.server.luau` / `.client.luau` become `Script` / `LocalScript`, `.plugin.luau` a `Script` only a plugin runs), `.json`, `.toml` and `.yaml` files become modules of their data, `.txt` files `StringValue`s, `.csv` files `LocalizationTable`s, and `.model.json`, `.rbxm` and `.rbxmx` files their one instance; a `<name>.meta.json` beside a file applies to its instance. As in Rojo, a suffix counts only in lower case.

Pass `--roots "<dir>;<dir>"` to specify roots directly instead of using a project's `rur.json`.

## Projects and `rur.json` places

`rur` picks the project from `--project <dir>`, else `RUR_PROJECT`, else `examples/project` (in a source checkout) or the current directory (in a release binary).

Declare named sets of roots (**places**) in `<project>/rur.json`:

```json
{
	"places": [
		{ "name": "client", "roots": ["shared", "client"] },
		{ "name": "server", "roots": ["shared", "server"] }
	]
}
```

- Root paths are relative to the project folder and should be direct children of it (per-root settings below key off the root folder's name).
- Without `places` or `rur.json`, the project has a single place named `default` rooted at the project folder itself.
- Story resolution: uses `--place <name>` if passed, else the first place whose roots contain the story file, else the first place. A story outside all mounted roots is mounted standalone under `ReplicatedStorage.RURFixtures` (so its `require` calls must go through services rather than `script.Parent`).
- `rur stories [--place <name>|all]` lists discovered stories (a story in a shared root appears once under the first matching place). The desktop storybook lists stories across all places and watches those roots for edits.

### Additional `rur.json` settings

Per-root settings take the form `{ "<key>": { "<root folder name>": value } }`:

| Key | Description |
|---|---|
| `placeIds` | Sets `game.PlaceId` for stories in that root (`--place-id` overrides). |
| `streamingTargetRadius` | Enables workspace streaming with the given radius. |
| `use2022Materials` | Set to `true` if the place uses 2022 materials (`MaterialService.Use2022Materials` is unreadable from Luau; defaults to pre-2022, matching Studio for places without the property). Used by `ViewportFrame`s. |
| `clientGui` | Dotted path to a `ModuleScript` whose `Mount()` builds the client's `PlayerGui`, invoked once when an edit-mode story first accesses `PlayerGui`. |

Project-wide setting:

| Key | Description |
|---|---|
| `compareProbes` | `{ "<name>": { "luau": "<expr>", "fresh": <value> } }` — expressions `rur compare` checks in Studio before mounting to detect stale state left by a previous compare. |

## Rojo projects and explicit mounts

Pass a Rojo project file via `--rojo <file.project.json>` or `--project <file.project.json>` instead of `rur.json` places:
- A `DataModel` tree mounts each `$path` at its target service path (so Wally's `Packages/` mapped to `ReplicatedStorage.Packages` works out of the box).
- `$className` nodes without `$path` create empty instances of that class; `{ "optional": ... }` paths are skipped if absent.
- A `$path` to another `*.project.json` makes the node that project's tree, as Rojo builds it: its class and children, then the node's own children (a node's own `$className` replaces only a `Folder` root). The tree's root works the same way, so a root that is a `$path` to a `DataModel` project, with more services beside it, is a `DataModel` tree. Chains of project files are followed; a cycle is refused with its files named.
- Non-`DataModel` trees (plugins or models) mount under `ReplicatedStorage/<project name>` (override with `--rojo-at Service/Name`).

Add extra mounts with repeatable `--mount Service/Name=<dir|file>` (applied after project mounts). When `--rojo` or `--mount` is passed without `--roots`, default project roots are not mounted. Each run logs what it mounted:

```text
rur: Rojo project C:\game\default.project.json: mounted ReplicatedStorage.Packages, ReplicatedStorage.Shared
```

See [examples/rojo-wally](../examples/rojo-wally) for a working Rojo + Wally setup.

### Automatic Rojo detection

When you run a `--story` or `--module` command with no project flags (`--rojo`, `--project`, `--roots`, `--mount`, `--place`, `RUR_PROJECT`), `rur` walks up from the story folder (then checks the current directory) looking for `default.project.json`, or a lone `*.project.json`. If a folder contains multiple `*.project.json` files without `default.project.json`, `rur` mounts none and lists them so you can choose with `--rojo`.

In a Wally package, `default.project.json` is usually the package's own tree, not a DataModel. The package's code finds its dependencies as siblings (`script:FindFirstAncestor("MyPackage").Parent` is the `Packages` folder that holds Wally's link modules), and only the DataModel dev project beside it, the one you serve into Studio to run the stories, has them. So when the `default.project.json` that `rur` finds is not a DataModel tree, `rur` checks the other `*.project.json` files beside it:

- If exactly one of them is a DataModel tree that mounts the story's file, `rur` mounts that one. The storybook without `--story` asks for a project that mounts every folder the package maps.
- If several qualify, `rur` keeps `default.project.json`.

Either way, the `rur: Rojo project ...` line says which file was used and why, and names the candidates; `--rojo` picks another.

```powershell
cd my-game
rur-native render --story src/shared/Stories/Button.story.luau --viewport 800x450 --dir out/Button
rur-native                                    # Open storybook on the current Rojo project
rur-native C:\games\my-game                   # Same as cd + rur-native (also accepts a *.project.json)
```

Detection never overrides a configured project (a release build respects a `rur.json` with `places`, and a checkout build keeps its default project for stories inside it).

**Safety guards:**
- Passing `/` (or a drive root like `C:\`) as a project is rejected so `rur-native` never scans an entire drive.
- Story discovery skips hidden dot-folders (`.git`) and `node_modules`, stopping with an error after 100,000 directories or 30 seconds.
- `rur-native render`, `sequence`, and `engine` reject unknown flags with `unknown option --x` (exit code `2`).

### Missing paths and failure hints

- A missing `$path` logs a warning; if it is a Wally folder (`Packages`, `ServerPackages`, `DevPackages`) beside a `wally.toml`, the warning reminds you to run `wally install`.
- When a story fails with a missing member error (e.g., `Packages is not a valid member of ReplicatedStorage`) or hangs in `WaitForChild`, `rur` inspects the project on disk (`native/src/hints.rs`) and prints an actionable hint on stderr, in `scene.json` (`diagnostics.hint`), and above the storybook preview—pointing out missing `$path`s, unmounted project files (`--rojo`), missing files in mounted folders, unmapped sibling folders, or the DataModel project beside a package's own tree that the run should have mounted.
- When a required module fails, Roblox logs the module's own error and raises `Requested module experienced an error while loading` to the caller. `rur render`, headless captures and the storybook show the logged error instead of that wrapper, and work out the hint from it.

## Studio snapshots

If a story clones GUI templates stored only in a Studio place file (e.g., `ServerStorage`), offline runs otherwise see empty placeholder instances. Export those instances from an open Studio into `<Service>.rbxm` files with `node tools/sweep/studio-assets.mjs`, then pass `--snapshot-dir <dir>` to mount them. Snapshots are never loaded implicitly in public builds.

## Environment

| Variable | Description |
|---|---|
| `RUR_PROJECT` | Default project directory |
| `RUR_STATE_DIR` | Cache, native build, snapshot, and log directory (default `.cache` in checkouts) |
| `RUR_OUT_DIR` | Default render output root (default `out`) |
| `RUR_ROBLOX_CONTENT` | Path to a Studio `content` directory for fonts and `rbxasset://` textures (default: installed Studio) |
| `RUR_BUNDLE_MANIFEST` | Project-relative manifest module whose contents key the model-bundle catalog cache |
