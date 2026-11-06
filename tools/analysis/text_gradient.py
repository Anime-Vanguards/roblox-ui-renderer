"""Text UIGradient model check on fixtures/text-probe-gradient*.luau (and any capture of gradient text).

Measured model (notes/text.md): each glyph quad (the glyph bitmap grown by 1 px on every side) is cut
along the lines where the gradient parameter t equals a keypoint time (0 and 1 included); every piece's
vertices take the gradient at that point (colour round(255 * C * TextColor3), alpha
round((1 - transparency) * Av)), the pieces are triangulated and the GPU interpolates across each
triangle; the fragment colour is converted to 8 bits (gpu8) and blended glyph by glyph.

usage: python tools/analysis/text_gradient.py <out/compare/name> [--rur dir] [--nodes a,b] [--split tr-bl|tl-br]
         [--fan first|ear]
"""

import json
import math
import os
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from text_subpixel import node_pieces  # noqa: E402


def arg(name, default=None):
    if name in sys.argv:
        return sys.argv[sys.argv.index(name) + 1]
    return default


def gpu8(v):
    """0..1 float -> 8 bits as this GPU converts shader outputs: 12-bit truncation, round half down."""
    t = np.floor(np.clip(v, 0, 1) * 4096)
    return np.floor((t * 255 + 2047) / 4096)


def quantize(c):
    return min(255, math.floor(c * 256))


def snap(v):
    return math.floor(v + 0.5)


def sample_color(seq, t):
    if t <= seq[0][0]:
        return np.array(seq[0][1:4])
    for a, b in zip(seq, seq[1:]):
        if t <= b[0]:
            f = 0 if b[0] == a[0] else (t - a[0]) / (b[0] - a[0])
            return np.array(a[1:4]) + (np.array(b[1:4]) - np.array(a[1:4])) * f
    return np.array(seq[-1][1:4])


def sample_number(seq, t):
    if not seq:
        return 0.0
    if t <= seq[0][0]:
        return seq[0][1]
    for a, b in zip(seq, seq[1:]):
        if t <= b[0]:
            f = 0 if b[0] == a[0] else (t - a[0]) / (b[0] - a[0])
            return a[1] + (b[1] - a[1]) * f
    return seq[-1][1]


def gradient_plane(gradient, rect):
    """(a, b, k) with t = a * x + b * y + k over device px, like the renderer's text gradients (native/src/render/gradients.rs): the 0..1
    span is the chord through the offset centre along the rotation."""
    f32 = lambda v: float(np.float32(v))
    x, y, w, h = rect
    angle = math.radians(gradient.get("rotation") or 0)
    dx, dy = math.cos(angle), math.sin(angle)
    ox, oy = gradient.get("offset") or [0, 0]
    # float32 like the engine: Offset.Y 0.1 * 210 is 21.0 exactly there (21.0000003 in double)
    cx, cy = f32(x + f32(w / 2) + f32(f32(ox) * w)), f32(y + f32(h / 2) + f32(f32(oy) * h))
    span = min(w / abs(dx) if abs(dx) > 1e-9 else math.inf, h / abs(dy) if abs(dy) > 1e-9 else math.inf)
    return dx / span, dy / span, -(cx * dx + cy * dy) / span + 0.5


def clip_polygon(poly, a, b, c, keep_ge):
    """Part of the convex polygon where a*x + b*y + c >= 0 (keep_ge) or <= 0, Sutherland-Hodgman along
    the input order; when the line cuts the polygon the result starts at the point where the walk
    re-enters the kept side (measured: the triangle fan of a cut piece starts there)."""
    out = []
    enter = None
    for i in range(len(poly)):
        P, Q = poly[i], poly[(i + 1) % len(poly)]
        fp, fq = a * P[0] + b * P[1] + c, a * Q[0] + b * Q[1] + c
        if not keep_ge:
            fp, fq = -fp, -fq
        if fp >= -1e-9:
            out.append(P)
        if (fp >= -1e-9) != (fq >= -1e-9):
            s = fp / (fp - fq)
            if fq >= -1e-9:
                enter = len(out)
            out.append((P[0] + (Q[0] - P[0]) * s, P[1] + (Q[1] - P[1]) * s))
    if enter is not None:
        out = out[enter:] + out[:enter]
    return out


def triangles(poly, split, fan):
    """Triangulation of a single polygon: an axis-aligned rectangle along the given diagonal, others as a fan."""
    xs, ys = [p[0] for p in poly], [p[1] for p in poly]
    if len(poly) == 4 and len(set(round(v, 9) for v in xs)) == 2 and len(set(round(v, 9) for v in ys)) == 2:
        L, R, T, B = min(xs), max(xs), min(ys), max(ys)
        if split == "tl-br":
            return [((L, T), (R, T), (R, B)), ((L, T), (R, B), (L, B))]
        return [((L, T), (R, T), (L, B)), ((R, T), (R, B), (L, B))]
    start = 0
    if fan == "min":
        start = min(range(len(poly)), key=lambda i: (poly[i][1], poly[i][0]))
    ordered = poly[start:] + poly[:start]
    return [(ordered[0], ordered[i], ordered[i + 1]) for i in range(1, len(ordered) - 1)]


def glyph_draws(scene, node, fill_color, alpha8, gradient, rect, pieces_of, split="tl-br", fan="first", variant_t="double"):
    """Per glyph quad: (x, y, bitmap 0..1, [(triangle, colours, alphas)])."""
    draws = []
    times = []
    if gradient:
        a, b, k = gradient_plane(gradient, rect)
        times = sorted(set([0.0, 1.0] + [kp[0] for kp in gradient["color"]] + [kp[0] for kp in (gradient.get("transparency") or [])]))

    f32 = np.float32
    if gradient:
        x_, y_, w_, h_ = rect
        angle = math.radians(gradient.get("rotation") or 0)
        gdx, gdy = f32(math.cos(angle)), f32(math.sin(angle))
        ox, oy = gradient.get("offset") or [0, 0]
        gcx = f32(f32(x_) + f32(f32(w_) / f32(2)) + f32(f32(ox) * f32(w_)))
        gcy = f32(f32(y_) + f32(f32(h_) / f32(2)) + f32(f32(oy) * f32(h_)))
        span_x = f32(w_) / abs(gdx) if abs(gdx) > 1e-6 else f32(np.inf)
        span_y = f32(h_) / abs(gdy) if abs(gdy) > 1e-6 else f32(np.inf)
        gspan = f32(min(span_x, span_y))

    def vertex(x, y):
        if not gradient:
            return fill_color, alpha8
        if variant_t == "f32":
            # float32 like Studio: ties such as 93.5 go the way (p - centre) . d / span + 0.5 rounds
            dot = f32(f32(f32(f32(x) - gcx) * gdx) + f32(f32(f32(y) - gcy) * gdy))
            t = float(f32(f32(dot / gspan) + f32(0.5)))
        else:
            t = a * x + b * y + k
        t = min(1.0, max(0.0, t))
        color = sample_color(gradient["color"], t)
        transparency = sample_number(gradient.get("transparency") or [], t)
        return np.floor(color * fill_color + 0.5), math.floor((1 - transparency) * alpha8 + 0.5)

    for px, py, bitmap in pieces_of:
        h, w = bitmap.shape
        quad = [(px - 1, py - 1), (px + w + 1, py - 1), (px + w + 1, py + h + 1), (px - 1, py + h + 1)]
        polys = [quad]
        if gradient:
            polys = []
            bounds = [-math.inf] + times + [math.inf]
            for lo, hi in zip(bounds, bounds[1:]):
                poly = quad
                if lo > -math.inf:
                    poly = clip_polygon(poly, a, b, k - lo, True)
                if hi < math.inf and len(poly) >= 3:
                    poly = clip_polygon(poly, a, b, k - hi, False)
                if len(poly) >= 3:
                    polys.append(poly)
        tris = []
        for poly in polys:
            for tri in triangles(poly, split, fan):
                tris.append((tri, [vertex(*p) for p in tri]))
        draws.append((px, py, bitmap, tris))
    return draws


def composite(canvas, x0, y0, draws):
    """Blends each glyph quad over the canvas (origin x0, y0) with Studio's arithmetic."""
    for px, py, bitmap, tris in draws:
        h, w = bitmap.shape
        for j in range(h):
            for i in range(w):
                g = bitmap[j, i]
                if g <= 0:
                    continue
                X, Y = px + i + 0.5, py + j + 0.5
                for (P, Q, R), ((cp, ap), (cq, aq), (cr, ar)) in tris:
                    det = (Q[1] - R[1]) * (P[0] - R[0]) + (R[0] - Q[0]) * (P[1] - R[1])
                    if abs(det) < 1e-12:
                        continue
                    l1 = ((Q[1] - R[1]) * (X - R[0]) + (R[0] - Q[0]) * (Y - R[1])) / det
                    l2 = ((R[1] - P[1]) * (X - R[0]) + (P[0] - R[0]) * (Y - R[1])) / det
                    l3 = 1 - l1 - l2
                    if l1 >= -1e-9 and l2 >= -1e-9 and l3 >= -1e-9:
                        rgb = np.asarray(cp) * l1 + np.asarray(cq) * l2 + np.asarray(cr) * l3
                        alpha = (ap * l1 + aq * l2 + ar * l3) / 255
                        src = gpu8(rgb / 255)
                        A = gpu8(g * alpha)
                        D = canvas[py + j - y0, px + i - x0]
                        canvas[py + j - y0, px + i - x0] = np.floor((src * A + D * (255 - A)) / 255 + 0.5)
                        break


def main():
    compare = sys.argv[1]
    rur = arg("--rur", os.path.join(compare, "rur"))
    scene = json.load(open(os.path.join(rur, "scene.json"), encoding="utf8"))
    studio = np.asarray(Image.open(os.path.join(compare, "studio.png")).convert("RGB")).astype(float)
    background = np.array([quantize(v) for v in (scene.get("background") or [0, 0, 0])], dtype=float)
    only = arg("--nodes")
    split = arg("--split", "tl-br")
    fan = arg("--fan", "first")
    S = scene["viewport"]["scale"]
    for node in scene["nodes"]:
        if not node.get("text") or not node["text"].get("lines") or not node.get("gradient") or node.get("strokes"):
            continue
        if only and node.get("name") not in only.split(","):
            continue
        text = node["text"]
        rect = [v * S for v in node["rect"]]
        color = np.array([quantize(v) for v in text["color"]], dtype=float)
        Av = min(255, math.floor((1 - (text.get("transparency") or 0)) * 256))
        pieces = [(px, py, bm) for px, py, bm in node_pieces(scene, node)]
        draws = glyph_draws(scene, node, color, Av, node["gradient"], rect, pieces, split, fan, arg("--t", "double"))
        x0 = min(p[0] for p in pieces) - 2
        y0 = min(p[1] for p in pieces) - 2
        x1 = max(p[0] + p[2].shape[1] for p in pieces) + 2
        y1 = max(p[1] + p[2].shape[0] for p in pieces) + 2
        canvas = np.zeros((y1 - y0, x1 - x0, 3)) + background
        composite(canvas, x0, y0, draws)
        mask = np.zeros(canvas.shape[:2], dtype=bool)
        for px, py, bm in pieces:
            mask[py - y0:py - y0 + bm.shape[0], px - x0:px - x0 + bm.shape[1]] |= bm > 0
        target = studio[y0:y1, x0:x1]
        ok = (canvas == target).all(axis=2)
        print(f"{node.get('name', '?'):10s} {int(ok[mask].sum())}/{int(mask.sum())} = {100 * ok[mask].mean():.3f}%")


if __name__ == "__main__":
    main()
