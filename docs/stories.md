# Stories, modules, models and fixtures

What `rur render` (and the desktop storybook, the render service and `rur compare`) can run.

## Sources

| Flag | File | Mounted as |
|---|---|---|
| `--story <file.story.luau>` | a story (below) | the story mounted into the story target |
| `--module <file.luau>` | a module returning an Instance or `function(Target, Controls)` | its Instance (or what the function builds) parented to the target |
| `--fixture <file.luau>` | `return function(Target, Controls) ... end` | as a module; this repository's test scenes are fixtures |
| `--model <file.rbxm[x]>` | a saved instance tree | its top-level instances parented to the target |
| `--scene <scene.json>` | a saved or hand-edited scene | drawn without running any Luau |

The target is a Frame named `StoryTarget` in a ScreenGui named `RURStory` under `CoreGui`, sized to the
viewport (`--viewport WxH` logical px at `--scale`, default 1.5), or to the viewport minus `--margin` on
each side.

## Story formats

A story module returns one of:

- A UI Labs story table: `{ controls = {...}, story = function(props) ... end }`. `props` holds `target`
  and `controls`. With `vide = <the Vide module>` in the table, every control is a Vide source and the
  story is mounted with `vide.mount`, as the UI Labs plugin does; without it controls are plain values
  and `story` returns (or parents) the UI itself. A module or fixture that returns such a table is
  mounted as a story too.
- A table with `render = function(props) ... end`: called with `{ target, controls = {} }`.
- A function: called with the target.

Controls are the UI Labs kinds: plain values, `{ value, ... }` lists (the first is the default),
`EntryType = "Control"` tables and `EntryType = "ControlGroup"` groups. `rur render --controls
'{"Group.Name": 2}'` (or a JSON file) starts the story with other values: paths are dot-separated control
names, choices take a 1-based index or the option's label, colours `[r, g, b]`. `node cli/controls.mjs`
lists a story's controls. An unknown path fails the render.

See [examples/project](../examples/project) for one story of each common shape.

## Time

A run starts its virtual clock at 0 and runs scripts, tweens and springs frame by frame (1/60 s) until
`--time` (default 1), then lays out, exports and draws. The capture is the engine frame nearest
`--time`. Scripts that read the wall clock (`tick`, `os.time`, `DateTime`) read it from the run's start.
`rur sequence` captures many moments of one run.

## Timed input (`--actions`)

A JSON list of actions, each at `t` seconds of virtual time (default 0), checked before anything runs
(a bad list exits with code 64):

| type | fields |
| --- | --- |
| `move` | `x`, `y` |
| `down`, `up` | `x`, `y` (default: the pointer), `button` 1 / 2 / 3 (default 1) |
| `click` | a point, `button`, `hold` (seconds between down and up) |
| `drag` | `from` and `to` points, `button`, `steps` (default 10), `duration` (default 0.25 s) |
| `wheel` | `x`, `y`, `notches` (+1 = one notch away from the user) or `dy` (px, 100 per notch) |
| `leave` | the pointer leaves the viewport |
| `key` | `key` (an `Enum.KeyCode` name), `down` (default true), `shift`, `ctrl`, `alt` |
| `text`, `paste` | `text`, typed or pasted into the focused TextBox |

A point is a `node` path (as `node cli/inspect.mjs tree` prints it, or any suffix of it that starts at a
name) with optional `anchor` `{x, y}` (fractions of its size, default the centre) and `offset` `{x, y}`
(px), or `x`, `y` in logical viewport px. A node is located when the timeline reaches the action, from
its current position. A fixture with a sibling `<name>.actions.json` gets those actions in the golden
corpus.

## Writing a fixture

Fixtures are the measurement scenes: one rule per fixture, built from scratch with `Instance.new`, no
assets you can't share. Prefer textures that ship with Studio (`rbxasset://textures/...`) or local files
(`file://` URIs), and generic text. Keep a header comment that says what the fixture measures and which
`rur compare` command reproduces it, for example:

```lua
-- UIStroke on rounded frames at fractional sizes: where the stroke band starts and ends.
--   node cli/rur.mjs compare --fixture fixtures/stroke-radius.luau --viewport 900x400 --time 0.2 --background 20,20,26
return function(Target)
	...
end
```

New fixtures join the golden corpus (`node tools/engine/golden.mjs --fast`); a new one has no golden
until a maintainer records it after checking it against Studio.

## Output

A render writes `render.png` (physical pixels, straight alpha), `scene.json` (every node's rect,
visuals, text glyph positions and draw order), `summary.json`, `dependencies.json` and the glyph atlas
and images the scene draws with; [render.md](render.md#render) lists what each holds, and
`node cli/inspect.mjs` reads them ([render.md](render.md#inspect-text-first-for-agents)).
