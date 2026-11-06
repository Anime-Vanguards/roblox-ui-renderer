# Input

How the engine's input layer (`native/src/engine/input/`) delivers pointer, wheel, keyboard and text input,
measured by delivering real input to Studio (`tools/interaction/`) and recording the events and states.

## Frames

- Per frame the engine handles the frame's button events first, in order, then one movement update at the
  final pointer position. A press, a move away and back and a release in one frame still click; the
  intermediate position is never seen.
- Handlers of one input event run before the next event of the same frame (the deferred queue is flushed
  after every event).

## Hover

- Every GuiObject under the pointer takes part (MouseEnter / MouseMoved / MouseLeave and GuiState Hover),
  including objects that others cover: buttons, Active frames and InputSink frames above do not block
  hover.
- One movement update fires, topmost first: MouseMoved on every object under the pointer, InputChanged on
  objects already hovered, MouseLeave and InputEnded on objects left, MouseEnter and InputBegan on objects
  entered, then UserInputService.
- A GUI created or shown under a still pointer gets no MouseEnter until the pointer moves.
- `Interactable = false` propagates to descendants: no input events, GuiState NonInteractable.
- Hit tests: rects and clips are closed on all four edges; rotated objects hit-test their rotated box;
  ClipsDescendants, ScrollingFrame windows and CanvasGroups clip.

## Buttons

- The first GuiButton or TextBox under the pointer takes the press; objects below it get nothing. Objects
  above it (labels, frames) get InputBegan too and do not block.
- Press: MouseButton1Down, then InputBegan on every object down to the button, then UserInputService.
  Release: MouseButton1Click, MouseButton1Up, Activated(input, clickCount), then InputEnded on the objects
  that got InputBegan.
- Leaving the button during the press cancels MouseButton1Click but not Activated. An `Active = false`
  TextButton fires Down, Click and Up but not Activated.
- `clickCount` counts clicks less than 0.5 s apart, without a cap and whatever their distance.
- `gameProcessedEvent` for buttons is true when an Active object or any GuiButton / TextBox is under the
  point, checked at the press and at the release.
- AutoButtonColor tints hovered and pressed buttons by the measured amounts (`fixtures/interaction-autobuttoncolor.luau`).

## Keyboard and selection

- A key reaches every object under the pointer (InputBegan / InputEnded with the pointer's position); one
  InputObject per KeyCode.
- `GuiService.SelectedObject`: Return presses and releases the selected button like a click at its
  centre. Arrow and gamepad navigation are not emulated.
- `--device gamepad` and `--device touch` change what UserInputService reports; ContextActionService binds
  run for key input, highest priority and newest first.

## Wheel and scrolling

- One wheel notch moves the target CanvasPosition by 100 logical px, whatever the frame size; notches during
  the animation add to the target.
- CanvasPosition follows a critically damped spring with `omega = f32(9 * pi)`, every step in float32:
  `d = x - target`, `h = omega * dt`, `b = v + omega * d`, `e = exp(-h)`,
  `x = target + (d * (1 + h) + v * dt) * e`, `v = (v - omega * (b * dt)) * e`, snapping to the target
  when less than 1 px remains. `tools/interaction/reference/scroll-physics.json` holds the Studio recordings
  the tests check against (55 cases, 3,648 frames).
- The topmost ScrollingFrame under the pointer that can still move takes the wheel; a frame at the end of
  its range passes it on. A script's CanvasPosition assignment cancels the animation.
- Dragging a scroll bar thumb scrolls by `mouse delta * max / (track - thumb)`; clicks on the track do
  nothing.
- The desktop storybook predicts scrolling at display rate between engine frames with the same spring
  (`native/src/presentation.rs`).

## TextBox and drag

- Clicking a TextBox focuses it (Focused, TextBoxFocused, InputBegan); typed and pasted text, the caret,
  selection, copy and cut follow Studio's edit rules (`fixtures/interaction-textbox.luau`).
- UIDragDetector drags in play mode; in edit mode it is inert, as in Studio.

## Driving input

- One-shot renders: `--actions` ([stories.md](stories.md#timed-input---actions)).
- Live sessions: input events in `frame` commands ([live-preview.md](live-preview.md)).
- Studio: `tools/interaction/` delivers window messages to Studio's viewport (Windows only) and records
  what Studio's engine saw, for comparing the two event streams.
