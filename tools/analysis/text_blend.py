"""Fits Studio's text compositing on fixtures/text-probe-blend.luau (or any capture of plain solid-colour
labels whose own background is opaque): per channel samples (text colour c, background d, glyph
coverage g from FreeType, TextTransparency T) -> Studio's value, scored against candidate blend models.

usage: python tools/analysis/text_blend.py <out/compare/name> [--rur dir] [--dump samples.npz]
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


def quantize(c):
    return min(255, math.floor(c * 256))


def samples(compare, rur):
    scene = json.load(open(os.path.join(rur, "scene.json"), encoding="utf8"))
    studio = np.asarray(Image.open(os.path.join(compare, "studio.png")).convert("RGB")).astype(np.int64)
    ours = np.asarray(Image.open(os.path.join(rur, "render.png")).convert("RGB")).astype(np.int64)
    rows = []
    for node in scene["nodes"]:
        text = node.get("text")
        bg = node.get("bg")
        if not text or not text.get("lines") or not bg or bg.get("transparency", 1) > 0 or node.get("gradient") or node.get("strokes"):
            continue
        pieces = node_pieces(scene, node)
        if not pieces:
            continue
        x0 = min(p[0] for p in pieces)
        y0 = min(p[1] for p in pieces)
        x1 = max(p[0] + p[2].shape[1] for p in pieces)
        y1 = max(p[1] + p[2].shape[0] for p in pieces)
        g = np.zeros((y1 - y0, x1 - x0))
        count = np.zeros((y1 - y0, x1 - x0), dtype=int)
        for px, py, bitmap in pieces:
            h, w = bitmap.shape
            region = g[py - y0:py - y0 + h, px - x0:px - x0 + w]
            np.maximum(region, bitmap, out=region)
            count[py - y0:py - y0 + h, px - x0:px - x0 + w] += (bitmap > 0)
        # only pixels a single glyph covers (overlaps are a separate question)
        keep = (count <= 1)
        g8 = np.round(g * 255).astype(np.int64)
        c = np.array([quantize(v) for v in text["color"]])
        d = np.array([quantize(v) for v in bg["color"]])
        T = float(text.get("transparency") or 0)
        S = studio[y0:y1, x0:x1]
        O = ours[y0:y1, x0:x1]
        for ch in range(3):
            sel = keep
            rows.append(np.stack([np.full(sel.sum(), c[ch]), np.full(sel.sum(), d[ch]), g8[sel], np.full(sel.sum(), T * 1e6),
                                  S[..., ch][sel], O[..., ch][sel]], axis=1))
    data = np.concatenate(rows)
    return data[:, 0], data[:, 1], data[:, 2], data[:, 3] / 1e6, data[:, 4], data[:, 5]


def f32(x):
    return np.asarray(x, dtype=np.float32)


def out_round(x):
    return np.floor(x + 0.5)


def out_gpu(x):
    """Float output -> 8 bits the way this GPU converts it (measured by the GPU agent on Chrome's
    output): 12-bit truncation, then round half down."""
    t = np.floor(np.clip(x, 0, 1) * 4096)
    return np.floor((t * 255 + 2047) / 4096)


def main():
    compare = sys.argv[1]
    rur = arg("--rur", os.path.join(compare, "rur"))
    c, d, g, T, s, o = samples(compare, rur)
    print(f"{len(s)} samples; ours exact {100 * (o == s).mean():.3f}%")
    edge = (g > 0) & (g < 255)
    print(f"  edge samples {edge.sum()}; ours exact on edges {100 * (o == s)[edge].mean():.3f}%")

    a = g / 255 * (1 - T)
    af = f32(f32(g) / f32(255)) * f32(1 - f32(T))
    models = {}
    # straight alpha, float blend, one rounding
    models["round(c a + d (1-a))"] = out_round((c * a + d * (1 - a)))
    models["round f32"] = out_round(f32(c) * af + f32(d) * (f32(1) - af))
    # 8-bit alpha (various quantizers), then blend
    for qname, q in {"round": lambda x: np.floor(x * 255 + 0.5), "floor": lambda x: np.floor(x * 255 + 1e-9),
                     "gpu": lambda x: out_gpu(x)}.items():
        A = q(a)
        models[f"a8={qname}; round(c A + d (255-A))/255"] = out_round((c * A + d * (255 - A)) / 255)
        models[f"a8={qname}; gpu out"] = out_gpu((c * A + d * (255 - A)) / 255 / 255)
        # premultiplied source quantized first (Chrome-like)
        for sname, sq in {"round": lambda x: np.floor(x + 0.5), "gpu": lambda x: out_gpu(x / 255)}.items():
            S = sq(c * a) if True else None
            models[f"a8={qname}; s8={sname}(c a); round(s8 + d (255-A)/255)"] = out_round(S + d * (255 - A) / 255)
            models[f"a8={qname}; s8={sname}(c a); gpu(s8 + d (255-A)/255)"] = out_gpu((S + d * (255 - A) / 255) / 255)
    models["gpu(c a + d (1-a))"] = out_gpu((c * a + d * (1 - a)) / 255)
    models["floor(c a + d (1-a))"] = np.floor((c * a + d * (1 - a)) + 1e-9)
    for name, value in sorted(models.items(), key=lambda kv: -(kv[1] == s).mean()):
        e = value - s
        print(f"  {100 * (e == 0).mean():7.3f}%  edges {100 * (e == 0)[edge].mean():7.3f}%  max {np.abs(e).max():3.0f}  {name}")
    if arg("--dump"):
        np.savez(arg("--dump"), c=c, d=d, g=g, T=T, s=s, o=o)


if __name__ == "__main__":
    main()
