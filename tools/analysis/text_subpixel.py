"""Fits a sub-pixel offset per text label: Studio's pixels against our FreeType coverage (placed like
the renderer places it, at whole pixels) shifted by (dx, dy) with linear interpolation, for plain
solid-colour labels over a flat background.

usage: python tools/analysis/text_subpixel.py <out/compare/name> [--rur dir] [--filter text] [--step 0.0625]
Prints per label: the line top / pen x fractions, the best (dx, dy) and the error at (0, 0) vs best.
"""

import json
import math
import os
import sys

import freetype
import numpy as np
from PIL import Image

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "glyphs"))
from rasterize import FontFace, bitmap_rows  # noqa: E402


def arg(name, default=None):
    if name in sys.argv:
        return sys.argv[sys.argv.index(name) + 1]
    return default


def snap(v):
    return math.floor(v + 0.5)


FACES = {}


def glyph_bitmap(font, ppem, glyph_id):
    face = FACES.get(font["file"])
    if face is None:
        face = FontFace(font["file"])
        FACES[font["file"]] = face
    face.set_ppem(ppem)
    try:
        face.face.load_glyph(glyph_id, face.flags)
        face.face.glyph.render(freetype.FT_RENDER_MODE_NORMAL)
    except freetype.FT_Exception:
        return None, 0, 0
    rows = bitmap_rows(face.face.glyph.bitmap)
    if not rows:
        return None, 0, 0
    return np.array(rows, dtype=float) / 255, face.face.glyph.bitmap_left, face.face.glyph.bitmap_top


def node_pieces(scene, node):
    fonts = scene["fonts"]
    pieces = []
    for line in node["text"]["lines"]:
        ox = line.get("px")
        if ox is None:
            ox = snap(line["x"])
        top = snap(line["top"])
        for glyph in line["glyphs"]:
            font_index, glyph_id, gx, ppem = glyph[0], glyph[1], glyph[2], glyph[3]
            font = fonts[font_index]
            span = font["ascender"] - font["descender"]
            baseline = top + snap(font["ascender"] * ppem / font["unitsPerEm"] * (font.get("sizeUnits") or span) / (span + (font.get("lineGap") or 0)))
            bitmap, left, btop = glyph_bitmap(font, ppem, glyph_id)
            if bitmap is None:
                continue
            pieces.append((ox + snap(gx) + left, baseline - btop, bitmap))
    return pieces


def coverage(pieces, box, dx, dy):
    """Union coverage of the pieces (source-over) inside box, each shifted by (dx, dy) bilinearly."""
    x0, y0, x1, y1 = box
    out = np.zeros((y1 - y0, x1 - x0))
    for px, py, bitmap in pieces:
        h, w = bitmap.shape
        padded = np.zeros((h + 2, w + 2))
        padded[1:-1, 1:-1] = bitmap
        # value at integer pixel (X, Y) = bitmap sampled at (X - px - dx, Y - py - dy)
        fx, fy = math.floor(dx), math.floor(dy)
        ax, ay = dx - fx, dy - fy
        shifted = np.zeros((h + 3, w + 3))
        shifted[:-1, :-1] += (1 - ax) * (1 - ay) * padded
        shifted[:-1, 1:] += ax * (1 - ay) * padded
        shifted[1:, :-1] += (1 - ax) * ay * padded
        shifted[1:, 1:] += ax * ay * padded
        sx, sy = px - 1 + fx, py - 1 + fy
        X0, Y0 = max(x0, sx), max(y0, sy)
        X1, Y1 = min(x1, sx + w + 3), min(y1, sy + h + 3)
        if X1 <= X0 or Y1 <= Y0:
            continue
        region = out[Y0 - y0:Y1 - y0, X0 - x0:X1 - x0]
        piece = shifted[Y0 - sy:Y1 - sy, X0 - sx:X1 - sx]
        region += piece * (1 - region)
    return out


def main():
    compare = sys.argv[1]
    rur = arg("--rur", os.path.join(compare, "rur"))
    scene = json.load(open(os.path.join(rur, "scene.json"), encoding="utf8"))
    studio = np.asarray(Image.open(os.path.join(compare, "studio.png")).convert("RGB")).astype(float)
    background = np.array(scene.get("background") or [0, 0, 0]) * 255
    step = float(arg("--step", "0.0625"))
    only = arg("--filter")
    shifts = np.arange(-0.5, 0.5 + 1e-9, step)
    nodes = [n for n in scene["nodes"] if n.get("text") and n["text"].get("lines") and not n.get("strokes") and not n.get("gradient")
             and not n["text"].get("transparency") and not n["text"].get("stroke")]
    all_pieces = {n["id"]: node_pieces(scene, n) for n in nodes}
    # pixels covered by more than one label are skipped
    owner = np.zeros(studio.shape[:2], dtype=int)
    for n in nodes:
        for px, py, bitmap in all_pieces[n["id"]]:
            h, w = bitmap.shape
            owner[max(0, py - 2):py + h + 2, max(0, px - 2):px + w + 2] += 1 << 0
    for n in nodes:
        if only and only not in n.get("path", ""):
            continue
        pieces = all_pieces[n["id"]]
        if not pieces:
            continue
        x0 = min(p[0] for p in pieces) - 2
        y0 = min(p[1] for p in pieces) - 2
        x1 = max(p[0] + p[2].shape[1] for p in pieces) + 2
        y1 = max(p[1] + p[2].shape[0] for p in pieces) + 2
        box = (x0, y0, x1, y1)
        color = np.array([min(255, math.floor(c * 256)) for c in n["text"]["color"]], dtype=float)
        target = studio[y0:y1, x0:x1]
        valid = np.zeros(target.shape[:2], dtype=bool)
        for px, py, bitmap in pieces:
            h, w = bitmap.shape
            valid[max(0, py - 1 - y0):py + h + 1 - y0, max(0, px - 1 - x0):px + w + 1 - x0] = True
        valid &= owner[y0:y1, x0:x1] <= 1
        best = None
        base_err = None
        for dy in shifts:
            for dx in ([0.0] if "--no-x" in sys.argv else shifts):
                a = coverage(pieces, box, dx, dy)[..., None]
                model = background + (color - background) * a
                err = np.abs(np.round(model) - target).max(axis=2)[valid]
                score = (err ** 2).mean()
                if dx == 0 and dy == 0:
                    base_err = (score, (err > 0).mean(), err.max())
                if best is None or score < best[0]:
                    best = (score, dx, dy, (err > 0).mean(), err.max())
        line = n["text"]["lines"][0]
        print(f"{n.get('name', '?')[:26]:26s} top {line['top']:9.3f} (frac {line['top'] % 1:.3f})  x {line['x']:9.3f}  "
              f"base rms {math.sqrt(base_err[0]):6.2f} off {100 * base_err[1]:5.1f}% max {base_err[2]:3.0f}   "
              f"best dx {best[1]:+.4f} dy {best[2]:+.4f} rms {math.sqrt(best[0]):6.2f} off {100 * best[3]:5.1f}% max {best[4]:3.0f}")


if __name__ == "__main__":
    main()
