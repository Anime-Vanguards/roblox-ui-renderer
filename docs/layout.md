# Layout rules

How rur's layout (`native/src/engine/layout/`) computes `AbsolutePosition`, `AbsoluteSize`,
`TextBounds` and scroll state. Each rule was measured in Studio; the probe scripts are in
`tools/studio/` and `tools/analysis/layout_probes.py` compares their numbers with rur's.

## Arithmetic

- Studio's GUI arithmetic is single precision (float32), in unscaled local units, then scaled. Sizes:
  `Wu = f32(f32(Size.Scale * parent content Wu) + Offset)`, `AbsoluteSize = f32(Wu * s)` with `s` the
  float32 product of the UIScale chain. Paddings, positions, list offsets and `TextBounds` follow the
  same pattern. It matters at exact fits (a text that fits a box at 26.0 in float32 but not at
  25.9999985 in double) and at .5 pixel snapping.
- `AbsolutePosition` is read through the centre: `c = f32(anchored + f32(W * 0.5))`, then
  `f32(c - f32(W * 0.5))`, so 18.6 can read back as 18.600006 next to a wide object.
- AnchorPoint components are clamped to [0, 1]. `UIScale.Scale` is clamped to [1e-7, 1000] when assigned.

## The top-bar inset

- With `--topbar-inset`, scripts read `AbsolutePosition` measured from below the inset, as in a playing
  Roblox client, so a ScreenGui's content reads the inset higher than it is drawn. With a 58 px inset, an
  `IgnoreGuiInset` ScreenGui reads `(0, -58)` and the full screen height. A default one reads `(0, 0)`, 58 px
  shorter. `InputObject.Position`, `Mouse.Y` and `GetGuiObjectsAtPosition` use the same space. Layout
  records, drawing, hit tests and exported rects stay in screen pixels.

## AutomaticSize

- Children size and position against a *stable* size: the base size (Size and constraints) grown on
  the automatic axes by the extents the children have without their scale terms. The final size is the
  larger of the stable size and where the children end up against it. Base 30x20 with a child at
  Position (1, 5), 20 wide: the children see 30, the final width is 55.
- Descendants count, not only children: every visible descendant below the children of a plain
  container extends it (not into ScrollingFrames), with scale positions counting as 0. A child that is
  itself AutomaticSize covers its content on its automatic axes. A scale-sized descendant that holds
  children counts by what it holds.
- UIPadding scale components are ignored for the automatic size (offsets count) but apply against the
  final size when placing the content.
- The space an object measures scale-sized content against blends both sides of its position by its
  AnchorPoint `a`: `(1 - a) * (available - p) + a * p`.
- A UIAspectRatioConstraint on an automatic object applies to the stable size, and the stable size is
  then measured from the content on the other axis too.
- Text objects resolve the scale Size of their automatic axes against the parent's whole size, not its
  content box.
- Wrapped text with AutomaticSize X wraps at the width available in the parent.
- An AutomaticSize object whose centre lies outside its clip is not drawn at all, nor its subtree
  (`fixtures/stories-autosize-cull*.luau`); `summary.json` lists these as `culled`.

## Lists, grids, pages, tables

- UIListLayout: a line is as thick as its thickest item. With flex (Wraps, HorizontalFlex /
  VerticalFlex other than None, or an ItemLineAlignment other than Automatic) items grow by GrowRatio
  shares of the free space and shrink weighted by ShrinkRatio x size, freezing at 0 as CSS does; Space*
  alignments treat Padding as a minimum gap. In flex lists a UIScale'd item keeps its unscaled
  cross-axis size and invisible items are 0x0 at the content origin.
- A list does not wrap along an automatic axis of its container.
- Invisible children of a UIListLayout or UIGridLayout sit at the content origin; list children keep
  their size, grid children get the cell size.
- UIGridLayout: a UIScale'd item is its cell scaled about the cell centre; cells per row allow a relative
  1e-5 overshoot.
- UIPageLayout pages sit side by side one container size plus Padding apart, the current page at the
  origin. UITableLayout sizes columns and rows by their largest cells.

## Borders and padding

- BorderMode Inset insets an object's content by BorderSizePixel, Middle by half of it, Outline not at
  all, like an offset UIPadding.
- A content box narrower than its padding lies between the two padded edges.

## Constraints

- UIAspectRatioConstraint FitWithinMaxSize: the largest box of the ratio inside the object's size.
  ScaleWithParentSize: the dominant axis keeps its size, the other follows the ratio, capped by the parent.
- UITextSizeConstraint bounds the UIScale'd text size: `floor(TextSize * UIScale)` clamped to
  `[floor(MinTextSize), floor(MaxTextSize)]`.

## ScrollingFrame

- CanvasSize scale is relative to the frame's parent content size; offsets scale with UIScale and are
  truncated to whole px.
- Assigning CanvasPosition clamps it at once to the frame's current layout.
- Bars: a vertical bar is needed when the canvas is taller than the frame, a horizontal one when it is
  wider; a needed bar makes the other axis need its bar when canvas >= frame - (thickness + border).
- `AbsoluteWindowSize` is the frame minus `ScrollBarThickness + BorderSizePixel` per visible bar.
- The canvas offset is rounded to whole physical pixels, half up.
- AutomaticCanvasSize: an automatic axis is `max(CanvasSize, content extent + UIPadding offsets)`; list
  Padding is measured against the frame's own Size applied to the measuring box.

## Rotation

- Under rotated ancestors, `AbsolutePosition` is the ancestor-rotated centre of the unrotated box minus
  half of `AbsoluteSize`; the object's own Rotation does not move it. `AbsoluteSize` is unrotated.
- Children placed by a UIListLayout or UIGridLayout ignore their own Rotation.
- Rotated descendants of a clipping, unrotated frame are clipped by it in the current (modern) rotation
  clipping mode, with a coarse cull; `--rotation-clipping disabled` selects the legacy behaviour.

## Text boxes

- Size under UIScale: `floor(floor(TextSize * UIScale) * DPI)` physical px, the product in float32.
  `TextBounds` is in absolute (scaled) px.
- A line of W whole px fits a plain text box when `W <= f32(box * DPI)`. Rich text wraps only lines at
  least 1 px wider than that.
- An unwrapped line wider than its box reports its width without its trailing whitespace when that
  makes it fit. `TextBounds.X` of wrapped text spans every line, including lines the box height cuts off.
- Word wrap takes a break opportunity only when more of the next word fits on a fresh line; otherwise it
  breaks inside the word.
- TextTruncate measures the ellipsis (U+2026) with its hinted advance; AtEnd keeps the longest prefix of
  whole words that fits beside it (or a character prefix of the first word); SplitWord keeps the longest
  character prefix plus its trailing whitespace. Truncated text's `TextBounds` measure what is shown.
- TextScaled picks the largest whole size at which no word breaks and all lines fit. Plain text steps
  through UIScale'd logical sizes, rich text through physical pixel sizes. Setting `TextScaled = true`
  also sets `TextWrapped`, and `TextWrapped = false` clears `TextScaled`.

See [text.md](text.md) for glyph metrics, fallback fonts and how lines are placed and drawn.
