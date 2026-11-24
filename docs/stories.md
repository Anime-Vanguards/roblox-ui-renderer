# Stories, modules, models, and fixtures

This page explains the input formats `rur` can mount, how UI Labs and Flipbook stories/controls work, how to replay timed input with `--actions`, and how to author measurement fixtures.

## Input types

| Flag | Expected input | How it mounts |
|---|---|---|
| `--story <file.story.luau>` | UI Labs, Hoarcekat, or Flipbook story | Mounted into `StoryTarget` via the matching framework adapter |
| `--module <file.luau>` | Returns an `Instance` or `function(Target, Controls)` | Returned/created UI parented under `StoryTarget` |
| `--fixture <file.luau>` | `return function(Target, Controls) ... end` | Mounted like a module (used by this repo's test suite) |
| `--model <file.rbxm[x]>` or `<file.rbx>` | Saved Roblox model file, or a Roblox archive holding one | Top-level instances parented under `StoryTarget` |
| `--scene <scene.json>` | Saved or hand-edited scene JSON | Rasterized directly without executing Luau |

A Roblox archive (`.rbx`, the save format that holds a place or model with its content beside it) is read wherever a model file is (`--model`, `--load`, `--snapshot-dir`, `--mount`), as the model it holds with the values its writer kept outside the scene (script sources, long text) put back. An archive that cannot be read fails `--model` and `--load` with the reason, and is a warning naming the file elsewhere. Rojo project trees mount only what Rojo reads, so a `.rbx` inside one is left out. The format: roblox-object-renderer's `docs/archives.md`.

UI mounts into a `Frame` named `StoryTarget` inside `CoreGui.RURStory`, sized to `--viewport WxH` (logical pixels at `--scale`, default `1.5`) minus `--margin` on each side.

## Supported story formats

A story module can return:

1. **UI Labs story table:** `{ controls = {...}, story = function(props) ... end }`
   - `props` provides `target` and `controls`.
   - If `vide = <Vide module>` is present, each control becomes a Vide source and the story mounts via `vide.mount`. Otherwise, controls are plain values and `story` returns or parents the UI directly.
   - Modules and fixtures returning this table shape also mount as stories.
2. **UI Labs generic story:** `{ controls = {...}, render = function(props) ... end }`
   - `props` provides `target`, `controls` (plain values), `converted` (UI Labs-style control objects for `UILabs.CreateControlStates`: primitives wrapped in control descriptors, groups recursively converted, other tables kept as-is, unsupported values dropped with a warning), and `subscribe(listener)` returning an unsubscribe function.
   - Changing a control in a live session calls every subscriber with the new values table and `{ __old, __new }` diffs; errors or yields in one listener emit a warning without blocking remaining listeners. Returned cleanup functions are not called because `rur` does not unmount.
3. **Hoarcekat function:** `return function(target) ... end` (returned cleanup is not called).
4. **Flipbook story:** see below.

### Flipbook stories and storybooks

`rur` supports Flipbook 2.x stories following Storyteller 1.13 semantics (without its Iris renderer). A story table routes through a Flipbook renderer whenever it specifies `packages`, its `story` is a non-function (React/Roact element or `Instance`), or its owning storybook defines `packages`, `mapStory` or `mapDefinition`:

| `packages` | Mount behavior | On control change |
|---|---|---|
| `Roact` | `Roact.mount(Roact.createElement(story, props), target)` (or mounts element directly) | `Roact.update` |
| `React` + `ReactRoblox` | `ReactRoblox.createRoot(target):render(React.createElement(story, props))` (supports hooks) | Re-renders root |
| `Fusion` | Parents `story(props)` to `target`; controls are Fusion `Value`s (own scope in 0.3) | Updates the `Value` |
| `Vide` | `vide.mount` of `story(props)`; controls are Vide sources | Updates the source |
| `{}` (none) | Calls `story(target, props)` if 2 args, else `story(props)`; parents returned `Instance` | Runs previous cleanup and remounts |

Legacy lowercase keys (`react`, `reactRoblox`, `roact`, `fusion`) are also accepted (so UI Labs React stories mount the same way). A story's own `packages` overrides its storybook's. `props` includes `controls`, `container` (aliased as `target`), `story`, `theme = "Dark"`, `locale = "en-us"`, and the story's own `props` table. Multi-story `stories` tables mount their first entry.

**Storybooks (`*.storybook.luau` / `.lua`):** Return `{ name, storyRoots = { ... }, packages, mapStory, mapDefinition }`. `rur` discovers storybooks inside mounted project roots (skipping Wally `_Index` internals) and assigns each story to the first storybook whose `storyRoots` contain it. A storybook that fails to load is skipped with a warning.

A storybook's `mapDefinition` and `mapStory` run for every story it owns, whatever renders it:

- `mapDefinition(story)` runs once, when the story mounts. It receives the loaded story: its own fields plus `id`, `name`, `storybook`, `source` and `packages`. The story it returns is the one rendered, but the props and controls still come from the original story.
- `mapStory(story)` returns the component that renders in the story's place. React and Roact render it with the props and pass the story's element as its children.
- React, Roact and the Manual renderer call `mapStory` again on each render. Fusion and Vide call it once, since their controls change in place.
- Under a storybook with middleware, a Hoarcekat function mounts as a story without packages, so the Manual renderer calls it with `(container, props)`. Without middleware it is still called with the target alone.
- A UI Labs Vide story (`vide = ...`) keeps its UI Labs props, with `mapStory` applied inside `vide.mount`.

### Controls

Supported control formats include UI Labs controls (plain values, `{ default, ... }` dropdown arrays, `EntryType = "Control"`, `EntryType = "ControlGroup"`) and Flipbook typed controls (`Select`, `Radio`, `Number`, `Slider`, `Boolean`, `String`, `Color`, `MultiSelect`, `Check`).

- Inspect controls: `node cli/controls.mjs`
- Override on CLI: `--controls '{"Group.Name": 2}'` (inline JSON or file path). Paths are dot-separated; choices accept 1-based indices or option labels; colors use `[r, g, b]`. Unknown control paths fail the render.

See [examples/project](../examples/project) for minimal examples of each story type.

## Virtual time

Every render starts at `t = 0` and steps scripts, tweens, and springs at 60 Hz (`1/60 s` per frame) until `--time` (default `1`), capturing the nearest engine frame. Wall-clock APIs (`tick`, `os.time`, `DateTime`) advance from the start of the run. Use `rur sequence` to capture multiple frames from one run.

## Timed input (`--actions`)

Pass a JSON array (inline or file path) of input events scheduled at `t` seconds of virtual time (default `0`). Invalid specs fail immediately with exit code `64`.

| `type` | Fields |
|---|---|
| `move` | `x`, `y` (or a `node` point) |
| `down`, `up` | `x`, `y` (default: current pointer), `button` `1` / `2` / `3` (default `1`) |
| `click` | Point, `button`, `hold` (seconds between down and up) |
| `drag` | `from` and `to` points, `button`, `steps` (default `10`), `duration` (default `0.25` s) |
| `wheel` | Point (default: current pointer), `notches` (`+1` = away from user) or `dy` (`100` px per notch) |
| `leave` | Pointer exits the viewport |
| `key` | `key` (`Enum.KeyCode` name), `down` (default `true`), `shift`, `ctrl`, `alt` |
| `text`, `paste` | `text` string inserted into the focused `TextBox` |

**Targeting points:** Specify either logical viewport coordinates (`x`, `y`) or a `node` path (matching `inspect tree` or any name-aligned suffix) with optional `anchor` (`{x, y}`, default center) and `offset` (`{x, y}` in px). Node targets resolve dynamically from live layout at the moment the action fires. `click`, `drag`, and `wheel` move the pointer to the target point first.

A fixture with a sibling `<name>.actions.json` automatically replays those actions in the golden test suite.

## Writing a fixture

Fixtures are minimal, single-rule scenes used to measure and lock in Roblox Studio parity:
- Build everything from scratch with `Instance.new` using only shareable content (`rbxasset://textures/...`, local `file://` files, and generic text).
- Include a header comment stating what rule the fixture tests and the exact `rur compare` command to reproduce it:

```lua
-- UIStroke on rounded frames at fractional sizes: where the stroke band starts and ends.
--   node cli/rur.mjs compare --fixture fixtures/stroke-radius.luau --viewport 900x400 --time 0.2 --background 20,20,26
return function(Target)
	...
end
```

New fixtures are picked up automatically by `node tools/engine/golden.mjs --fast`, and need a maintainer to record their golden digest after verifying against Studio.

## Output

Every render writes `render.png` (physical pixels, straight alpha), `scene.json`, `summary.json`, `dependencies.json`, `manifest.json`, `glyphs.png`, and `assets/`. See [render.md](render.md#render) for full details and [render.md](render.md#inspect-text-first-for-agents) for `cli/inspect.mjs`.
