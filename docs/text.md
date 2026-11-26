# Text measurement and rendering

How `rur` shapes, measures (`native/src/engine/text/`), rasterizes (`native/src/glyphs.rs`), and composites (`native/src/render/text.rs`) text to match Roblox Studio. Constants are verified against `data/studio/` probes and `text-*` compare fixtures.

## Text modes

Roblox toggles between two text layout/rendering pipelines using the server flag `FFlagLegacyTextSubpixelRendering`. `rur` supports both:

| Mode | Roblox flag | Behavior |
|---|---|---|
| **Whole-pixel** *(default)* | Off | Fully hinted glyph bitmaps. Each glyph advance (plus kerning) rounds half-up to a whole pixel independently. Non-integer pixel sizes rasterize at the floored size and scale quads up. |
| **Quarter-pixel** | On | Lightly hinted glyphs with quarter-pixel pens up to 32 GUI px. A continuous pen accumulates exact 26.6 advances and rounds once per glyph position, so line width equals `round(pen_end) - round(pen_start)`. |

The default tracks production Roblox (as of 2026-11-12, whole-pixel is active on Studio, desktop, and mobile). Switching modes affects `TextBounds`, line wrapping, truncation, and `TextScaled`.

- **CLI / env switch:** Pass `--text-subpixel on` (quarter-pixel) or `--text-subpixel off` (whole-pixel), or set `RUR_TEXT_SUBPIXEL=on`. Run `rur doctor` to see the active mode. The web build sets it via its config `env`.
- **Maintaining both modes:** Every text fixture has goldens in both modes (`edit/<name>` and `edit/<name>@quarter`), separate frozen corpora (`text-corpus.whole.json` / `text-corpus.json`), and separate Studio measurement datasets (`data/studio/whole/` / `data/studio/`). The whole-pixel datasets exist at device scales 1, 1.5, 2 and 3 (`*-1x.json` ... `*-3x.json`; phones run at 2 to 3), measured in a Roblox client with `FFlagTextBoundsRoundUpToLogicalPixels` off: production has it on, and a script then reads every `TextBounds` rounded up to a whole logical pixel, as `rur` reports it ([layout.md](layout.md#text-boxes)); the datasets keep the unrounded value line breaks and `TextScaled` are decided on. If Roblox flips the production flag again, update `subpixel_setting` in `native/src/engine/text/mod.rs` and `textSubpixel` in `cli/paths.mjs`, swap the `@quarter` target in `tools/engine/golden.mjs`, and re-record goldens.

## Fonts and fallback

- Local built-in fonts load from Studio's `content/fonts` (override with `RUR_ROBLOX_CONTENT`). Cloud fonts (`rbxassetid://`) download into the asset cache. No font files ship in this repository.
- `Enum.Font` maps to Studio family files (`rbxasset://fonts/families/Arial.json` measures as Arimo, matching Studio).
- Missing glyphs fall back through: Source Sans Pro Regular → Arimo → RobloxEmoji → Twemoji → Noto script fonts → cached cloud CJK fallback.
- **Without Studio's fonts** (no Studio install, or `RUR_ROBLOX_CONTENT` unset or naming a folder without `fonts/`), a built-in family and its Source Sans Pro fallback have no face, so text in it is not drawn. rur has no font of its own to stand in; a cloud font still draws once it is downloaded. Such a render says so: `summary.json`'s `missingFonts` lists the family (`scene.json`: `diagnostics.unavailableFonts`, and a `diagnostics.fonts` message), `rur inspect` notes each label that drew no text, and `rur doctor` warns about the Studio folder.
- **Emoji sequences:** Variation selectors, ZWJ, skin-tone modifiers, keycaps, and tag characters inherit the preceding code point's face so emoji ligatures shape in a single run. Default-ignorable code points have zero advance.

## Metrics and shaping

- **Face sizing:** Faces scale so their `hhea` line height including line gap (`ascender - descender + lineGap`) equals `S + 1` pixels for pixel size `S` (faces with non-zero line gaps include Arimo, Ubuntu, Permanent Marker, AccanthisADFStd, Guru, and cloud fonts like Poppins).
  - Without a line gap, FreeType requests `(S + 1) * 64` in 26.6 fixed point.
  - With a line gap, it requests `floor((S + 1) * 64 * (ascender - descender) / (ascender - descender + lineGap))`.
- **Whole-pixel mode:** Each advance (including kerning) rounds half-up individually; the truncation ellipsis (`…`) uses its FreeType hinted advance.
- **Quarter-pixel mode:** Advances use 26.6 fixed point (`FT_MulFix(advance, x_scale) + GPOS/kern`). One continuous pen runs across rich-text runs, wrapped lines, and newlines, rounding half-up at each glyph (Studio 0.746+).
- **Rich text:** Runs shape independently (no cross-run kerning). `<font size=N>` resolves to `floor(f32(f32(N * UIScale) * DPI))`. The base rich-text size round-trips through unscaled units in `f32`, which can yield 1 px smaller than plain text at identical `TextSize` (e.g., `30` at `UIScale 0.885` is 44 px rich vs. 45 px plain).
- **Truncation ellipsis:** Starts its own pen: total width equals kept whole-pixel width + rounded ellipsis advance. (Legacy hinted-ellipsis metrics remain cached under `<state dir>/hinted`.) Truncated rich-text lines draw the ellipsis in the label's face matching the last kept glyph's size, color, and weight, while `TextBounds` stays the longest fitting prefix.
- **Combining marks & baselines:** Base + combining marks compose when the font contains the precomposed glyph. Line baselines come from the label's (or rich-text run's) primary face even when individual glyphs fall back to emoji or other fonts.

## Rasterization

- Uses compiled-in **FreeType 2.14.2** (`native/build.rs`, `src/glyphs/rur_glyph.c`) producing grayscale bitmaps at whole device pixels.
- **Glyphs over 150 px** (`LegacyArial` at `TextSize` 68 and up at 1.5x: the face is sized 1.5 times the `TextSize`) are rasterized at 150 px and drawn on quads scaled by `size / 150` about the pen, like a fractional size, so they are soft on both axes; the line keeps the glyph's own size.
- **Synthetic bold:** Applied when requested weight >= 600 and the resolved face is < 600 via `FT_Outline_EmboldenXY(2*S, 2*S)` in 26.6 units (advances unchanged).
- **`UIStroke` on text:** FreeType stroker band with radius `floor(10 * t) / 10` px (`t = f32(thickness * DPI)`) and opacity `alpha8(1 - stroke_transparency)` (independent of `TextTransparency`).
- **Legacy `TextStroke`:** Renders glyph fills four times at diagonal offsets `(±1, ±1)` behind the fill.

## Placement and compositing

- **Line start:** Content alignment anchor snaps half-up: `Left = round(x)`, `Right = round(x + w) - width`, `Center = round(x + w/2) - floor(width/2)` (rich text shifts odd-width centered lines 1 px right).
- **Glyph positions:** Placed at `line_pen + prior_advances` along the line baseline (determined by the largest pixel size on the line, shared by fallback glyphs and `<font size>` runs).
- **Rotated & fractional text:** Rotated text skips pen/line-top rounding, rotating upright bitmaps around the label center with 8-bit bilinear resampling. Text in a rotated `CanvasGroup` is drawn the same way into the group's texture: unrounded pens and line top, glyphs resampled there (once, also in a resampled texture). Fractional text sizes (rich text under certain `UIScale`s) draw floored-size bitmaps scaled by `unfloored / floored` around each glyph's pen point.
- **Draw order:** `<mark>` highlights → `UIStroke` → rich `<stroke>` → legacy `TextStroke` → glyph fills → `<u>` / `<s>` decorations.
- **Blending:** Straight alpha per glyph quad: `A = gpu8(g/255 * alpha8(1 - TextTransparency)/255)`, blended with `round((c * A + D * (255 - A)) / 255)` ([images.md](images.md)).
- **`<u>` and `<s>`:** Whole-pixel rects of thickness `ceil(S / DPI / 20)` logical px; underline aligns to the bottom rows of the run box, strikethrough starts at `ceil(S / 2) + 1`.
- **`UIGradient` on text:** Evaluates across the label's rect per texel ([images.md](images.md#uigradient)), also tinting strokes that don't have their own gradient.

Fixtures: `text-raster`, `text-probe-blend`, `text-probe-rich`, `text-probe-decor*`, `text-probe-synth`, `text-probe-rotate`, `text-probe-groupturned*`, `text-probe-gradient*`, `text-probe-uiscale`, `layout-textpen*`, `layout-truncate*`.
