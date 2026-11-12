# Compositing: fills, images, CanvasGroups, gradients, strokes, shadows, paths

How the renderer (`native/src/render/`, the uber shader `shaders/uber.*.glsl`) turns a scene into pixels
the way Studio does. All numbers are for Studio at device scale 1.5 on a D3D11 GPU; `rur compare` reports
how close a capture came (`exactPercent`: largest channel difference at most 2 of 255).

## Alpha and blending

- Every transparency becomes an 8-bit alpha like colours do: `alpha8(T) = min(255, floor((1 - T) * 256))`
  (0.1 -> 230, 0.5 -> 128, 0.9 -> 25).
- Shader outputs are converted to 8 bits with `gpu8(x) = (floor(x * 4096) * 255 + 2047) >> 12` before the
  blend.
- The blend is float with one rounding: `round((c * A + D * (255 - A)) / 255)` for colour `c`, alpha `A`
  and destination `D`. Gradient fills blend the same way.

Fixtures: `group-blend`, `rects`.

## Edge snapping

- Device edges: Studio scales the float32 AbsolutePosition and AbsoluteSize separately by the device scale
  in float32 and adds them in float32, then snaps each edge to whole pixels, rounding half up with a small
  tolerance below the tie that scales with the device scale (`render/geometry.rs snap_device`).
- Every rect, clip, border and image quad goes through it.

Fixtures: `image-probe-ties`, `snap-probe-ties1x`.

## Images

- Images are drawn as quads with snapped edges; the texture is sampled bilinearly with 8-bit weights at the
  texel centres the snapped quad maps to; minified textures use mip levels, and a level with an odd side
  rounds its even axis down.
- 9-slice (`ScaleType.Slice`), Tile, Fit and Crop follow Studio's quad layout; `ImageRectOffset` /
  `ImageRectSize` crop the texture.
- A rotated image's corners sit on the rasterizer's 1/256 px grid and its texture coordinates run over the
  quad's two triangles. Its mip level follows the texels a pixel spans across, down and along both diagonals;
  the anti-aliased fringe samples on the edge line at the level of its own strip.
- A node whose rotations cancel is drawn on snapped edges, fill and image alike.
- An `EditableImage` has no mip levels: a minified one aliases, as in Studio.
- Thumbnails (`rbxthumb://`) use the thumbnail service's WebP rendition, as Studio does.

Fixtures: `image-probe-*` (the ones with Studio's own `rbxasset://` textures).

## CanvasGroup

- The group's texture is allocated in whole logical pixels: `N = ceil(ceil(Q / DPI) * DPI)` device px for
  a snapped quad of `Q`; when `N != Q` the content is resampled. The texture size is a float32 difference.
- Content is laid out and snapped on the screen grid, then scaled into the texture; images inside are
  sampled in texture space.
- The texture holds premultiplied colour; the composite unpremultiplies, multiplies by GroupColor3 and the
  group's UIGradient, and applies GroupTransparency and the group's UICorner.
- Very large groups are capped (an area and a side limit, `render/geometry.rs group_texture_dims`).

Fixtures: `group-resample`, `group-misc`, `group-blend`, `canvasgroup*`, `group-probe-*`.

## UIGradient

- Shaded per pixel (Studio 0.747, notes/gradients.md): the pixel's `t`, the colour between the two
  keypoints around it interpolated in squared space (the squared keypoint colours held in 8 bits), the
  opacity linearly, then an ordered 4x4 dither before the colour is cut to 8 bits.
- `t`: Linear uses the rotation as a screen-space angle over the unsnapped rect, the centre moved by
  `Offset * size`; Radial is the distance from that centre over a quarter of width plus height; Conical the
  angle from the rotation in turns. `Scale` divides `t` about its middle; `TileMode` repeats or mirrors it.
- Keypoints under 1 / 256 apart draw from a 512-entry table over the part of the gradient the rect shows.

Fixtures: `gradients`, `gradient-edges`, `gradient-probe-*`, `stroke-gradient`.

## UICorner, borders and UIStroke

- Rounded fills: alpha is `gpu8(a8 / 255 * corner coverage)`; the corner radius is limited by half the
  unsnapped short side.
- Border strokes are drawn per side as Studio does (Outline, Middle, Inset borders; UIStroke Border with
  joins and its Center / Inner / Outer position) with the measured coverage of rounded corners.
- A UIStroke's thickness under UIScale and at fractional sizes follows the measured rounding
  (`render/text.rs stroke_radius` for text strokes).

Fixtures: `corners`, `strokes`, `stroke-*`.

## UIShadow

- The shadow is a mesh of the measured blur profile around the shape (BlurRadius, Offset, rounded corners
  on thin shapes); overlapping shadows of siblings combine as Studio draws them, one after the other.
- Features Studio gates off (`Inset`, `ShowBehindParent`, `Mode = Text`) are ignored, as in Studio.

Fixtures: `shadows*`, `shadow-probe-*`.

## Path2D

- A Path2D draws its curve as a stroke of its Color3, `Thickness` wide: a strip of triangles along the curve with
  a ramp on each long edge and square ends, as measured in Studio 0.747. Straight segments are one piece; curved
  ones are cut at up to thirty places, fewer where the curve is flat to a pixel.
- Corners are bevels: two triangles join one segment's end to the next one's start. A closed path's last corner,
  at its first point, is not joined and keeps a notch.
- Where the stroke overlaps itself (the inside of a corner, a crossing) partly covered pixels are covered twice.
- `Transparency` is not drawn: Studio keeps the value and draws the path opaque.
- It draws over its parent's background, text and UIStroke and under the parent's GuiObject children of the same
  ZIndex; paths of one ZIndex draw in child order. Clipping ancestors and CanvasGroups clip it; the parent's
  UICorner does not.

Fixtures: `path2d-strokes`, `path2d-edges`, `path2d-curves`, `path2d-thick`, `path2d-order`, `path2d-zindex`,
`path2d-zindex-global`, `path2d-layout`.

## Scroll bars

- The thumb is three image quads (top, middle, bottom) drawn like ImageLabels with
  ScrollBarImageColor3 and transparency. Its length is `max(2 * thickness + 1, round(visible * visible /
  canvas))` logical px, its offset `floor((visible - thumb) * position / (canvas - visible))`. Under a
  UIScale the bar is `ceil(ScrollBarThickness * UIScale)` px thick. Horizontal bars use the vertical
  artwork turned a quarter turn.

Fixtures: `scrollbars`, `image-probe-scroll*`, `image-probe-hscroll`.

## Draw order and layers

- ZIndex and sibling order as Studio draws them (Sibling and Global ZIndexBehavior).
- With `--all-layers`, every enabled ScreenGui under the PlayerGui and CoreGui is drawn: the PlayerGui pass
  under CoreGui's, then DisplayOrder, then, for equal DisplayOrders, the order the ScreenGuis were last
  parented or enabled, the latest on top (`fixtures/screen-layers.luau`, `cargo test --release layer_tests`).
