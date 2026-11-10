"""Reference model of Studio's text drawing, in numpy, for probe captures whose text sits on a flat
colour (the label's own opaque background or the capture background): every glyph quad (text strokes
first, then the fill) is blended one by one with Studio's arithmetic, solid colours or per-vertex
gradients (tools/analysis/text_gradient.py). Scores the model against the capture per text node.

usage: python tools/analysis/text_model.py <out/compare/name> [--rur dir] [--nodes a,b]
         [--label-gradient-on-stroke] [--stroke-rect label|grown]
"""

import json
import math
import os
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "glyphs"))
from rasterize import FontFace, bitmap_rows, render_stroked  # noqa: E402
import freetype  # noqa: E402
import text_gradient as tg  # noqa: E402


def arg(name, default=None):
    if name in sys.argv:
        return sys.argv[sys.argv.index(name) + 1]
    return default


def snap(v):
    return math.floor(v + 0.5)


def quantize(c):
    return min(255, math.floor(c * 256))


def stroke_radius(thickness, scale):
    t = np.float32(np.float32(thickness) * np.float32(scale))
    return math.floor(math.floor(np.float32(t * np.float32(10))) / 10 * 64) / 64


FACES = {}
CACHE = {}


def bitmap(font_file, ppem, glyph_id, stroke=0, join="round"):
    key = (font_file, ppem, glyph_id, stroke, join)
    if key in CACHE:
        return CACHE[key]
    face = FACES.get(font_file)
    if face is None:
        face = FontFace(font_file)
        FACES[font_file] = face
    face.set_ppem(ppem)
    try:
        if stroke:
            rows, left, top = render_stroked(face.face, glyph_id, stroke, face.flags, join)
        else:
            face.face.load_glyph(glyph_id, face.flags)
            face.face.glyph.render(freetype.FT_RENDER_MODE_NORMAL)
            rows = bitmap_rows(face.face.glyph.bitmap)
            left, top = face.face.glyph.bitmap_left, face.face.glyph.bitmap_top
    except freetype.FT_Exception:
        rows, left, top = [], 0, 0
    result = (np.array(rows, dtype=float) / 255 if rows else None, left, top)
    CACHE[key] = result
    return result


def glyph_pieces(scene, node, stroke_px=0, join="round"):
    """[(x, y, bitmap 0..1)] in draw order (lines top to bottom, glyphs left to right)."""
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
            bm, left, btop = bitmap(font["file"], ppem, glyph_id, stroke_px, join)
            if bm is None:
                continue
            pieces.append((ox + snap(gx) + left, baseline - btop, bm, glyph[4] if len(glyph) > 4 else None))
    return pieces


def model(scene, node, options):
    """(x0, y0, image) of the node's glyph region drawn over the scene background plus the node's own
    opaque background rect (snapped)."""
    S = scene["viewport"]["scale"]
    text = node["text"]
    rect = [v * S for v in node["rect"]]
    color = np.array([quantize(v) for v in text["color"]], dtype=float)
    Av = min(255, math.floor((1 - (text.get("transparency") or 0)) * 256))
    draws = []
    everything = []
    for stroke in node.get("strokes") or []:
        if stroke.get("mode") != "Contextual":
            continue
        px = stroke_radius(stroke["thickness"], S)
        if (stroke.get("transparency") or 0) >= 1 or not px > 0:
            continue
        join = "miter" if stroke.get("joins") == "Miter" else "bevel" if stroke.get("joins") == "Bevel" else "round"
        pieces = [p[:3] for p in glyph_pieces(scene, node, px, join)]
        everything += pieces
        scolor = np.array([quantize(v) for v in stroke["color"]], dtype=float)
        salpha = min(255, math.floor((1 - (stroke.get("transparency") or 0)) * 256))
        gradient = stroke.get("gradient")
        grect = rect
        if gradient and options.get("stroke_rect") == "grown":
            grow = stroke["thickness"] * S
            grect = [rect[0] - grow, rect[1] - grow, rect[2] + 2 * grow, rect[3] + 2 * grow]
        if not gradient and options.get("label_gradient_on_stroke") and node.get("gradient"):
            gradient = node["gradient"]
        draws += tg.glyph_draws(scene, node, scolor, salpha, gradient, grect, pieces)
    fills = glyph_pieces(scene, node)
    everything += fills
    runs = text.get("runs") or []
    for piece in fills:
        run = runs[piece[3]] if piece[3] is not None and piece[3] < len(runs) else {}
        run_color = np.array([quantize(v) for v in run["color"]], dtype=float) if run.get("color") else color
        run_alpha = Av
        if run.get("transparency") is not None:
            run_alpha = min(255, math.floor((1 - run["transparency"]) * (1 - (text.get("transparency") or 0)) * 256))
        if run_alpha > 0:
            draws += tg.glyph_draws(scene, node, run_color, run_alpha, node.get("gradient"), rect, [piece[:3]])
    if not everything:
        return None
    x0 = min(p[0] for p in everything) - 2
    y0 = min(p[1] for p in everything) - 2
    x1 = max(p[0] + p[2].shape[1] for p in everything) + 2
    y1 = max(p[1] + p[2].shape[0] for p in everything) + 2
    canvas = np.zeros((y1 - y0, x1 - x0, 3)) + np.array([quantize(v) for v in (scene.get("background") or [0, 0, 0])], dtype=float)
    bg = node.get("bg")
    if bg and (bg.get("transparency", 1) or 0) <= 0:
        rx, ry, rw, rh = node["rect"]
        L, T, R, B = snap(rx * S) - x0, snap(ry * S) - y0, snap((rx + rw) * S) - x0, snap((ry + rh) * S) - y0
        canvas[max(0, T):max(0, B), max(0, L):max(0, R)] = [quantize(v) for v in bg["color"]]
    tg.composite(canvas, x0, y0, draws)
    mask = np.zeros(canvas.shape[:2], dtype=bool)
    for piece in everything:
        px, py, bm = piece[:3]
        mask[py - y0:py - y0 + bm.shape[0], px - x0:px - x0 + bm.shape[1]] |= bm > 0
    return x0, y0, canvas, mask


def main():
    compare = sys.argv[1]
    rur = arg("--rur", os.path.join(compare, "rur"))
    scene = json.load(open(os.path.join(rur, "scene.json"), encoding="utf8"))
    studio = np.asarray(Image.open(os.path.join(compare, "studio.png")).convert("RGB")).astype(float)
    only = arg("--nodes")
    options = {"label_gradient_on_stroke": "--label-gradient-on-stroke" in sys.argv, "stroke_rect": arg("--stroke-rect", "label")}
    total = [0, 0]
    for node in scene["nodes"]:
        if not node.get("text") or not node["text"].get("lines"):
            continue
        if only and node.get("name") not in only.split(","):
            continue
        result = model(scene, node, options)
        if result is None:
            continue
        x0, y0, image, mask = result
        h, w = image.shape[:2]
        target = studio[max(0, y0):y0 + h, max(0, x0):x0 + w]
        image = image[max(0, -y0):, max(0, -x0):][:target.shape[0], :target.shape[1]]
        mask = mask[max(0, -y0):, max(0, -x0):][:target.shape[0], :target.shape[1]]
        diff = np.abs(image - target).max(axis=2)[mask]
        if diff.size == 0:
            continue
        total[0] += int((diff == 0).sum())
        total[1] += diff.size
        print(f"{node.get('name', '?'):16s} {100 * (diff == 0).mean():8.3f}%  max {int(diff.max()):3d}  ({diff.size} px)")
    print(f"total {100 * total[0] / max(1, total[1]):.3f}% of {total[1]} glyph px")


if __name__ == "__main__":
    main()
