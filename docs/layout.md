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
- **Deep descendants:** Visible descendants beyond direct children also expand plain containers (stopping at `ScrollingFrame` boundaries), with scale positions treated as `0`. Nested `AutomaticSize` children contribute their content on automatic axes; scale-sized descendants with children contribute their children's extents.
- **Padding:** `UIPadding` scale components are ignored during automatic size measurement (offsets count), then applied against the final size when placing children.
- **Anchor blending:** Available space for scale-sized content blends both sides around `AnchorPoint` `a`: `(1 - a) * (available - p) + a * p`.
- **Aspect ratio:** `UIAspectRatioConstraint` applies to the stable size, which is then measured against content on the opposite axis as well.
- **Automatic text:** Text objects resolve scale `Size` on automatic axes against the parent's full size (not padded content box). Wrapped text with `AutomaticSize.X` wraps at available parent width. `AutomaticSize.Y` keeps only lines whose bottom falls within `f32` rounding of available parent height below the object's position.
- **Center culling:** Any `AutomaticSize` element whose center lies outside its clip rect is not drawn at all, nor is its subtree (`fixtures/stories-autosize-cull*.luau`; reported under `culled` in `summary.json`).

## Lists, grids, pages, and tables

- **`UIListLayout`:** Each line is as thick as its thickest item. When flex is active (`Wraps`, non-`None` `HorizontalFlex`/`VerticalFlex`, or non-`Automatic` `ItemLineAlignment`), items grow proportional to `GrowRatio` and shrink weighted by `ShrinkRatio * size` (clamping at `0` like CSS), `Space*` alignments treat `Padding` as a minimum gap, `UIScale`'d items keep unscaled cross-axis size, and invisible items sit at `0x0` at the content origin. Lists never wrap along an automatic axis of their container.
- **Invisible items:** In non-flex `UIListLayout`, invisible children sit at the content origin keeping their size; in `UIGridLayout`, they sit at the origin with cell size.
- **`UIGridLayout`:** `UIScale`'d cells scale around the cell center; column count allows a `1e-5` relative overshoot tolerance.
- **`UIPageLayout` & `UITableLayout`:** Pages sit side by side spaced by `container_size + Padding` with the active page at the origin. Tables size rows and columns by their largest cells.

## Borders, padding, and constraints

- **Borders:** `BorderMode.Inset` insets content by `BorderSizePixel`, `Middle` by `BorderSizePixel / 2`, `Outline` by `0` (equivalent to offset `UIPadding`). If padding exceeds width, the content box sits between the inverted edges.
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
- **Line fitting:** Plain text fits a box if `W <= f32(box * DPI)`; rich text only wraps when a line exceeds that by at least `1 px`.
- Unwrapped lines wider than their box drop trailing whitespace from reported width if doing so makes them fit. Wrapped `TextBounds.X` spans all lines, including lines clipped vertically.
- Word wrap moves a word to a new line only if more of it fits on a fresh line than on the remainder of the current line; otherwise it splits mid-word.
- **`TextTruncate`:** Measures ellipsis (`…`, U+2026) using its rounded advance. `AtEnd` keeps the longest whole-word prefix (or character prefix of the first word); `SplitWord` keeps the longest character prefix plus trailing whitespace. `TextBounds` reflects the visible truncated text.
- **`TextScaled`:** Picks the largest integer size where no word splits and all lines fit (stepping UIScale'd logical sizes for plain text, physical sizes for rich text). Enabling `TextScaled` forces `TextWrapped = true`; disabling `TextWrapped` clears `TextScaled`.

See [text.md](text.md) for glyph metrics, font fallbacks, and text rasterization.
