# Border UIStroke coverage model fitted against Studio captures (fixtures/stroke-*.luau).
#   python stroke_model.py <out/compare/run> [name-prefix] [-v]    per-frame mismatch counts
#
# Everything is in physical pixels, relative to the snapped frame rect [X0, X1) x [Y0, Y1).
# D = outward extent of the stroke, I = inward extent (Outer: t, 0; Center: t/2, t/2; Inner: 0, t).
import json
import math
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from stroke_probe import load  # noqa: E402

SCALE = 1.5
PARAMS = dict(
    ring_margin=0.15,  # ring geometry extends this far past the outer boundary
    clip_margin=0.15,  # clip-mode geometry margin for thick strokes
    clip_margin_min_d=3.0,  # below this outward extent the clip geometry has no margin
)


def smoothstep(x):
    x = np.clip(x, 0, 1)
    return x * x * (3 - 2 * x)


def coarse_fwidth(d, ox, oy):
    """fwidth with 2x2-quad coarse derivatives (quads aligned to even pixel coordinates);
    d[j, i] is pixel (ox + i, oy + j). The grid must start and end on quad boundaries."""
    H, W = d.shape
    assert ox % 2 == 0 and oy % 2 == 0 and H % 2 == 0 and W % 2 == 0
    tl = d[0::2, 0::2]
    tr = d[0::2, 1::2]
    bl = d[1::2, 0::2]
    fw = np.abs(tr - tl) + np.abs(bl - tl)
    return np.repeat(np.repeat(fw, 2, axis=0), 2, axis=1)


def sd_round_box(px, py, hx, hy, r):
    qx = np.abs(px) - hx + r
    qy = np.abs(py) - hy + r
    return np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r


def extents(t, position):
    if position == "Center":
        return t / 2, t / 2
    if position == "Inner":
        return 0.0, t
    return t, 0.0


def stroke_alpha(X0, Y0, X1, Y1, radius, t, position, joins, ox, oy, W, H, scale=SCALE, params=PARAMS):
    """Coverage of a border stroke on the pixel grid [ox, ox + W) x [oy, oy + H).
    radius: scalar or [tl, tr, br, bl] (physical px)."""
    D, I = extents(t, position)
    cx, cy = (X0 + X1) / 2, (Y0 + Y1) / 2
    hw, hh = (X1 - X0) / 2, (Y1 - Y0) / 2
    radii = radius if isinstance(radius, (list, tuple)) else [radius] * 4
    radii = [max(0.0, min(v, hw, hh)) for v in radii]
    xs = np.arange(ox, ox + W) + 0.5
    ys = np.arange(oy, oy + H) + 0.5
    ax, ay = np.meshgrid(xs, ys)
    px, py = ax - cx, ay - cy
    # Each quadrant is drawn on its own with its corner's radius (and render mode)
    left, top = px < 0, py < 0
    out = np.zeros_like(px)
    for rq, mask in ((radii[0], left & top), (radii[1], ~left & top), (radii[2], ~left & ~top), (radii[3], left & ~top)):
        if mask.any():
            a = quadrant_alpha(rq, t, D, I, X0, Y0, X1, Y1, hw, hh, ax, ay, px, py, position, joins, ox, oy, scale, params)
            out = np.where(mask, a, out)
    return out


def quadrant_alpha(r, t, D, I, X0, Y0, X1, Y1, hw, hh, ax, ay, px, py, position, joins, ox, oy, scale, params):
    rounded_join = joins not in ("Miter", "Bevel")
    sharp = r == 0
    # The join shapes the contour the stroke grows away from the frame edge: the outer one for
    # Outer/Center strokes, the inner one for Inner strokes (sharp inner corners -> clip mode).
    ring = r - I > 0 and (rounded_join or position != "Inner")
    # Distances inside the frame from the nearest vertical / horizontal edge
    u = np.minimum(ax - X0, X1 - ax)
    v = np.minimum(ay - Y0, Y1 - ay)
    # Bevel legs measured from the stroke's outer corner: D on sharp Center strokes, t otherwise
    legs = D if sharp and position == "Center" else t

    def outer_geometry(m):
        inside = (ax >= X0 - D - m) & (ax < X1 + D + m) & (ay >= Y0 - D - m) & (ay < Y1 + D + m)
        if joins == "Bevel" and D > 0:
            # Corner cut by the diagonal u + v = legs - 2D, pushed out by the margin; the left
            # corners' diagonals are left edges (pixel centres on them are inside)
            s = u + v + 2 * D - legs + m * math.sqrt(2)
            inside &= (s > 0) | ((s == 0) & (px < 0))
        return inside

    if ring:
        # Ring mode (UI shader RcDesc.w > -9999): linear ramps on both contours, distances to the
        # core box scaled by the DPI factor; the quad reaches a little past the outer contour.
        # Miter/Bevel joins keep a hard (geometric) outer contour.
        d = sd_round_box(px, py, hw - r, hh - r, 0)
        a = np.clip(scale * (d - (r - I) + 0.5), 0, 1)
        if rounded_join:
            a = a * np.clip(scale * (r + D + 0.5 - d), 0, 1)
        inside = outer_geometry(params["ring_margin"])
        return np.where(inside, a, 0.0)

    # Clip mode: outer contour anti-aliased like UICorner (smoothstep of d / fwidth), hole cut by
    # geometry (pixel centres, top-left rule; flipped for Inner strokes on sharp frames).
    m = params["clip_margin"]
    if not rounded_join and D > 0:
        # Miter/Bevel outer contour: hard geometry
        a = np.ones_like(px)
        if not (sharp and joins == "Bevel" and position == "Center"):
            m = 0.0
    elif sharp and D == 0:
        a = np.ones_like(px)
        m = 0.0
    else:
        R = r + D + 0.5
        d = sd_round_box(px, py, hw + D, hh + D, R)
        a = smoothstep(0.5 - d / np.maximum(coarse_fwidth(d, ox, oy), 1e-9))
        if sharp and D <= params["clip_margin_min_d"]:
            m = 0.0
    inside = outer_geometry(m)
    if position == "Inner" and sharp:
        hole = (ax > X0 + I) & (ax <= X1 - I) & (ay > Y0 + I) & (ay <= Y1 - I)
    else:
        hole = (ax >= X0 + I) & (ax < X1 - I) & (ay >= Y0 + I) & (ay < Y1 - I)
        if joins == "Bevel" and position == "Inner":
            # Rounded frames: the hole's corners are bevelled with legs I (diagonals of the left
            # corners are the hole's left edges, so centres on them belong to the hole)
            s = u + v - 3 * I
            hole &= (s > 0) | ((s == 0) & (px < 0))
    return np.where(inside & ~hole, a, 0.0)


def window(f, pad):
    ox = (f["X0"] - pad) // 2 * 2
    oy = (f["Y0"] - pad) // 2 * 2
    W = (f["X1"] + pad - ox + 1) // 2 * 2
    H = (f["Y1"] + pad - oy + 1) // 2 * 2
    return max(0, ox), max(0, oy), W, H


def predict(alpha, frames, pad=26, params=PARAMS):
    """Composite every frame's stroke (white over black) into a full-canvas prediction."""
    canvas = np.zeros_like(alpha)
    for f in frames:
        if not f["stroke"]:
            continue
        s = f["stroke"]
        ox, oy, W, H = window(f, pad)
        W = min(W, (canvas.shape[1] - ox) // 2 * 2)
        H = min(H, (canvas.shape[0] - oy) // 2 * 2)
        corner = f["corner"] or 0
        radius = [v * SCALE for v in corner] if isinstance(corner, list) else corner * SCALE
        a = stroke_alpha(f["X0"], f["Y0"], f["X1"], f["Y1"], radius, s["thickness"] * SCALE, s["position"], s["joins"], ox, oy, W, H, params=params)
        a = a * (1 - (s.get("transparency") or 0))
        region = canvas[oy:oy + H, ox:ox + W]
        canvas[oy:oy + H, ox:ox + W] = region + a * (1 - region)
    return canvas


def evaluate(run, prefix="", verbose=False, pad=26, params=PARAMS):
    alpha, frames = load(run)
    pred = np.floor(predict(alpha, frames, pad, params) * 255 + 1e-6)
    diff = np.abs(pred - np.round(alpha * 255))
    bad_all = diff > 2
    total_bad = 0
    for f in frames:
        if not f["name"].startswith(prefix) or not f["stroke"]:
            continue
        s = f["stroke"]
        ox, oy, W, H = window(f, pad)
        bad = bad_all[oy:oy + H, ox:ox + W]
        total_bad += int(bad.sum())
        if verbose and bad.sum():
            t = s["thickness"] * SCALE
            print(f"{f['name']:8s} t={t:7.4f} {s['position']:6s} {s['joins']:5s} r={f['corner']} bad={int(bad.sum()):4d} max={diff[oy:oy + H, ox:ox + W].max():4.0f}")
            ys, xs = np.nonzero(bad)
            for y, x in list(zip(ys, xs))[:6]:
                print(f"      ({x + ox},{y + oy}) rel({x + ox - f['X0']},{y + oy - f['Y0']}) studio={alpha[y + oy, x + ox] * 255:.0f} model={pred[y + oy, x + ox]:.0f}")
    print(f"{run}: bad {total_bad} (whole canvas {int(bad_all.sum())} of {bad_all.size})")
    return total_bad


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    evaluate(args[0], args[1] if len(args) > 1 else "", "-v" in sys.argv)
