# Projects, places and roots

A render runs one story in an emulated `DataModel`. Beside the story, rur mounts a project: your game's
or plugin's source, so the story's `require`s reach real modules.

## Roots

A root is a directory laid out by service: `<root>/ReplicatedStorage/Shared/Button.luau` becomes
`game.ReplicatedStorage.Shared.Button`, `<root>/StarterPlayer/StarterPlayerScripts/...` becomes
`game.StarterPlayer.StarterPlayerScripts...`. Folders become Folders (`init.luau` and `init.meta.json`
work as in Rojo), `.luau` files become ModuleScripts (`.server.luau` / `.client.luau` Scripts and
LocalScripts), `.json` files JSON modules and `.rbxm` / `.rbxmx` files the instances they hold.

`--roots "<dir>;<dir>"` names the roots of a run directly. Without it they come from the project's place.

## Projects and places

The project is `--project <dir>`, else the `RUR_PROJECT` environment variable, else
[`examples/project`](../examples/project) in this checkout. A project's places are named sets of roots,
declared in the project's `rur.json`:

```json
{
	"places": [
		{ "name": "client", "roots": ["shared", "client"] },
		{ "name": "server", "roots": ["shared", "server"] }
	]
}
```

- Roots are relative to the project directory. Keep them direct children of it: the engine reads its
  per-root settings from the same `rur.json`, keyed by the root folder's name (below).
- The first place is the default. Without `places` (or without a `rur.json`) a project has one place,
  `default`, whose only root is the project directory.
- A story runs in `--place <name>` when given, else in the first place whose roots contain the story,
  else in the default place. A story outside every root is mounted on its own, as a ModuleScript under
  `ReplicatedStorage.RURFixtures`; its requires must then reach the project through services
  (`game:GetService("ReplicatedStorage").Shared.Button`), not `script.Parent`.
- `rur stories [--place <name>|all]` lists a project's stories; a story under a root two places share is
  listed once, with the first of them.

The desktop storybook (`rur native`) lists the stories of every place of its `--project`, and the render
service watches the same roots for changes.

[cli/places.mjs](../cli/places.mjs) and [native/src/places.rs](../native/src/places.rs) implement these rules.

## Other `rur.json` keys

The engine reads these per root, as `{ "<key>": { "<root folder name>": value } }`:

| Key | Meaning |
|---|---|
| `placeIds` | `game.PlaceId` for stories mounted from that root (`--place-id` overrides it) |
| `streamingTargetRadius` | makes the place streaming-enabled with that radius |
| `use2022Materials` | `true` when the place uses the 2022 materials (MaterialService.Use2022Materials, which scripts cannot read); without it the place has the pre-2022 materials, as a place saved without the property does in Studio. ViewportFrames draw their parts with the place's materials |
| `clientGui` | the dot-separated path of a ModuleScript whose `Mount()` builds the client's own PlayerGui, run once when an edit-mode story first reaches the PlayerGui |

And this one for the whole project:

| Key | Meaning |
|---|---|
| `compareProbes` | `{ "<name>": { "luau": "<expression>", "fresh": <value> } }`: values `rur compare` reads in Studio before mounting, to report state Studio kept from an earlier compare |

## Rojo projects and explicit mounts

A Rojo project (`*.project.json`) can stand in for the project's places. Name it with `--rojo <file>` or
`--project <file.project.json>`, and its tree becomes the run's mounts:

- A `DataModel` tree mounts each `$path` at its instance path (directories as Rojo does), so a Wally
  `Packages` folder mapped to `ReplicatedStorage.Packages` is `require`-able as in the game.
- `$className` nodes without a `$path` become instances of that class; `{ "optional": ... }` paths are
  skipped when missing; a `$path` to another `*.project.json` nests that project.
- Any other tree (a plugin or model) mounts at `ReplicatedStorage/<project name>`; `--rojo-at Service/Name`
  moves it.

`--mount Service/Name=<dir|file>` (repeatable) adds a mount of its own, after the project's. With a Rojo project
or mounts and no `--roots`, no project roots are mounted. The render prints which project it mounted where:

```
rur: Rojo project C:\game\default.project.json: mounted ReplicatedStorage.Packages, ReplicatedStorage.Shared
```

**Found by itself.** With no project options at all (`--rojo`, `--project`, `--roots`, `--mount`, `--place`,
`RUR_PROJECT`), a `--story` or `--module` run mounts the project file nearest the story (its folder or a parent),
else the current directory's. A folder's project file is its `default.project.json`, else its only `*.project.json`.
A folder with several and no `default.project.json` mounts none of them, and rur names them so you can pass one with
`--rojo`. Run from a Rojo project, a story needs no flags:

```powershell
cd my-game        # default.project.json, Packages/ (Wally), src/
rur-native render --story src/shared/Stories/Button.story.luau --viewport 800x450 --dir out/Button
rur-native                     # the storybook on the same project (a release build; else --project default.project.json)
```

**Naming the project folder.** `rur-native <project folder>` opens the storybook as running `rur-native` inside that
folder does: it mounts the folder's project file (when its `rur.json` declares no places) and lists the same
stories, in any build. The path is relative to the current directory (`rur-native .` for the current folder,
`rur-native ../my-game` for another); a `*.project.json` file works too, as `--rojo` does. It replaces `--project`.

```powershell
rur-native C:\games\my-game    # the same as: cd C:\games\my-game; rur-native
```

`/` is not shorthand for the project folder: it names the root of the drive (`C:\` on Windows), and rur-native refuses
a drive root as a project rather than search the whole drive for stories. The storybook's story search skips dot
folders (`.git`) and `node_modules`, and stops with an error in the Stories panel after 100,000 folders or 30 seconds,
since a project that large is almost always the wrong folder.

Detection stays out of the way of a project rur already knows: a release build keeps a current directory whose
`rur.json` declares places, and a checkout build keeps its default project for the stories inside it. Any project
option turns it off.

The native commands (`render`, `sequence`, `engine`, `record` and the storybook) and the Node CLI read Rojo
projects the same way: [native/src/rojo.rs](../native/src/rojo.rs) and [cli/rojo.mjs](../cli/rojo.mjs), which
`rur-native rojo-mounts <file>` and the CLI test compare. [examples/rojo-wally](../examples/rojo-wally) is a
small Rojo + Wally project whose story requires a package through `ReplicatedStorage.Packages`.

```powershell
node cli/rur.mjs render --rojo ../my-plugin/default.project.json --story ../my-plugin/stories/App.story.luau --plugin
```

**A `$path` that doesn't exist** is skipped with a warning; when it is a Wally folder (`Packages`, `ServerPackages`,
`DevPackages`) beside a `wally.toml`, the warning says to run `wally install`.

**When a require fails.** A story that reads an instance nothing mounted (`Packages is not a valid member of
ReplicatedStorage "ReplicatedStorage"`), or whose mount still waits when the run ends (a `WaitForChild` for it),
gets a hint from what rur finds on disk ([native/src/hints.rs](../native/src/hints.rs)):

- a project file maps the instance to a path that doesn't exist (`wally install` when it is a Wally folder);
- a project file maps it, but the run used another project, `rur.json` places, or none of several project files:
  the `--rojo` that mounts it;
- inside a mounted folder: the folder on disk, which has no such file (`wally install` when wally.toml lists it);
- a child of a service no project file maps: what the nearest project file has under that service, or the folder of
  that name beside it and how to map it.

`rur-native render` prints the failure (or where the mount waits) and the hint on stderr; `scene.json` has the hint
as `diagnostics.hint`; the storybook shows both above the preview. When rur finds no cause there is no hint.

**Options are checked.** `rur-native render`, `sequence` and `engine` stop with `unknown option --x` (exit 2) on
an option they don't know, rather than skipping it with its value.

## Studio snapshots

Stories sometimes reach instances that exist only in a Studio place (GUI templates kept in
`ServerStorage`, say). Offline they are placeholders with default properties. `node
tools/sweep/studio-assets.mjs` exports such instances from the open Studio into `<Service>.rbxm` files,
and `--snapshot-dir <dir>` loads them into a run. Nothing loads a snapshot implicitly.

## Environment

| Variable | Meaning |
|---|---|
| `RUR_PROJECT` | the default project |
| `RUR_STATE_DIR` | caches, builds and logs (default `.cache` in this checkout) |
| `RUR_OUT_DIR` | the default output root (default `out`) |
| `RUR_ROBLOX_CONTENT` | the Studio `content` folder fonts and `rbxasset://` textures come from (default: the installed Studio) |
| `RUR_BUNDLE_MANIFEST` | the manifest module, relative to the project, whose text keys the model bundle catalog cache |
