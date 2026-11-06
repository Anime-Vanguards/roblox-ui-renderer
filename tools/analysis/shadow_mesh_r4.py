# UIShadow round 4: an independent Python model of Studio's shadow mesh (native/src/render/coverage.rs shadow_mesh /
# shadowMeshD / shadowAlpha8 / shadowVertexAlpha) scored against saved `rur compare` captures of shadow
# fixtures drawn white on black (fixtures/shadow-probe-*.luau).
#
#   python tools/analysis/shadow_mesh_r4.py out/compare/shadow-probe-g out/compare/shadow-probe-n ...
#       whole-capture score: every shadow of the scene composited over the backdrop, pixels > 0 / > 1 / > 2 off
#   python tools/analysis/shadow_mesh_r4.py --amplitudes out/compare/shadow-probe-n
#       the 8-bit vertex alpha of each bar (the cross-section through the bar's middle, every value it reproduces
#       exactly), how the amplitude table in coverage.rs shadow_gain was measured
#   python tools/analysis/shadow_mesh_r4.py --edge out/compare/shadow-probe-g
#       the straight-edge profile of the BlurRadius 300 square against integer 256-texel tables (dynamic
#       programming: the best integer table and its violations), how the profile texture was identified
#
# The model (measured, see notes/images.md "Round 4: UIShadow"):
#   - profile texel i = floor(255 Phi(-3 + 6 i / 255)), coordinate c = (D + 3) / 6 * 256 - 0.5, bilinear with
#     8-bit weights; alpha byte = round(255 trunc12(texel * va / 255))
#   - mesh: outer contour = rect grown by 3 sigma (corner arcs r + 3 sigma), D = -3; inner contour = rect inset
#     by 3 sigma per axis, at least 0.5 logical px half size, corners max(r - 3 sigma, 0), D = min(3, min half /
#     sigma); arcs of 8 segments (32 above 50 logical px radius); D linear over the triangles
#   - va = round half down of 255 g(hx / sigma) g(hy / sigma) (1 - transparency), g fitted (shadowGain)
import json
import math
import os
import sys

import numpy as np
from PIL import Image

SCALE = 1.5
PROFILE = np.array([math.floor(255 * 0.5 * (1 + math.erf((-3 + 6 * i / 255) / math.sqrt(2)))) for i in range(256)], float)
GAIN = [1.1194847702092372, 0.04884356429770024, -0.053056182552928786, -0.01306252663876636, 0.0200130793956547, -0.0034392315659790887]


def gain(h):
    if h >= 3.25:
        return 1.0
    return math.tanh(sum(c * h ** (i + 1) for i, c in enumerate(GAIN)))


def texel(D):
    """Bilinear profile value (0..1) at D with 8-bit weights."""
    c = np.clip((np.clip(D, -3, 3) + 3) / 6 * 256 - 0.5, 0, 255)
    i = np.clip(np.floor(c).astype(int), 0, 254)
    w = np.floor((c - i) * 256 + 0.5) / 256
    return (PROFILE[i] * (1 - w) + PROFILE[i + 1] * w) / 255


def alpha8(t, va):
    return np.floor(255 * np.floor(t * va / 255 * 4096) / 4096 + 0.5)


def mesh_triangles(x0, y0, x1, y1, sigma, radii):
    """Triangles ((x, y, D) x 3) of the shadow mesh for the device-space rect and corner radii [tl, tr, br, bl]."""
    cx, cy, hx, hy = (x0 + x1) / 2, (y0 + y1) / 2, (x1 - x0) / 2, (y1 - y0) / 2
    k, e = 3 * sigma, 0.5 * SCALE
    ihx, ihy = max(hx - k, e), max(hy - k, e)
    di = min(3.0, min(hx, hy) / sigma)
    outer, inner = [], []
    for idx, (sx, sy) in enumerate([(-1, -1), (1, -1), (1, 1), (-1, 1)]):
        r = max(0.0, min(radii[idx], hx, hy))
        n = 32 if r / SCALE > 50 else 8
        a0 = {0: math.pi, 1: 1.5 * math.pi, 2: 0.0, 3: 0.5 * math.pi}[idx]
        angles = [a0 + j * (math.pi / 2) / n for j in range(n + 1)]
        ro, ri = r + k, max(r - k, 0.0)
        ocx, ocy = cx + sx * (hx - r), cy + sy * (hy - r)
        icx, icy = cx + sx * (ihx - ri), cy + sy * (ihy - ri)
        outer.append([(ocx + ro * math.cos(t), ocy + ro * math.sin(t), -3.0) for t in angles])
        inner.append([(icx + ri * math.cos(t), icy + ri * math.sin(t), di) for t in angles] if ri > 0 else [(icx, icy, di)] * (n + 1))
    tris = []
    for c in range(4):
        o, i = outer[c], inner[c]
        for j in range(len(o) - 1):
            tris += [(o[j], o[j + 1], i[j]), (o[j + 1], i[j + 1], i[j])]
        n = (c + 1) % 4
        tris += [(o[-1], outer[n][0], i[-1]), (outer[n][0], inner[n][0], i[-1])]
    ring = [v for c in range(4) for v in inner[c]]
    for j in range(len(ring)):
        tris.append(((cx, cy, di), ring[j], ring[(j + 1) % len(ring)]))
    return tris, hx, hy


def raster(tris, gx, gy, w, h):
    """D at the pixel centres of [gx, gx + w) x [gy, gy + h) (NaN outside the mesh)."""
    out = np.full((h, w), np.nan)
    for a, b, c in tris:
        area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
        if abs(area) < 1e-9:
            continue
        xs, ys = [a[0], b[0], c[0]], [a[1], b[1], c[1]]
        i0, i1 = max(int(math.floor(min(xs) - gx - 0.5)), 0), min(int(math.ceil(max(xs) - gx + 0.5)), w)
        j0, j1 = max(int(math.floor(min(ys) - gy - 0.5)), 0), min(int(math.ceil(max(ys) - gy + 0.5)), h)
        if i1 <= i0 or j1 <= j0:
            continue
        J, I = np.mgrid[j0:j1, i0:i1]
        px, py = gx + I + 0.5, gy + J + 0.5
        l1 = ((b[0] - px) * (c[1] - py) - (c[0] - px) * (b[1] - py)) / area
        l2 = ((c[0] - px) * (a[1] - py) - (a[0] - px) * (c[1] - py)) / area
        l3 = 1 - l1 - l2
        inside = (l1 >= -1e-9) & (l2 >= -1e-9) & (l3 >= -1e-9)
        sub = out[j0:j1, i0:i1]
        sub[inside] = (l1 * a[2] + l2 * b[2] + l3 * c[2])[inside]
    return out


def snap(v):
    return math.floor(v + 0.5)


def scene_shadows(capture):
    scene = json.load(open(os.path.join(capture, "rur", "scene.json")))
    nodes = scene["nodes"] if isinstance(scene["nodes"], list) else list(scene["nodes"].values())
    return [n for n in nodes if n.get("shadows")]


def render(capture, width, height, background):
    img = np.zeros((height, width, 3)) + background
    for node in scene_shadows(capture):
        x, y, w, h = node["rect"]
        x0, y0, x1, y1 = snap(x * SCALE), snap(y * SCALE), snap((x + w) * SCALE), snap((y + h) * SCALE)
        corner = node.get("corner") or 0
        radii = [corner * SCALE] * 4 if not isinstance(corner, list) else [c * SCALE for c in corner]
        for shadow in sorted(node["shadows"], key=lambda s: s.get("zIndex", 0)):
            sigma = shadow["blur"] * SCALE / 2
            if sigma <= 0:
                continue
            sx, sy = [v * SCALE for v in shadow.get("spread", [0, 0])]
            ox, oy = [v * SCALE for v in shadow.get("offset", [0, 0])]
            grow = min(sx, sy) / 2
            rr = [max(0.0, r + grow) if r > 0 else 0.0 for r in radii]
            X0, X1, Y0, Y1 = x0 - sx / 2 + ox, x1 + sx / 2 + ox, y0 - sy / 2 + oy, y1 + sy / 2 + oy
            tris, hx, hy = mesh_triangles(X0, Y0, X1, Y1, sigma, rr)
            pad = int(math.ceil(3 * sigma + max(rr))) + 3
            gx0, gy0 = max(int(math.floor(X0)) - pad, 0), max(int(math.floor(Y0)) - pad, 0)
            gx1, gy1 = min(int(math.ceil(X1)) + pad, width), min(int(math.ceil(Y1)) + pad, height)
            D = raster(tris, gx0, gy0, gx1 - gx0, gy1 - gy0)
            t = np.where(np.isnan(D), 0.0, texel(np.nan_to_num(D, nan=-3)))
            va = max(0, math.ceil(255 * gain(hx / sigma) * gain(hy / sigma) * (1 - shadow.get("transparency", 0)) - 0.5))
            a = alpha8(t, va)[..., None] / 255
            color = np.floor(np.array(shadow.get("color", [0, 0, 0])) * 255 + 0.5)
            dst = img[gy0:gy1, gx0:gx1]
            img[gy0:gy1, gx0:gx1] = np.floor(color * a + dst * (1 - a) + 0.5)
    return img


def score(capture):
    studio = np.array(Image.open(os.path.join(capture, "studio.png")).convert("RGB")).astype(float)
    report = json.load(open(os.path.join(capture, "report.json")))
    background = (report.get("options") or {}).get("background") or [0, 0, 0]
    model = render(capture, studio.shape[1], studio.shape[0], np.array(background, float))
    e = np.abs(model - studio).max(2)
    print(f"{os.path.basename(capture)}: > 0 {int((e > 0).sum())}, > 1 {int((e > 1).sum())}, > 2 {int((e > 2).sum())}, max {int(e.max())}")


def amplitudes(capture):
    """Bars along x: the vertex alphas that reproduce each bar's middle cross-section exactly."""
    studio = np.array(Image.open(os.path.join(capture, "studio.png")).convert("RGB")).astype(float)[..., 0]
    for node in scene_shadows(capture):
        x, y, w, h = node["rect"]
        shadow = node["shadows"][0]
        sigma = shadow["blur"] * SCALE / 2
        x0, y0, x1, y1 = snap(x * SCALE), snap(y * SCALE), snap((x + w) * SCALE), snap((y + h) * SCALE)
        col = int((x0 + x1) / 2)
        tris, hx, hy = mesh_triangles(x0, y0, x1, y1, sigma, [0, 0, 0, 0])
        gy0, gy1 = max(0, int(y0 - 3 * sigma - 3)), int(y1 + 3 * sigma + 3)
        D = raster(tris, col, gy0, 1, gy1 - gy0)[:, 0]
        t = np.where(np.isnan(D), 0.0, texel(np.nan_to_num(D, nan=-3)))
        v = studio[gy0:gy1, col]
        fits = [va for va in range(1, 256) if np.all(alpha8(t, va) == v)]
        print(f"half height {hy / sigma:.4f} sigma: vertex alpha {fits}, model {math.ceil(255 * gain(hy / sigma) * gain(hx / sigma) - 0.5)}")


def edge(capture, row=1650, x0=45.0, span=1350.0):
    """Best integer 256-texel table for the straight edge of shadow-probe-g (dynamic programming)."""
    studio = np.array(Image.open(os.path.join(capture, "studio.png")).convert("RGB")).astype(float)[..., 0]
    v = studio[row, :1440]
    px = np.arange(1440) + 0.5
    c = (px - x0) / span * 256 - 0.5
    i = np.floor(c).astype(int)
    w = c - i
    vals = np.arange(256.0)
    a, b = vals[:, None], vals[None, :]
    cost, back = np.zeros(256), []
    for k in range(255):
        C = np.zeros((256, 256))
        for wj, vj in zip(w[i == k], v[i == k]):
            C += np.floor(a * (1 - wj) + b * wj + 0.5) != vj
        total = cost[:, None] + C
        back.append(np.argmin(total, axis=0))
        cost = total.min(axis=0)
    seq = [int(np.argmin(cost))]
    for step in reversed(back):
        seq.append(int(step[seq[-1]]))
    seq = np.array(seq[::-1])
    print(f"best integer table: {int(cost.min())} violations; equals floor(255 Phi(-3 + 6 i / 255)): {bool(np.all(seq == PROFILE))}")


if __name__ == "__main__":
    args = sys.argv[1:]
    mode = "score"
    if args and args[0] in ("--amplitudes", "--edge"):
        mode, args = args[0][2:], args[1:]
    for capture in args:
        {"score": score, "amplitudes": amplitudes, "edge": edge}[mode](capture)
