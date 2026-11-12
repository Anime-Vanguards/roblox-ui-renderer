# The mip level of turned images, read from a compare of fixtures/image-probe-rotlod.luau.
#
#   python tools/analysis/image_rotlod.py out/compare/image-probe-rotlod [--rur] [--fit]
#
# The fixture's texture is 16 x 16 with one opaque white row over clear ones. At mip level k its edge texel is
# 2^-k opaque and as bright (straight alpha is filtered channel by channel), so a pixel whose centre lies in the
# first half texel inside the top edge shows (edge value)^2 over the black backdrop, and the level follows from
# its red channel alone. Per image the script prints the level Studio (or, with --rur, the render) shows, and
# what the renderer's rule gives (native/src/render/sampling.rs footprint_length). --fit scans the rule's two
# constants over all readings.
import json
import math
import sys

import numpy as np
from PIL import Image

TEXELS = 16


def level_of(value):
    """The mip level whose edge texel, squared, is `value` (0 .. 1)."""
    edge = math.sqrt(value)
    if edge >= 1:
        return 0.0
    k = max(0, min(3, math.floor(-math.log2(max(edge, 1e-3)))))
    high, low = 2.0 ** -k, 2.0 ** -(k + 1)
    return k + (high - edge) / (high - low)


def length(a, b, k):
    return max(abs(a), abs(b)) + k * min(abs(a), abs(b))


def footprint(su, sv, angle, k=11 / 32, q=11 / 16):
    """Texels a pixel spans, for an image of su x sv texels a pixel along its own axes, turned by angle."""
    c, s = math.cos(angle), math.sin(angle)
    across, down = (su * c, -sv * s), (su * s, sv * c)
    plus = (across[0] + down[0], across[1] + down[1])
    minus = (across[0] - down[0], across[1] - down[1])
    return max(length(*across, k), length(*down, k), q * length(*plus, k), q * length(*minus, k))


def readings(directory, rendered):
    image = np.asarray(Image.open(f"{directory}/{'rur/render.png' if rendered else 'studio.png'}").convert("RGB")).astype(float)
    scene = json.load(open(f"{directory}/rur/scene.json"))
    scale = scene.get("viewport", {}).get("scale", 1.5)
    out = []
    for node in scene["nodes"]:
        if not node.get("image") or not node.get("matrix"):
            continue
        x, y, w, h = (v * scale for v in node["rect"])
        m = node["matrix"]
        cos, sin = m[0], m[1]
        lx0, ly0 = node["rect"][0] + node["rect"][2] / 2, node["rect"][1] + node["rect"][3] / 2
        cx, cy = (m[0] * lx0 + m[2] * ly0 + m[4]) * scale, (m[1] * lx0 + m[3] * ly0 + m[5]) * scale
        su, sv = TEXELS / w, TEXELS / h
        reach = int(max(w, h) / 2 + 4)
        levels = []
        for py in range(max(0, int(cy) - reach), min(image.shape[0], int(cy) + reach + 1)):
            for px in range(max(0, int(cx) - reach), min(image.shape[1], int(cx) + reach + 1)):
                dx, dy = px + 0.5 - cx, py + 0.5 - cy
                lx, ly = cos * dx + sin * dy, -sin * dx + cos * dy
                if abs(lx) > w / 2 - min(6, w / 4):
                    continue
                inside = ly + h / 2
                if 0.02 < inside < 0.5 / sv - 0.02:
                    levels.append(level_of(image[py, px, 0] / 255))
        if levels:
            out.append((node["path"].split(".")[-1], su, sv, math.atan2(sin, cos), float(np.mean(levels)), float(np.std(levels)), len(levels)))
    return out


def main():
    arguments = [a for a in sys.argv[1:] if not a.startswith("--")]
    if not arguments:
        raise SystemExit(__doc__ or "usage: image_rotlod.py <compare dir> [--rur] [--fit]")
    rows = readings(arguments[0], "--rur" in sys.argv)
    worst = 0.0
    for name, su, sv, angle, level, spread, count in rows:
        rule = max(0.0, math.log2(footprint(su, sv, angle)))
        usable = level > 0.03 and spread < 0.01
        if usable:
            worst = max(worst, abs(rule - level))
        print(f"{name:16s} texels/px {su:.3f} x {sv:.3f}  level {level:.3f} (sd {spread:.3f}, {count} px)  rule {rule:.3f}{'' if usable else '  (not used)'}")
    print(f"{len(rows)} images; the rule's largest miss on the usable readings: {worst:.4f} of a level")
    if "--fit" in sys.argv:
        data = [(su, sv, angle, level) for _, su, sv, angle, level, spread, _ in rows if level > 0.03 and spread < 0.01]
        scored = []
        for k in np.arange(0.30, 0.42, 0.005):
            for q in np.arange(0.66, 0.72, 0.0025):
                miss = [math.log2(footprint(su, sv, angle, k, q)) - level for su, sv, angle, level in data]
                scored.append((max(abs(v) for v in miss), float(np.sqrt(np.mean(np.square(miss)))), round(float(k), 4), round(float(q), 4)))
        for worst, rms, k, q in sorted(scored)[:5]:
            print(f"shorter side x {k}, diagonals x {q}: largest miss {worst:.4f}, rms {rms:.4f}")


if __name__ == "__main__":
    main()
