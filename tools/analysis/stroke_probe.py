# Load a border-stroke probe capture (white strokes on black) and expose per-frame alpha maps.
#   python stroke_probe.py <out/compare/run> [name-prefix]   prints edge profiles per frame
import json
import math
import sys

import numpy as np
from PIL import Image

SCALE = 1.5


def snap(v):
    return math.floor(v + 0.5)


def load(run, source="studio"):
    path = f"{run}/studio.png" if source == "studio" else f"{run}/rur/render.png"
    img = np.asarray(Image.open(path).convert("RGB"), dtype=float)
    layout = json.load(open(f"{run}/layout-full.json"))
    scene = json.load(open(f"{run}/rur/scene.json"))
    by_name = {node["name"]: node for node in scene["nodes"]}
    frames = []
    for row in layout["rows"]:
        if row.get("missing"):
            continue
        x, y, w, h = row["studio"]
        name = row["path"].split("/")[-1]
        node = by_name.get(name)
        frames.append(dict(
            name=name, x=x, y=y, w=w, h=h,
            X0=snap(x * SCALE), Y0=snap(y * SCALE), X1=snap((x + w) * SCALE), Y1=snap((y + h) * SCALE),
            fx0=x * SCALE, fy0=y * SCALE, fx1=(x + w) * SCALE, fy1=(y + h) * SCALE,
            node=node, stroke=(node or {}).get("strokes", [None])[0], corner=(node or {}).get("corner"),
        ))
    frames.sort(key=lambda f: f["name"])
    # alpha = red channel / 255 (white stroke on black)
    return img[:, :, 0] / 255.0, frames


def profiles(alpha, f, reach=24):
    """Edge profiles through the middle of each side, ordered from outside to inside."""
    cx = (f["X0"] + f["X1"]) // 2
    cy = (f["Y0"] + f["Y1"]) // 2
    top = [(f["Y0"] - reach + i, alpha[f["Y0"] - reach + i, cx]) for i in range(2 * reach)]
    bottom = [(f["Y1"] + reach - 1 - i, alpha[f["Y1"] + reach - 1 - i, cx]) for i in range(2 * reach)]
    left = [(f["X0"] - reach + i, alpha[cy, f["X0"] - reach + i]) for i in range(2 * reach)]
    right = [(f["X1"] + reach - 1 - i, alpha[cy, f["X1"] + reach - 1 - i]) for i in range(2 * reach)]
    return dict(top=top, bottom=bottom, left=left, right=right)


def fmt(profile):
    return " ".join(f"{v * 255:3.0f}" for _, v in profile)


if __name__ == "__main__":
    run = sys.argv[1]
    prefix = sys.argv[2] if len(sys.argv) > 2 else ""
    alpha, frames = load(run)
    for f in frames:
        if not f["name"].startswith(prefix):
            continue
        s = f["stroke"] or {}
        print(f"{f['name']:10s} t={s.get('thickness')} {s.get('position')} {s.get('joins')} r={f['corner']} rect=({f['fx0']:.3f},{f['fy0']:.3f})-({f['fx1']:.3f},{f['fy1']:.3f})")
        for side, prof in profiles(alpha, f, 14).items():
            print(f"   {side:6s} {fmt(prof)}")
