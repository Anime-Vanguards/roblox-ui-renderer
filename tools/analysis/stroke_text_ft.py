# Compares Studio's text stroke coverage (white stroke on black, TextTransparency 1) with candidate
# rasterizations of the stroke band placed like the renderer places glyph bitmaps.
#   python stroke_text_ft.py <out/compare/run> <name-prefix> [mode]
# modes: ft (FreeType stroker, as tools/glyphs/rasterize.py), sdf (exact distance band, linear AA)
import json
import math
import os
import sys

import freetype
import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "glyphs"))
from rasterize import FontFace, bitmap_rows, render_stroked  # noqa: E402
from stroke_text import glyph_fields, snap  # noqa: E402


def studio_radius(t):
    """Measured: the FreeType stroker radius is floor(10 t) / 10 physical px, truncated to 26.6."""
    return math.floor(math.floor(t * 10 + 1e-9) / 10 * 64 + 1e-9) / 64


JOINS = {
    "Round": freetype.FT_STROKER_LINEJOIN_ROUND,
    "Bevel": freetype.FT_STROKER_LINEJOIN_BEVEL,
    "Miter": 3,  # FT_STROKER_LINEJOIN_MITER_FIXED
    "MiterVariable": 2,  # FT_STROKER_LINEJOIN_MITER_VARIABLE
}


def render_stroked_join(face, glyph_id, radius, flags, join, limit):
    face.load_glyph(glyph_id, flags | freetype.FT_LOAD_NO_BITMAP)
    glyph = face.glyph.get_glyph()
    stroker = freetype.Stroker()
    stroker.set(int(round(radius * 64)), freetype.FT_STROKER_LINECAP_ROUND, JOINS[join], int(limit * 65536))
    glyph.stroke(stroker, True)
    blyph = glyph.to_bitmap(freetype.FT_RENDER_MODE_NORMAL, freetype.Vector(0, 0), True)
    return bitmap_rows(blyph.bitmap), blyph.left, blyph.top


def ft_band(scene, node, t, joins="Round", limit=0):
    """FreeType stroker bitmaps per glyph -> list of (x, y, alpha grid) pieces."""
    fonts = scene["fonts"]
    pieces = []
    faces = {}
    for line in node["text"]["lines"]:
        ox = snap(line["x"])
        top = snap(line["top"])
        for font_index, glyph_id, gx, ppem in line["glyphs"]:
            font = fonts[font_index]
            face = faces.get(font_index)
            if face is None:
                face = FontFace(font["file"])
                faces[font_index] = face
            face.set_ppem(ppem)
            if joins == "Round" and limit == 0:
                rows, left, top_bm = render_stroked(face.face, glyph_id, round(t * 64) / 64, face.flags)
            else:
                rows, left, top_bm = render_stroked_join(face.face, glyph_id, t, face.flags, joins, limit)
            if not rows:
                continue
            baseline = top + snap(font["ascender"] * ppem / font["unitsPerEm"])
            pieces.append((ox + snap(gx) + left, baseline - top_bm, np.array(rows, dtype=float) / 255))
    return pieces


def composite(shape, pieces, kind="max"):
    canvas = np.zeros(shape)
    for x, y, a in pieces:
        h, w = a.shape
        region = canvas[y:y + h, x:x + w]
        if kind == "max":
            np.maximum(region, a[:region.shape[0], :region.shape[1]], out=region)
        else:
            region += a[:region.shape[0], :region.shape[1]] * (1 - region)
    return canvas


if __name__ == "__main__":
    run, prefix = sys.argv[1], sys.argv[2]
    mode = sys.argv[3] if len(sys.argv) > 3 else "ft"
    scene = json.load(open(f"{run}/rur/scene.json"))
    studio = np.asarray(Image.open(f"{run}/studio.png").convert("RGB")).astype(float)[:, :, 0] / 255
    S = scene["viewport"]["scale"]
    total = 0
    nodes = [n for n in scene["nodes"] if n.get("text") and n.get("strokes") and n["text"].get("transparency", 0) >= 1]
    pieces = []
    for node in nodes:
        t = node["strokes"][0]["thickness"] * S
        if mode == "ft":
            pieces += ft_band(scene, node, studio_radius(t))
        else:
            for x0, y0, sd, key in glyph_fields(scene, node, pad=int(t) + 4):
                pieces.append((x0, y0, np.clip(t + 0.5 - np.abs(sd), 0, 1)))
    pred = composite(studio.shape, pieces, sys.argv[4] if len(sys.argv) > 4 else "max")
    for node in nodes:
        if not node.get("name", "").startswith(prefix):
            continue
        t = node["strokes"][0]["thickness"] * S
        x, y, w, h = node["rect"]
        X0, Y0, X1, Y1 = snap(x * S) - 10, snap(y * S) - 10, snap((x + w) * S) + 10, snap((y + h) * S) + 10
        p = np.floor(pred[Y0:Y1, X0:X1] * 255 + 1e-6)
        s = np.round(studio[Y0:Y1, X0:X1] * 255)
        diff = np.abs(p - s)
        bad = int((diff > 2).sum())
        total += bad
        print(f"{node['name']:6s} t={t:7.4f} bad={bad:5d} max={diff.max():4.0f} mean={diff[(p > 0) | (s > 0)].mean():6.2f} sum(studio-model)={(s - p).sum() / 255:8.1f}")
    print("total", total)
