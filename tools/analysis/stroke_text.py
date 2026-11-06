# Text UIStroke analysis: exact signed distance of each pixel centre to the (hinted) FreeType glyph
# outline, placed like the renderer places glyph bitmaps, next to Studio's stroke coverage.
#   python stroke_text.py <out/compare/run> <node name> [channel]
import json
import math
import os
import sys

import freetype
import numpy as np
from PIL import Image

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "glyphs"))
from rasterize import FontFace  # noqa: E402


def snap(v):
    return math.floor(v + 0.5)


def flatten_outline(outline, steps=24):
    """Closed polylines (list of (N, 2) arrays, font units 26.6 / 64 -> pixels, y up)."""
    contours = []
    current = []

    def move_to(p, _):
        nonlocal current
        if current:
            contours.append(np.array(current))
        current = [(p.x / 64, p.y / 64)]

    def line_to(p, _):
        current.append((p.x / 64, p.y / 64))

    def conic_to(c, p, _):
        x0, y0 = current[-1]
        for k in range(1, steps + 1):
            s = k / steps
            x = (1 - s) ** 2 * x0 + 2 * (1 - s) * s * c.x / 64 + s * s * p.x / 64
            y = (1 - s) ** 2 * y0 + 2 * (1 - s) * s * c.y / 64 + s * s * p.y / 64
            current.append((x, y))

    def cubic_to(c1, c2, p, _):
        x0, y0 = current[-1]
        for k in range(1, steps + 1):
            s = k / steps
            a, b, c, d = (1 - s) ** 3, 3 * (1 - s) ** 2 * s, 3 * (1 - s) * s * s, s ** 3
            current.append((a * x0 + b * c1.x / 64 + c * c2.x / 64 + d * p.x / 64, a * y0 + b * c1.y / 64 + c * c2.y / 64 + d * p.y / 64))

    outline.decompose(None, move_to=move_to, line_to=line_to, conic_to=conic_to, cubic_to=cubic_to)
    if current:
        contours.append(np.array(current))
    return contours


def signed_distance(contours, X, Y):
    """Signed distance (negative inside, nonzero winding) from points (X, Y) to the polylines."""
    best = np.full(X.shape, np.inf)
    winding = np.zeros(X.shape, dtype=int)
    for c in contours:
        a = c
        b = np.roll(c, -1, axis=0)
        for (ax, ay), (bx, by) in zip(a, b):
            dx, dy = bx - ax, by - ay
            L2 = dx * dx + dy * dy
            if L2 == 0:
                continue
            s = np.clip(((X - ax) * dx + (Y - ay) * dy) / L2, 0, 1)
            d = np.hypot(X - (ax + s * dx), Y - (ay + s * dy))
            best = np.minimum(best, d)
            # winding number (crossings of the horizontal ray to +x)
            up = (ay <= Y) & (by > Y)
            down = (ay > Y) & (by <= Y)
            cross = (bx - ax) * (Y - ay) - (X - ax) * (by - ay)
            winding += np.where(up & (cross > 0), 1, 0) - np.where(down & (cross < 0), 1, 0)
    return np.where(winding != 0, -best, best)


def glyph_fields(scene, node, pad=24):
    """Yields (x0, y0, signed distance grid) per glyph of the node, in physical pixels."""
    fonts = scene["fonts"]
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
            face.face.load_glyph(glyph_id, face.flags | freetype.FT_LOAD_NO_BITMAP)
            contours = flatten_outline(face.face.glyph.outline)
            baseline = top + snap(font["ascender"] * ppem / font["unitsPerEm"])
            pen = ox + snap(gx)
            xs = np.concatenate(contours)[:, 0] + pen
            ys = baseline - np.concatenate(contours)[:, 1]
            x0, x1 = int(math.floor(xs.min())) - pad, int(math.ceil(xs.max())) + pad
            y0, y1 = int(math.floor(ys.min())) - pad, int(math.ceil(ys.max())) + pad
            X, Y = np.meshgrid(np.arange(x0, x1) + 0.5 - pen, baseline - (np.arange(y0, y1) + 0.5))
            yield x0, y0, signed_distance(contours, X, Y), (glyph_id, ppem)


def load(run):
    scene = json.load(open(f"{run}/rur/scene.json"))
    studio = np.asarray(Image.open(f"{run}/studio.png").convert("RGB")).astype(float)
    return scene, studio


if __name__ == "__main__":
    run, name = sys.argv[1], sys.argv[2]
    channel = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    scene, studio = load(run)
    node = next(n for n in scene["nodes"] if n.get("name") == name)
    t = node["strokes"][0]["thickness"] * scene["viewport"]["scale"]
    for x0, y0, sd, key in glyph_fields(scene, node):
        a = studio[y0:y0 + sd.shape[0], x0:x0 + sd.shape[1], channel] / 255
        # mid-height horizontal profile
        j = sd.shape[0] // 2
        print(key, "t", t, "row", y0 + j)
        for i in range(sd.shape[1]):
            if a[j, i] > 0 or abs(abs(sd[j, i]) - t) < 1.5:
                print(f"   x={x0 + i:5d} sd={sd[j, i]:8.3f} |sd|-t={abs(sd[j, i]) - t:7.3f} studio={a[j, i] * 255:5.1f}")
