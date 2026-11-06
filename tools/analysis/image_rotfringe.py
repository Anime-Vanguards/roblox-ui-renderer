"""Rotated image fringe analysis (notes/images.md round 6).

For the rotated ImageRect cells of fixtures/image-probe-rotfringe.luau over a flat backdrop: every pixel whose
centre lies just outside the quad's top edge (the texture's top row, soft alpha over an opaque second row) gives
Studio's drawn alpha A = (capture - backdrop) / (tint - backdrop) on the red channel. With the fringe coverage
c = 1 - d (d = distance outside the edge, device px) the sampled alpha is A / c; its position between texel rows
0 and 1 is t = (A / c - row0) / (row1 - row0) (texel units past row 0's centre). Also prints the same for the
pixels just inside the edge (where Studio samples the plain bilinear value).

  python tools/analysis/image_rotfringe.py [compare dir] [--max-y 330]
"""
import json
import sys

import numpy as np
from PIL import Image

args = [a for a in sys.argv[1:] if not a.startswith("--")]
d = args[0] if args else "out/compare/r6-rotfringe"
max_y = float(sys.argv[sys.argv.index("--max-y") + 1]) if "--max-y" in sys.argv else 330
st = np.asarray(Image.open(d + "/studio.png").convert("RGB")).astype(float)
ou = np.asarray(Image.open(d + "/rur/render.png").convert("RGB")).astype(float)
scene = json.load(open(d + "/rur/scene.json"))
S = scene["viewport"]["scale"]
BG = 20.0
H, W = st.shape[:2]
YY, XX = np.mgrid[0:H, 0:W]
tex_cache = {}


def texture(uri):
    if uri not in tex_cache:
        name = uri.split("//")[-1]
        tex_cache[uri] = np.asarray(Image.open(f"{d}/rur/assets/{name}.png")).astype(float)
    return tex_cache[uri]


for n in scene["nodes"]:
    img = n.get("image")
    if not img or not n.get("matrix") or img.get("rectSize", [0, 0])[0] == 0:
        continue
    x, y, w, h = n["rect"]
    m = n["matrix"]
    if max(m[1] * a + m[3] * b + m[5] for a in (x, x + w) for b in (y, y + h)) * S > max_y:
        continue
    tex = texture(img["uri"])
    A_ = tex[..., -1]
    ox, oy = img["rectOffset"]
    rw, rh = img["rectSize"]
    det = m[0] * m[3] - m[1] * m[2]
    px = (XX + 0.5) / S - m[4]
    py = (YY + 0.5) / S - m[5]
    lx = (m[3] * px - m[2] * py) / det
    ly = (-m[1] * px + m[0] * py) / det
    u = ox + (lx - x) / w * rw
    v = oy + (ly - y) / h * rh
    dist = (y - ly) * S  # device px above the top edge
    ppt = h * S / rh
    rows = {"out": [], "in": []}
    sel = (u > ox + 3) & (u < ox + rw - 3) & (np.abs(dist) < 1)
    for j, i in zip(*np.nonzero(sel)):
        U = u[j, i] - 0.5
        k = int(np.floor(U))
        f = U - k
        r0 = A_[0, k] * (1 - f) + A_[0, k + 1] * f
        r1 = A_[1, k] * (1 - f) + A_[1, k + 1] * f
        if r1 - r0 < 80:
            continue
        a = (st[j, i, 0] - BG) / (255 - BG) * 255
        ao = (ou[j, i, 0] - BG) / (255 - BG) * 255
        dd = dist[j, i]
        if dd > 0.05:
            c = 1 - dd
            rows["out"].append((dd, (a / c - r0) / (r1 - r0), (ao / c - r0) / (r1 - r0)))
        elif dd < -0.05:
            # inside: bilinear at v (texel units), sample row position vt - 0.5 (clamped at 0)
            vt = max(0.0, v[j, i] - 0.5)
            rows["in"].append((dd, (a - r0) / (r1 - r0) - vt, (ao - r0) / (r1 - r0) - vt))
    rot = np.degrees(np.arctan2(m[1], m[0]))
    for side in ("out", "in"):
        r = np.array(rows[side])
        if len(r):
            print(f"{n['name']:9s} {ppt:.3f} px/texel rot {rot:6.1f} {side:3s} n {len(r):3d}  studio t {np.median(r[:, 1]):+.4f} "
                  f"(texel) = {np.median(r[:, 1]) * ppt:+.4f} px   ours {np.median(r[:, 2]):+.4f}")
