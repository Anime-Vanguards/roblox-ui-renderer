# Layout rules

How `rur`'s layout engine (`native/src/engine/layout/`) computes `AbsolutePosition`, `AbsoluteSize`, `TextBounds`, and `ScrollingFrame` geometry. All rules are measured in Roblox Studio via probes in `tools/studio/` and validated by `tools/analysis/layout_probes.py`.

## Single-precision arithmetic

- Studio evaluates GUI layout in **32-bit float (`f32`)** in unscaled local space first, then multiplies by the `UIScale` product `s`:
  `Wu = f32(f32(Size.Scale * parent_content_Wu) + Offset)` → `AbsoluteSize = f32(Wu * s)`.
  Padding, positions, list offsets, and `TextBounds` follow the same `f32` order—critical for tight text fits (e.g., `26.0` in `f32` vs. `25.9999985` in `f64`) and `.5` pixel snap boundaries.
- `AbsolutePosition` reconstructs via the center: `c = f32(anchored + f32(W * 0.5))`, then `f32(c - f32(W * 0.5))`, reproducing Studio's rounding quirks (such as `18.6` reading back as `18.600006` next to a wide element).
- `AnchorPoint` clamps to `[0, 1]`. `UIScale.Scale` clamps to `[1e-7, 1000]` on assignment.

## Top-bar inset

With `--topbar-inset <px>` (e.g., `58`), scripts read `AbsolutePosition` in Roblox client space (shifted upward by the inset relative to physical screen coordinates):
- An `IgnoreGuiInset = true` `ScreenGui` reads `(0, -58)` with full screen height.
- A default `ScreenGui` reads `(0, 0)` with height reduced by `58 px`.
- `InputObject.Position`, `Mouse.Y`, and `GetGuiObjectsAtPosition` use the same inset coordinate space, while layout records, rendering, hit tests, and exported rects remain in screen pixels.

## AutomaticSize

- **Two-pass sizing:** Children size and position against the container's **stable size** (base `Size` + constraints expanded on automatic axes by children's non-scale extents). The container's final size is `max(stable_size, child_final_extents)`. Example: a `30x20` base container with a `20 px` child at `Position (1, 5)` resolves child scale against `30`, yielding final width `55`.
- **Measure basis:** While an automatic object is measured, it measures against its own `Size`: the scale part resolved against the space it can grow into (capped by its `MaxSize`) less its padding, plus the offset. A plain container's children size against that basis but position against the empty content box: a scale-1 child of a `Size (0, 20)` frame with `6 + 6 px` of padding is `20` wide and the frame `32`; a `0.5` child of a `Size (0, 50)` frame capped at `30` ends `12.5` wide. A list's scale `Padding` resolves against the same basis while measuring, then against the box less the padding, and the items extend the frame past its stable size (`fixtures/layout-autosize-constraint3.luau`, `layout-autolist-scalepad.luau`). Objects with a `UIAspectRatioConstraint` also follow the aspect rule below.
- **Deep descendants:** Visible descendants beyond direct children also expand plain containers (stopping at `ScrollingFrame` boundaries), with scale positions treated as `0`. Nested `AutomaticSize` children contribute their content on automatic axes; scale-sized descendants with children contribute their children's extents. Inside a plain container, its childless children size against the container's own `Size` resolved against its measured box (a `(0.5, 0.5)` frame measured `95 x 77.5` holding a `(1, 1)` frame counts `38.75` high), and only the offsets of its padding count (`fixtures/layout-autonested-scalepos.luau`).
- **Padding:** `UIPadding` scale components are ignored during automatic size measurement (offsets count), then applied against the stable box (constrained) when placing children, also when the children pushed the object past it.
- **Anchor blending:** Available space for scale-sized content blends both sides around `AnchorPoint` `a`: `(1 - a) * (available - p) + a * p`.
- **Aspect ratio:** An automatic object with a `UIAspectRatioConstraint` (`FitWithinMaxSize`, either `DominantAxis`) that holds children takes its width from what they hold and its height from the width. The content is measured inside the padding (scale leaves at the base `Size`, deeper childless children against their container's own `Size` on every axis); fitted to the ratio, its height plus the top and bottom padding, times the ratio, is the width, at least the base `Size` fitted to the ratio and, when the width is not automatic, at most the base width. The height is the width over the ratio (at most the base height when it is not automatic). Children lay out in that box and push the automatic axes further, so the object can end up off the ratio. A `240 x 60` frame of ratio `3` (fitted `180 x 60`) automatic on Y holding a `100 x 40` box is `180 x 60`, a `100 x 100` box `180 x 100`, a `260 x 100` box `240 x 100`; with `30 + 30 px` of top and bottom padding a `100 x 40` box makes it `240 x 80`. Left and right padding count nowhere. A `240 x 0` frame holding a `100 x 40` box is `100 x 40`. Automatic children grow into the box the ratio makes of the base width and the room the object has, less its padding (a `240 x 60` frame of ratio `3` with room offers `80`). On the axis that is not automatic, automatic content measures against the base `Size`. `ScaleWithParentSize` keeps its dominant axis (`fixtures/layout-autosize-aspect*.luau`, `layout-autosize-prompt.luau`).
- **Size constraints:** On an automatic axis, `UISizeConstraint` bounds the content inside the `UIPadding` offsets and border inset: with `MinSize` 22 and 6 + 6 px of padding, a label holding 8 px of text is 34 px wide. `MaxSize` then caps the object itself, and its padding's scale resolves against the capped size. Its children still lay out in the content box their content asked for, and they extend it past the cap (a 40 px child of a frame capped at 30 makes it 40). An automatic parent measures a capped child at its clamped content plus padding. The same clamp applies to a constrained descendant without `AutomaticSize`, from what it holds (`fixtures/layout-autosize-constraint*.luau`).
- **Automatic text:** Text objects resolve scale `Size` on automatic axes against the parent's full size (not padded content box). Wrapped text with `AutomaticSize.X` wraps at available parent width; with less room than its box, wrapped `AutomaticSize.XY` text takes its height from the text laid out in that room while its width, `TextBounds` and drawn text keep the box, and unwrapped `AutomaticSize.X` text drops its last space run only where that fits its room. A label draws its text laid out in its own content box (`|width|` when padding or a negative `Size` makes it negative), also where its automatic height came from a measure in another box. `AutomaticSize.Y` keeps only lines whose bottom falls within `f32` rounding of available parent height below the object's position (`fixtures/layout-autotext-matrix.luau`, `layout-autotext-linefit.luau`).
- **Center culling:** Any `AutomaticSize` element whose center lies outside its clip rect is not drawn at all, nor is its subtree (`fixtures/stories-autosize-cull*.luau`; reported under `culled` in `summary.json`). The screen is not a clip: with no clipping ancestor nothing is culled (`fixtures/layout-autosize-offscreen.luau`). An `AutomaticSize` label in an `AutomaticCanvasSize` `ScrollingFrame` whose box lies wholly outside its clip is not laid out for drawing and keeps the `TextBounds` of the canvas measure.

## Lists, grids, pages, and tables

- **`UIListLayout`:** Each line is as thick as its thickest item. When flex is active (`Wraps`, non-`None` `HorizontalFlex`/`VerticalFlex`, or non-`Automatic` `ItemLineAlignment`), items grow proportional to `GrowRatio` and shrink weighted by `ShrinkRatio * size` (clamping at `0` like CSS), `Space*` alignments treat `Padding` as a minimum gap, `UIScale`'d items keep unscaled cross-axis size, and invisible items sit at `0x0` at the content origin. Lists never wrap along an automatic axis of their container.
- **Invisible items:** In non-flex `UIListLayout`, invisible children sit at the content origin keeping their size; in `UIGridLayout`, they sit at the origin with cell size.
- **`UIGridLayout`:** `UIScale`'d cells scale around the cell center; column count allows a `1e-5` relative overshoot tolerance.
- **`UIPageLayout` & `UITableLayout`:** Pages sit side by side spaced by `container_size + Padding` with the active page at the origin. Tables size rows and columns by their largest cells.

## Borders, padding, and constraints

- **Borders:** `BorderMode.Inset` insets content by `BorderSizePixel`, `Middle` by `BorderSizePixel / 2`, `Outline` by `0` (equivalent to offset `UIPadding`). If padding exceeds width, the content box sits between the inverted edges.
- **`UISizeConstraint`:** Clamps the size to `[MinSize, MaxSize]` per axis. A constraint whose `MinSize` exceeds its `MaxSize` on either axis does nothing on both axes. The properties keep the values written, in either order.
- **`UIAspectRatioConstraint`:** `FitWithinMaxSize` chooses the largest fitting box of the target ratio; `ScaleWithParentSize` preserves the dominant axis and scales the other capped by the parent.
- **`UITextSizeConstraint`:** Clamps `floor(TextSize * UIScale)` to `[floor(MinTextSize), floor(MaxTextSize)]`.

## ScrollingFrame

- `CanvasSize` scale resolves against the frame's parent content size; offsets multiply by `UIScale` and truncate to integers.
- Writing `CanvasPosition` immediately clamps to current layout bounds.
- **Scroll bars:** A bar appears on an axis when `canvas > frame`. Once one bar is visible, the perpendicular axis also shows its bar whenever `canvas >= frame - (ScrollBarThickness + BorderSizePixel)`. `AbsoluteWindowSize` subtracts `ScrollBarThickness + BorderSizePixel` for each visible bar.
- Canvas scroll offset rounds half-up to whole physical pixels.
- **`AutomaticCanvasSize`:** Evaluates to `max(CanvasSize, content_extent + UIPadding_offsets)` (list `Padding` resolves against the frame's own `Size`).

## Rotation

- Under rotated ancestors, `AbsolutePosition` is the rotated center of the unrotated box minus half of `AbsoluteSize`; an object's own `Rotation` does not shift `AbsolutePosition`, and `AbsoluteSize` stays unrotated.
- Items laid out by `UIListLayout` or `UIGridLayout` ignore their own `Rotation`.
- Modern rotation clipping (`--rotation-clipping enabled`, default) clips rotated descendants against unrotated clipping ancestors with a coarse cull; `--rotation-clipping disabled` restores legacy behavior.

## Path2D

- Has no own rectangle; control point `Position` resolves like `GuiObject.Position` against the parent rect, and `LeftTangent`/`RightTangent` resolve like sizes. Rotates with a rotated parent.
- `UIScale` scales the parent rect, not path offsets or `Thickness`. `UIPadding` does not offset it. Inside a `ScrollingFrame`, it resolves against the window and does not scroll.
- `GetPositionOnCurve`, `GetTangentOnCurve`, their `ArcLength` variants, `GetLength`, and `GetBoundingRect` evaluate against the parent's laid-out rect in `f32` matching Roblox (including Roblox's arc-length approximation near sharp bends; see `ree`'s [docs/engine.md](https://github.com/Anime-Vanguards/roblox-engine-emulator/blob/main/docs/engine.md)).

## Text boxes

- **Physical size:** `floor(floor(TextSize * UIScale) * DPI)` in `f32`. `TextBounds` is reported in absolute (scaled) pixels.
- **`TextBounds` round up** to a whole logical pixel (`FFlagTextBoundsRoundUpToLogicalPixels`, on in production clients and Studio), and so do `TextService:GetTextSize` and `GetTextBoundsAsync`. An `AutomaticSize` label takes the rounded bounds; line breaks, `TextScaled`'s size and `TextFits` use the unrounded ones.
- A rich line of 1 px glyphs (`TextSize` 1 at 1.5x, or a `<font size>` that small) is 2 px tall; a plain one, or a size `TextScaled` picked, 1 px.
- An empty `TextBox` lays out its `PlaceholderText` as plain text, tags and all, even with `RichText` on, and `MaxVisibleGraphemes` cuts the placeholder as it cuts `Text`.
- **Line fitting:** Plain text fits a box if `W <= f32(box * DPI)`; rich text only wraps when a line exceeds that by at least `1 px`.
- Unwrapped lines wider than their box drop trailing whitespace from reported width if doing so makes them fit. Wrapped `TextBounds.X` spans all lines, including lines clipped vertically.
- Word wrap moves a word to a new line only if more of it fits on a fresh line than on the remainder of the current line; otherwise it splits mid-word.
- **`TextTruncate`:** Measures ellipsis (`…`, U+2026) using its rounded advance. `AtEnd` keeps the longest whole-word prefix (or character prefix of the first word); `SplitWord` keeps the longest character prefix plus trailing whitespace. `TextBounds` reflects the visible truncated text. An unwrapped rich line cut so short that not even its first grapheme and the ellipsis fit reports `TextBounds.Y = E * UIScale` (`E = floor(TextSize * UIScale)`, so `UIScale` is applied twice) instead of its line height.
- **`TextScaled`:** Picks the largest integer size where no word splits and all lines fit (stepping UIScale'd logical sizes for plain text, physical sizes for rich text). Enabling `TextScaled` forces `TextWrapped = true`; disabling `TextWrapped` clears `TextScaled`.

See [text.md](text.md) for glyph metrics, font fallbacks, and text rasterization.
