"""Rasterizes the glyphs a scene uses exactly like Roblox (FreeType, grayscale, linear coverage)
into an alpha atlas.

The renderer rasterizes with the C port of this file compiled into the native crate
(native/src/glyphs/rur_glyph.c through native/src/glyphs.rs, FreeType 2.14.2; its atlases are
byte-identical to this script's with the same FreeType). This Python model stays for the analysis
scripts in tools/analysis that import FontFace / render_stroked; it needs freetype-py (and fontTools)
and uses whatever FreeType that freetype-py loads.

Measured against Studio captures (fixtures/text-raster.luau, tools/analysis/text_modes.py):
  - the size is requested as exact 16.16 scales, FT_DivFix((S + 1) * 64, ascender - descender),
    not as a 26.6 char size (the 1/64 rounding changes auto-hinter decisions)
  - TrueType fonts without bytecode (maxp.maxSizeOfInstructions == 0 or no fpgm, e.g. FredokaOne,
    Montserrat) are auto-hinted; hinted TrueType and CFF fonts use FreeType's native hinting
  - glyphs without a cmap entry (ligatures, alternates, e.g. FredokaOne "l_l") are auto-hinted with the
    fallback style: Studio's FreeType has no HarfBuzz coverage, freetype-py's would give them their
    script's style (fixtures/stroke-text-a.luau FillPlain: 175 px off -> 0)

UIStroke on text (measured, fixtures/stroke-text-*.luau): the FreeType stroker's band on both sides of
the same hinted outline (FT_Glyph_Stroke, round caps), radius = strokePx (the caller quantizes it like
Studio); joins Round -> ROUND, Bevel -> BEVEL, Miter -> MITER_VARIABLE with miter limit 2.

Synthetic bold (measured, fixtures/text-probe-synth.luau: FredokaOne, Bangers, PermanentMarker asked for
weight 700 / 900 through <b> or FontFace, 12..72 px, 100%): the hinted outline emboldened with
FT_Outline_EmboldenXY(strength, strength), strength = 2 * S in 26.6 units (S = the pixel size, S / 32 px),
before it is rendered or stroked (a UIStroke band follows the emboldened outline).

usage: python rasterize.py <request.json> <atlas.png> <atlas.json>
request: {"glyphs": [[fontFile, ppem, glyphId, strokePx?, style?], ...]}
style: "+"-separated tokens: a join ("miter" | "bevel"; round when absent), "b<strength>" (synthetic
bold, strength in 26.6 units) and "i" (synthetic italic)

Synthetic italic (measured, fixtures/text-probe-synth.luau and text-probe-rich.luau RichBoldFO: <i> and
FontFace Italic on faces without an italic file): FreeType's oblique shear (FT_GlyphSlot_Oblique's
0x0366A, tan 12 degrees) about the baseline, advances unchanged; a Studio row-by-row edge trace gives
slope 0.2167 through the baseline. Applied to the hinted (and emboldened) outline before it is filled or
stroked. Not bit-exact: Studio's slanted edges are irregular in a way no sheared outline, sheared-quad
resample or per-row shift of the upright bitmap reproduces (72 px italic word: 7,843 pixels off by more
than 2 upright, 3,641 sheared, 3,661 resampled, 3,896 row-shifted).
"""

import ctypes
import json
import math
import sys

import freetype
from freetype.raw import FT_Request_Size
from PIL import Image


class FT_Size_RequestRec(ctypes.Structure):
    _fields_ = [
        ("type", ctypes.c_int),
        ("width", ctypes.c_long),
        ("height", ctypes.c_long),
        ("horiResolution", ctypes.c_uint),
        ("vertResolution", ctypes.c_uint),
    ]


FT_Request_Size.argtypes = [ctypes.c_void_p, ctypes.POINTER(FT_Size_RequestRec)]
FT_SIZE_REQUEST_TYPE_SCALES = 4

FT_Outline_EmboldenXY = freetype.raw._lib.FT_Outline_EmboldenXY
FT_Outline_EmboldenXY.argtypes = [ctypes.c_void_p, ctypes.c_long, ctypes.c_long]
FT_Outline_EmboldenXY.restype = ctypes.c_int


def embolden(face, strength):
    """Synthetic bold of the loaded glyph slot's outline (strength in 26.6 units, both axes)."""
    outline = ctypes.cast(ctypes.pointer(face._FT_Face.contents.glyph.contents.outline), ctypes.c_void_p)
    FT_Outline_EmboldenXY(outline, strength, strength)


def parse_style(style):
    """(join, bold strength, italic) of a request's style string, e.g. "miter+b108+i" -> ("miter", 108, True)."""
    join, bold, italic = "round", 0, False
    for token in str(style or "").split("+"):
        if token in ("round", "miter", "bevel"):
            join = token
        elif token.startswith("b") and token[1:].isdigit():
            bold = int(token[1:])
        elif token == "i":
            italic = True
    return join, bold, italic


class FT_Matrix(ctypes.Structure):
    _fields_ = [("xx", ctypes.c_long), ("xy", ctypes.c_long), ("yx", ctypes.c_long), ("yy", ctypes.c_long)]


FT_Outline_Transform = freetype.raw._lib.FT_Outline_Transform
FT_Outline_Transform.argtypes = [ctypes.c_void_p, ctypes.POINTER(FT_Matrix)]
FT_Outline_Transform.restype = None
OBLIQUE = FT_Matrix(0x10000, 0x0366A, 0, 0x10000)


def oblique(face):
    """Synthetic italic of the loaded glyph slot's outline: FreeType's oblique shear about the baseline."""
    outline = ctypes.cast(ctypes.pointer(face._FT_Face.contents.glyph.contents.outline), ctypes.c_void_p)
    FT_Outline_Transform(outline, ctypes.byref(OBLIQUE))


class FT_Prop_GlyphToScriptMap(ctypes.Structure):
    _fields_ = [("face", ctypes.c_void_p), ("map", ctypes.POINTER(ctypes.c_ushort))]


FT_Property_Get = freetype.raw._lib.FT_Property_Get
FT_Property_Get.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.c_void_p]
FT_Property_Get.restype = ctypes.c_int


def unmapped_glyphs_to_fallback_style(face):
    """Gives glyphs the cmap does not reach the auto-hinter's fallback style (the style of .notdef),
    like a FreeType build without HarfBuzz coverage."""
    prop = FT_Prop_GlyphToScriptMap(ctypes.cast(face._FT_Face, ctypes.c_void_p), None)
    library = ctypes.cast(freetype.get_handle(), ctypes.c_void_p)
    if FT_Property_Get(library, b"autofitter", b"glyph-to-script-map", ctypes.byref(prop)) != 0 or not prop.map:
        return
    mapped = set()
    code, index = face.get_first_char()
    while index:
        mapped.add(index)
        code, index = face.get_next_char(code, index)
    fallback = prop.map[0]
    for glyph in range(face.num_glyphs):
        if glyph not in mapped:
            prop.map[glyph] = fallback


def unhinted_truetype(path):
    """True for TrueType fonts that carry no hinting bytecode."""
    try:
        from fontTools.ttLib import TTFont
    except ImportError:
        return False
    font = TTFont(path, lazy=True)
    if "glyf" not in font:
        return False
    max_instructions = getattr(font["maxp"], "maxSizeOfInstructions", 0)
    return max_instructions == 0 or "fpgm" not in font


class FontFace:
    def __init__(self, path):
        self.face = freetype.Face(path)
        self.flags = freetype.FT_LOAD_FORCE_AUTOHINT if unhinted_truetype(path) else freetype.FT_LOAD_DEFAULT
        self.height_units = self.face.ascender - self.face.descender
        self.upm = self.face.units_per_EM
        if self.flags & freetype.FT_LOAD_FORCE_AUTOHINT:
            unmapped_glyphs_to_fallback_style(self.face)
        # FreeType 2.14 separates dotted Latin letters from their stems. Older bundled
        # libraries (freetype-py ships 2.13.2) need the same outline adjustment (aflatin.c's
        # vertical separation); 2.14 applies its native rule.
        self.separate_dots = (
            {self.face.get_char_index(code) for code in (ord("i"), ord("j"))} - {0}
            if freetype.version() < (2, 14, 0) and self.flags & freetype.FT_LOAD_FORCE_AUTOHINT
            else set()
        )

    def set_ppem(self, ppem):
        # ppem = (S + 1) * upm / (ascender - descender); request the line height S + 1 exactly
        line26 = int(round(ppem * self.height_units / self.upm * 64))
        scale = (line26 * 65536 + self.height_units // 2) // self.height_units
        request = FT_Size_RequestRec(FT_SIZE_REQUEST_TYPE_SCALES, scale, scale, 0, 0)
        FT_Request_Size(ctypes.cast(self.face._FT_Face, ctypes.c_void_p), ctypes.byref(request))

    def adjust_outline(self, glyph_id):
        """Backport the Latin i/j one-pixel separation before rasterizing or stroking."""
        if glyph_id not in self.separate_dots:
            return
        outline = self.face.glyph.outline
        points, ends = outline.points, outline.contours
        starts = [0] + [end + 1 for end in ends[:-1]]
        contours = [list(range(start, end + 1)) for start, end in zip(starts, ends)]
        if len(contours) < 2:
            return
        bounds = [
            (min(points[i][0] for i in contour), min(points[i][1] for i in contour),
             max(points[i][0] for i in contour), max(points[i][1] for i in contour))
            for contour in contours
        ]
        high = max(range(len(bounds)), key=lambda i: (bounds[i][3], bounds[i][1]))
        left, bottom, right, top = bounds[high]
        others = [bounds[i] for i in range(len(bounds)) if i != high and len(contours[i]) > 2]
        if len(contours[high]) <= 2 or not others:
            return
        if right < min(b[0] for b in others) or left > max(b[2] for b in others):
            return
        # FreeType's fallback when its internal blue-zone metrics are unavailable:
        # an accent may occupy at most 0.4 em. Sizes and coordinates are 26.6.
        height_limit = ((self.upm * 4 // 10) * self.face.size.y_scale + 32768) // 65536
        height = top - bottom
        if height > height_limit:
            return
        distance = min([64] + [bottom - b[3] for b in others if b[1] < bottom])
        adjustment = 64 - distance
        if not 0 < adjustment <= 66:
            return
        # Move contours in the same vertical band together, preserving counters.
        minimum = bottom - height // 8
        raw_points = self.face._FT_Face.contents.glyph.contents.outline.points
        for contour, bound in zip(contours, bounds):
            if bound[1] < bound[3] and bound[1] > minimum:
                for index in contour:
                    raw_points[index].y += adjustment

PADDING = 1
# The library in use (cli/freetype.mjs picks it: the pinned 2.14.2 build or freetype-py's bundled one)
FREETYPE_VERSION = ".".join(map(str, freetype.version()))


def main():
    request = json.load(open(sys.argv[1], encoding="utf8"))
    rasterize(request, sys.argv[2], sys.argv[3], {})


def rasterize(request, atlas_png, atlas_json, faces):
    entries = []
    for key in request["glyphs"]:
        font_file, ppem, glyph_id = key[0], key[1], key[2]
        stroke = key[3] if len(key) > 3 else 0
        join, bold, italic = parse_style(key[4] if len(key) > 4 else None)
        font = faces.get(font_file)
        if font is None:
            font = FontFace(font_file)
            faces[font_file] = font
        font.set_ppem(ppem)
        face = font.face
        try:
            if stroke:
                bitmap, left, top = render_stroked(face, glyph_id, stroke, font.flags, join, bold, font.adjust_outline, italic)
            else:
                face.load_glyph(glyph_id, font.flags | (freetype.FT_LOAD_NO_BITMAP if bold or italic else 0))
                font.adjust_outline(glyph_id)
                if bold:
                    embolden(face, bold)
                if italic:
                    oblique(face)
                face.glyph.render(freetype.FT_RENDER_MODE_NORMAL)
                bm = face.glyph.bitmap
                bitmap = bitmap_rows(bm)
                left, top = face.glyph.bitmap_left, face.glyph.bitmap_top
        except freetype.FT_Exception:
            bitmap, left, top = [], 0, 0
        entries.append({"key": key, "bitmap": bitmap, "left": left, "top": top})

    # Shelf-pack into an atlas.
    width = 2048
    x = y = shelf = 0
    for entry in entries:
        rows = entry["bitmap"]
        w = len(rows[0]) if rows else 0
        h = len(rows)
        if x + w + PADDING > width:
            x = 0
            y += shelf + PADDING
            shelf = 0
        entry["x"], entry["y"], entry["w"], entry["h"] = x, y, w, h
        x += w + PADDING
        shelf = max(shelf, h)
    height = max(1, y + shelf + PADDING)

    image = Image.new("L", (width, height), 0)
    pixels = image.load()
    for entry in entries:
        for row_index, row in enumerate(entry["bitmap"]):
            for col_index, value in enumerate(row):
                if value:
                    pixels[entry["x"] + col_index, entry["y"] + row_index] = value
    rgba = Image.new("RGBA", (width, height), (255, 255, 255, 0))
    rgba.putalpha(image)
    rgba.save(atlas_png)

    out = []
    for entry in entries:
        out.append({
            "key": entry["key"],
            "x": entry["x"],
            "y": entry["y"],
            "w": entry["w"],
            "h": entry["h"],
            "left": entry["left"],
            "top": entry["top"],
        })
    json.dump({"width": width, "height": height, "freetype": FREETYPE_VERSION, "glyphs": out},
              open(atlas_json, "w", encoding="utf8"))


def bitmap_rows(bm):
    if bm.width == 0 or bm.rows == 0:
        return []
    data = bm.buffer
    return [list(data[r * bm.pitch: r * bm.pitch + bm.width]) for r in range(bm.rows)]


STROKER_JOINS = {
    "round": (freetype.FT_STROKER_LINEJOIN_ROUND, 0),
    "bevel": (freetype.FT_STROKER_LINEJOIN_BEVEL, 0),
    # FT_STROKER_LINEJOIN_MITER_VARIABLE, miter limit 2.0 (16.16)
    "miter": (2, 2 << 16),
}


def render_stroked(face, glyph_id, stroke_px, flags, join="round", bold=0, adjust_outline=None, italic=False):
    """The FreeType stroker's band of radius stroke_px around the glyph outline (both sides), around the
    emboldened / slanted outline for synthetic bold / italic."""
    face.load_glyph(glyph_id, flags | freetype.FT_LOAD_NO_BITMAP)
    if adjust_outline:
        adjust_outline(glyph_id)
    if bold:
        embolden(face, bold)
    if italic:
        oblique(face)
    glyph = face.glyph.get_glyph()
    stroker = freetype.Stroker()
    line_join, miter_limit = STROKER_JOINS.get(join, STROKER_JOINS["round"])
    stroker.set(int(round(stroke_px * 64)), freetype.FT_STROKER_LINECAP_ROUND, line_join, miter_limit)
    glyph.stroke(stroker, True)
    blyph = glyph.to_bitmap(freetype.FT_RENDER_MODE_NORMAL, freetype.Vector(0, 0), True)
    return bitmap_rows(blyph.bitmap), blyph.left, blyph.top


if __name__ == "__main__":
    main()
