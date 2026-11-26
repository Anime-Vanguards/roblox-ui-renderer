# Input and interaction rules

How `rur`'s input layer (`native/src/engine/input/`) dispatches pointer, wheel, keyboard, and text events—calibrated by injecting real window messages into Roblox Studio (`tools/interaction/`), and input into a 0.747 Roblox client, and comparing event streams.

## Per-frame event ordering

- Within each frame, button events process first in order, followed by a single pointer movement update at the frame's final position. Pressing, moving away and back, and releasing within one frame still triggers a click because intermediate positions are coalesced.
- The deferred signal queue flushes after every individual input event, so handlers for one event finish before the next event in the same frame fires.

## Hover and hit testing

- **Pass-through hover:** Every `GuiObject` under the pointer receives hover events (`MouseEnter`, `MouseMoved`, `MouseLeave`) and `GuiState.Hover`. Overlapping buttons, `Active` frames, and `InputSink` frames do **not** block hover on objects beneath them.
- **Movement event order (topmost first):** `MouseMoved` on all objects under pointer → `InputChanged` on already-hovered objects → `MouseLeave` & `InputEnded` on exited objects → `MouseEnter` & `InputBegan` on entered objects → `UserInputService`.
- **Static pointer:** Creating or revealing a GUI directly beneath a stationary cursor does not fire `MouseEnter` until the pointer moves.
- **`Interactable = false`:** Propagates down to all descendants (suppresses input events and sets `GuiState.NonInteractable`).
- **Geometry:** Bounding rects and clip rects are closed on all four edges. Rotated elements hit-test against their rotated bounds. `ClipsDescendants`, `ScrollingFrame` viewports, and `CanvasGroup`s clip hit tests.

## Buttons and clicks

- **Sinking:** The topmost `GuiButton` or `TextBox` under the pointer claims button presses; objects below receive nothing. Non-button objects above it (labels, frames) still receive `InputBegan` without blocking it.
- **Event sequence:**
  - Press: `MouseButton1Down` → `InputBegan` (all objects down to the button) → `UserInputService`.
  - Release: `MouseButton1Click` → `MouseButton1Up` → `Activated(input, clickCount)` → `InputEnded` (on objects that received `InputBegan`).
- Moving off the button before releasing suppresses `MouseButton1Click` but still fires `Activated`. A `TextButton` with `Active = false` fires `Down`, `Click`, and `Up`, but not `Activated`.
- `clickCount` increments for consecutive clicks under `0.5 s` apart with no cap and no distance threshold.
- `gameProcessedEvent` is `true` for button events whenever an `Active` object, `GuiButton`, or `TextBox` sits under the cursor (re-checked on both press and release).
- `AutoButtonColor` applies Studio's measured hover/press tint shifts (`fixtures/interaction-autobuttoncolor.luau`).

## Keyboard, selection, and InputActions

- Key events (`InputBegan` / `InputEnded`) fire on every object under the pointer with the cursor's current coordinates (one `InputObject` per `KeyCode`).
- `GuiService.SelectedObject`: pressing `Return` clicks the selected button at its center. Arrow-key and gamepad UI navigation are not emulated.
- `--device gamepad` and `--device touch` update `UserInputService` device state. `ContextActionService` bindings fire in descending priority, newest first.
- **Input Action System:** Key and mouse button replays also drive bound `InputAction`s (`InputBinding.KeyCode`, including `MouseLeftButton`, `MouseRightButton`, `MouseMiddleButton`) after `UserInputService` events, updating `Pressed`, `Released`, and `GetState()`. Keys consumed by a focused `TextBox` do not trigger actions.

## Mouse wheel and smooth scrolling

- Each wheel notch shifts target `CanvasPosition` by **100 logical pixels** regardless of frame size; consecutive notches accumulate onto the target.
- **Scroll spring:** Critically damped spring in `f32` with `omega = f32(9 * pi)`:
  ```text
  d = x - target
  h = omega * dt
  b = v + omega * d
  e = exp(-h)
  x = target + (d * (1 + h) + v * dt) * e
  v = (v - omega * (b * dt)) * e
  ```
  Snaps to `target` when `|x - target| < 1 px`. Verified against 55 Studio recordings / 3,648 frames in `tools/interaction/reference/scroll-physics.json`.
- The topmost `ScrollingFrame` under the pointer with room to scroll claims the wheel; a frame already at its limit passes the event down. Writing `CanvasPosition` from Luau cancels any active spring animation.
- Dragging a scroll bar thumb scrolls by `mouse_delta * max_scroll / (track - thumb)`; clicking the empty track does nothing.
- The desktop storybook predicts scroll positions at display refresh rate between engine ticks using the same spring (`native/src/presentation.rs`).

## TextBox and dragging

- Clicking a `TextBox` focuses it (`Focused`, `TextBoxFocused`, `InputBegan`); clicks, double and triple clicks and drag selection match Studio (`fixtures/interaction-textbox.luau`).
- Typing and editing keys match a 0.747 client (measured key by key): typed text replaces the selection; Backspace / Delete; Left / Right; Home / End on the caret's displayed line (Ctrl: the text); Shift extends the selection; Ctrl+Left / Right / Backspace / Delete move or delete by words separated only by whitespace (Ctrl+Delete to the end of the next word); Ctrl+A selects everything; Up / Down move to the nearest place on the next line in a `MultiLine` box and otherwise keep the caret and drop the selection; Return submits a single-line box (`FocusLost(true)`) and adds a newline in a `MultiLine` one; Escape releases the box (`FocusLost(false)`).
- Not emulated: undo and redo (Ctrl+Z, Ctrl+Y). Copy, cut and paste use rur's own clipboard (the session's in the storybook); the client's were not measured.
- `UIDragDetector` is active in play mode and inert in edit mode, matching Studio.

## Driving input

- **Headless CLI renders:** `--actions` ([stories.md](stories.md#timed-input---actions)).
- **Live sessions:** `events` inside `frame` commands ([live-preview.md](live-preview.md)).
- **Studio comparison probes (Windows):** `tools/interaction/` injects window messages into Studio's viewport to record reference event streams.
