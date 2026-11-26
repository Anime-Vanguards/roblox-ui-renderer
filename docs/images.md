# Compositing and rasterization rules

How `rur`'s wgpu renderer (`native/src/render/`, `shaders/uber.*.glsl`) matches Roblox Studio pixel for pixel. All measurements target Studio at 1.5x device scale on D3D11 (`rur compare` measures `exactPercent` as the share of pixels within 2/255 on every channel).

## Alpha and blending

- **Transparency to 8-bit alpha:** `alpha8(T) = min(255, floor((1 - T) * 256))` (e.g., `0.1 → 230`, `0.5 → 128`, `0.9 → 25`).
- **Shader quantization:** Outputs quantize to 8 bits before blending via `gpu8(x) = (floor(x * 4096) * 255 + 2047) >> 12`.
- **Blend formula:** Floating-point over-blend with single rounding: `round((c * A + D * (255 - A)) / 255)` for source color `c`, alpha `A`, and destination `D` (applies to solid fills and gradients alike).

Fixtures: `group-blend`, `rects`.

## Edge snapping

Every rect, clip region, border, and image quad snaps to physical device pixels using `render/geometry.rs` (`snap_device`): Studio scales `AbsolutePosition` and `AbsoluteSize` independently by DPI in `f32`, sums them in `f32`, and snaps each edge half-up with a small DPI-scaled tie tolerance.

Fixtures: `image-probe-ties`, `snap-probe-ties1x`.

## Images

- **Axis-aligned quads:** Snapped edges sampled bilinearly with 8-bit weights at texel centers. Minified textures use mipmaps, rounding odd dimensions down on the even axis.
- **Scale types:** `Slice` (9-slice), `Tile`, `Fit`, and `Crop` match Studio's quad layout; `ImageRectOffset` / `ImageRectSize` crop the source texture.
- **`UICorner` on images:** Each image quad is rounded on its own with coarse 2 x 2 derivatives on even device pixels. A `Stretch` image's rounded rect is its snapped quad; a `Fit`, `Crop` or `Tile` image's and each 9-slice part's is the unsnapped quad stretched over the snapped one. In a `CanvasGroup`'s texture the same mask covers the quad's texels, each texel centre at its place in the quad.
- **Decoding:** PNG (any bit depth and colour type; 16-bit samples keep their high byte), JPEG (within one level of Studio's decoder on a few texels), straight alpha with the colour under transparent texels kept. `SliceCenter` and `ImageRect` of a texture over 1024 px are in the units of the image scaled to 1024 px on its longer side.
- **Rotated images:** Corners snap to the 1/256 px rasterizer grid with UVs interpolated across two triangles. Mip selection considers texel span horizontally, vertically, and along both diagonals; anti-aliased fringes sample along the edge line at their strip's mip level. If ancestor/self rotations cancel to 0°, fills and images fall back to axis-aligned edge snapping.
- **`EditableImage`:** Rendered without mipmaps (aliases on minification, matching Studio).
- **Thumbnails (`rbxthumb://`):** Fetched as WebP matching Studio.

Fixtures: `image-probe-*` (using built-in `rbxasset://` textures).

## CanvasGroup

- **Offscreen allocation:** Allocated in whole logical pixels: `N = ceil(ceil(Q / DPI) * DPI)` device px for snapped quad `Q` (resampled if `N != Q`). Large groups clamp to Studio's area and side limits (`render/geometry.rs` `group_texture_dims`), in device px at every device scale: a group 2,800 logical px wide is 8,400 px at 3x and gets the same 2,049-texel texture as at 1.5x.
- **Rendering & compositing:** Children snap on the screen grid and scale into the premultiplied group texture (child images and text sample in texture space). Compositing unpremultiplies, applies `GroupColor3`, the group's `UIGradient`, `GroupTransparency`, and `UICorner`, and blends once like any draw, `round((c A + D (255 - A)) / 255)`.
- **Borders:** Drawn over the composite at normal `Frame` border pixels with opacity `(1 - BackgroundTransparency) * (1 - GroupTransparency)`, modulated by `UIGradient` (not `GroupColor3`). The group fill covers the entire rect in all `BorderMode`s.
- **Rotated groups:** Render axis-aligned into their texture, then draw as a rotated quad (preserving child antialiasing unless rotated by a 90° multiple). The group's own background fills the texture over whole texels; text inside is placed by the rotated-text rule (unrounded pens and line top) and resampled there ([text.md](text.md)). If a rotated `CanvasGroup` has a `UIGradient`, the gradient evaluates in group coordinates and outer quad edges are not antialiased (matching rotated `Frame` + `UIGradient`).

- **Texture budget:** A Roblox client keeps its CanvasGroups' textures (width x height x 4 bytes each) within a memory budget set by its graphics quality level. From level 7 up (7 to 10 and 21 measured) every group draws by the rules above while their total stays within the budget, and past it every group is left blank: at level 21 the budget is about 96 MiB (device scale 3: 46 groups of 900 x 600 px and one of 501 x 501, 100.4 MB, draw; with one of 612 x 612 instead, 100.9 MB, all of them are blank). At levels 1 to 6 groups go blank in sets that change from scene to scene, and their textures have a lower resolution. A client's Automatic setting picks the level from the frame rate, so which groups a phone leaves blank depends on the device; Studio draws every group, and so does `rur`, as a client at the top level does within the budget.

Fixtures: `group-resample`, `group-misc`, `group-blend`, `canvasgroup*`, `group-probe-*`.

## UIGradient

Evaluated per pixel (measured in Studio 0.747):
- Color interpolates between surrounding keypoints in **squared space** (squared keypoint colors stored at 8-bit precision), opacity interpolates linearly, and an ordered 4x4 Bayer dither runs before 8-bit quantization.
- **Coordinate `t`:**
  - `Linear`: screen-space angle from `Rotation` across the unsnapped rect, shifted by `Offset * size`.
  - `Radial`: distance from shifted center divided by `(width + height) / 4`.
  - `Conical`: normalized angle from `Rotation` in turns.
  - `Elliptical`: distance from the shifted center in the ellipse that touches the rect's edges (`0` at the center,
    `1` at the middle of each edge, so the corners take the last keypoint's value). The ellipse turns with `Rotation`
    (its `width / 2` axis along the rotation's direction); `Offset` moves the center in screen axes.
  - `Scale` scales `t` around its midpoint (the center for `Radial` and `Elliptical`); `TileMode` repeats or mirrors.
- Keypoints spaced closer than `1/256` sample from a 512-entry lookup table over the visible gradient range
  (for `Radial` and `Elliptical`, `t` from 0 to `1 / Scale`; for `Conical`, 0 to `1 / min(Scale, 1)`).

Fixtures: `gradients`, `gradient-edges`, `gradient-probe-*` (`gradient-probe-elliptical` for `Elliptical`), `stroke-gradient`.

## UICorner, borders, and UIStroke

- **Rounded fills:** Alpha is `gpu8(a8 / 255 * corner_coverage)`; corner radius clamps to half of the unsnapped shorter side.
- **Borders & strokes:** `Outline`, `Middle`, and `Inset` borders and `UIStroke` (`Center` / `Inner` / `Outer` + joins) match Studio's per-side geometry and rounded-corner coverage. Thickness under `UIScale` and fractional sizes uses Studio's rounding (`render/text.rs` `stroke_radius` for text strokes).
- **Sharp stroke edges:** a sharp, unturned `Round` stroke reaching up to 1 + 1/sqrt(2) logical px past its frame has no outer margin: a pixel centred on its outer edge follows the top-left rule. Past that reach, and on turned frames, pixel centres up to 0.1 logical px beyond the edge are drawn (any device scale).
- **Edges on the GPU grid:** a stroke's straight edges are snapped to 1/256 device px as a GPU snaps a quad's corners, so an edge within 1/512 px of a pixel centre lies on it.

Fixtures: `corners`, `strokes`, `stroke-*` (`stroke-probe-margin-scale`, `stroke-probe-snap` for the two rules above).

## UIShadow

Rendered via a blur-profile mesh honoring `BlurRadius`, `Offset`, and rounded corners on thin shapes. Sibling shadows composite sequentially like Studio. Studio-gated properties (`Inset`, `ShowBehindParent`, `Mode = Text` where disabled) follow Studio's active behavior.

- **Hard shadows:** with `BlurRadius` 0 the shape is filled at the blur profile's top byte times the vertex alpha `round(255 * (1 - Transparency))` (halves up): `Transparency` 0 draws 254, 0.5 draws 127.
- **`Mode = Text`:** with `BlurRadius` 0, the text's own glyphs in the shadow's color. Above 0, each glyph glows from an 8-bit distance field of the glyph at TextSize 56 (56 texels per text size), drawn on a quad whose corners are rounded to whole device pixels and faded by the shadow's `Transparency` before the glyphs' glows blend. Color emoji glow from one field per color layer, in the layer's own color. Rich-text underlines and strikethroughs glow like a shadow of their rect (sigma = half the glow width). A `<mark>` highlight does not glow and lies under the glow, which tints it.

Fixtures: `shadows*`, `shadow-probe-*`.

## Path2D

Measured in Studio 0.747:
- Draws a triangle strip of width `Thickness` in `Color3` with antialiased ramps along the long edges and square caps. Straight segments are single quads; curves subdivide up to 30 times (fewer when flat within 1 px).
- **Corners:** Two-triangle bevel joins between consecutive segments. Closed paths leave the closing corner at control point 1 unjoined (leaving a small notch, matching Studio).
- **Self-overlap:** Partly covered pixels double-blend at sharp inner corners and crossings.
- **`Transparency`:** Stored on the instance, ignored during rendering (always drawn opaque, matching Studio).
- **Ordering & clipping:** Draws above its parent's background, text, and `UIStroke`, below child `GuiObject`s of the same `ZIndex`, and in child order among equal-`ZIndex` paths. Clipped by clipping ancestors and `CanvasGroup`s, not by its parent's `UICorner`.
- **Long paths:** a path longer than a texture is wide (8,192 device px) draws the part on the screen.

Fixtures: `path2d-strokes`, `path2d-edges`, `path2d-curves`, `path2d-thick`, `path2d-order`, `path2d-zindex`, `path2d-zindex-global`, `path2d-layout`, `path2d-probe-wide`.

## Scroll bars

Rendered as three image quads (top, middle, bottom) tinted by `ScrollBarImageColor3` and `ScrollBarImageTransparency`:
- **Thumb length:** `max(2 * thickness + 1, round(visible * visible / canvas))` logical px.
- **Thumb offset:** `floor((visible - thumb) * position / (canvas - visible))`.
- **Thickness under `UIScale`:** `ceil(ScrollBarThickness * UIScale)`. Horizontal bars rotate the vertical artwork by 90°.

Fixtures: `scrollbars`, `image-probe-scroll*`, `image-probe-hscroll`.

## Draw order and layers

- Respects `ZIndex`, sibling order, and both `Sibling` and `Global` `ZIndexBehavior`.
- With `--all-layers`, every enabled `ScreenGui` in `PlayerGui` renders beneath `CoreGui`, sorted by `DisplayOrder` and then by most recently parented or enabled (`fixtures/screen-layers.luau`, `cargo test --release layer_tests`).
