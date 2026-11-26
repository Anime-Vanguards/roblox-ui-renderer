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
- **Culled objects are not hit** (measured in a 0.747 client): a rotated element whose unrotated rect lies wholly outside its clip, and an `AutomaticSize` element whose centre is not strictly inside its clip (unless a `ScrollingFrame` above it is scrolled; a text element keeps itself), together with its own children (unless a list or grid layout places them or they are rotated; grandchildren are hit).

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
- `GuiService.SelectedObject`: pressing `Return` (or `ButtonA`) clicks the selected button at its center. Moving the selection: "Gamepad selection" below.
- `--device gamepad` and `--device touch` update `UserInputService` device state. `ContextActionService` bindings fire in descending priority, newest first.
- **Gamepad buttons** arrive as `Gamepad1` input with `Position` (0, 0, 1) going down and (0, 0, 0) coming up, as a connected pad's do in a client.
- **Input Action System:** Key and mouse button replays also drive bound `InputAction`s (`InputBinding.KeyCode`, including `MouseLeftButton`, `MouseRightButton`, `MouseMiddleButton`) after `UserInputService` events, updating `Pressed`, `Released`, and `GetState()`. Only input the GUI did not process reaches them (measured in a 0.747 client): keys a focused `TextBox` or a sinking `ContextActionService` bind takes, `Return` on the selected button, and clicks a `GuiButton`, `TextBox` or `Active` element takes do not. A release always reaches what its press holds. The wheel does not drive `MouseWheel` bindings yet.

## Gamepad selection

Measured in a 0.747 client with a gamepad connected (each rule below is the client's):
- **Keys:** the D-pad, the arrow keys and WASD move `GuiService.SelectedObject`, with or without a gamepad, and holding one does not repeat. With something selected the press is processed (also when nothing is there to move to); with nothing selected nothing moves. `GuiNavigationEnabled = false` stops the moves.
- **Where it goes:** a selectable object overlapping the selection's column (Up / Down) or row (Left / Right) wins over any other, nearest facing edge first, then the one nearer the centre line, then the left or top one. Otherwise an object within 45 degrees of the direction wins, by lateral offset; then the rest, by distance and angle. Over 300 random scenes the rule picks the client's button in 7,187 of 7,201 presses (`data/studio/navigation-random-scenes.json`).
- **Skipped:** objects that are hidden, `Selectable = false` (text labels and frames by default), `Interactable = false`, in a disabled `ScreenGui`, with their centre off the screen or outside a clipping frame, or with their centre under a button drawn above them. A `ScrollingFrame`'s buttons past its window are reachable, and moving to them does not scroll it.
- **Overrides:** `NextSelectionUp` / `Down` / `Left` / `Right` win when they name a selectable object. Inside a `SelectionGroup` a press looks inside the group first and leaves it only with `SelectionBehavior` `Escape`.
- **Signals:** `SelectionLost` and `SelectionGained` carry the `GuiNavigationDirection` (None for a script's selection); `SelectionChanged` fires on the old object and its ancestors, then on the new one and the ancestors not reached yet. A selection that becomes hidden, unselectable or moves off the screen is dropped.
- **Buttons:** `ButtonA` activates the selected button like `Return`; on another selected object it does nothing. `ButtonSelect` clears the selection, or with none selects the object with the lowest `SelectionOrder`, then the highest layer, then the topmost and leftmost (`AutoSelectGuiEnabled = false` disables it).
- **Highlight:** `rbxasset://textures/ui/SelectionBox.png` as a 9-slice (slice center 20, 20, 42, 42) over the selected object grown by 14 px, drawn last in its `ScreenGui`, unclipped and turned with the object; a `SelectionImageObject` on the object or on its `PlayerGui` replaces it (its background, border, corners and image).
- Not emulated: the thumbstick, the analog selection cursor, and selection in `SurfaceGui` / `BillboardGui`.

## Touch

With `--device touch`, pointer button 1 is a finger, following a 0.747 touch client:
- The finger hovers only while it is down; its `Touch` input object carries both the hover events (`MouseEnter`, `MouseMoved`, `MouseLeave`, `InputBegan` / `InputChanged` / `InputEnded`) and the press. Elements it comes down on show `GuiState.Press`.
- Down: `MouseButton1Down` on the button it lands on, then `MouseEnter` and `InputBegan`, then `UserInputService.InputBegan` and `TouchStarted`. Moves fire `InputChanged` and `TouchMoved`.
- Up: on the button it came down on `MouseButton1Up`, `Activated`, `MouseButton1Click`; on another button only `MouseButton1Up`. A `TextBox` takes focus at the release. Then `MouseLeave`, `InputEnded`, `UserInputService.InputEnded` and `TouchEnded`.
- Not emulated: a second finger, touch gestures, and scrolling a `ScrollingFrame` by dragging it.

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
- `gameProcessedEvent` of the wheel is `true` when an `Active` element or a `ScrollingFrame` with `ScrollingEnabled` is under the pointer, whether or not anything scrolled (measured in a 0.747 client).
- Dragging a scroll bar thumb scrolls by `mouse_delta * max_scroll / (track - thumb)`; clicking the empty track does nothing.
- The desktop storybook predicts scroll positions at display refresh rate between engine ticks using the same spring (`native/src/presentation.rs`).

## TextBox and dragging

- Clicking a `TextBox` focuses it (`Focused`, `TextBoxFocused`, `InputBegan`); clicks, double and triple clicks and drag selection match Studio (`fixtures/interaction-textbox.luau`).
- Typing and editing keys match a 0.747 client (measured key by key): typed text replaces the selection; Backspace / Delete; Left / Right; Home / End on the caret's displayed line (Ctrl: the text); Shift extends the selection; Ctrl+Left / Right / Backspace / Delete move or delete by words separated only by whitespace (Ctrl+Delete to the end of the next word); Ctrl+A selects everything; Up / Down move to the nearest place on the next line in a `MultiLine` box and otherwise keep the caret and drop the selection; Return submits a single-line box (`FocusLost(true)`) and adds a newline in a `MultiLine` one; Escape releases the box (`FocusLost(false)`).
- Not emulated: undo and redo (Ctrl+Z, Ctrl+Y). Copy, cut and paste use rur's own clipboard (the session's in the storybook); the client's were not measured.
- `UIDragDetector` is inert in edit mode, matching Studio. In play mode it follows a 0.747 client: the detector's parent takes a left press like a button (a button with a detector is not clicked), every move translates by the whole mouse delta (`TranslatePlane`, `TranslateLine`, the response styles, `BoundingUI`, `Min`/`MaxDragTranslation`), and `Rotate` turns the parent by the angle swept about its centre.

## Driving input

- **Headless CLI renders:** `--actions` ([stories.md](stories.md#timed-input---actions)).
- **Live sessions:** `events` inside `frame` commands ([live-preview.md](live-preview.md)).
- **Studio comparison probes (Windows):** `tools/interaction/` injects window messages into Studio's viewport to record reference event streams.
