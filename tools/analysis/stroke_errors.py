# Per-node breakdown of pixel mismatches (max channel diff > 2) between Studio and the offline render.
#   python stroke_errors.py <out/compare/run> [pad]
import json
import math
import sys

import numpy as np
from PIL import Image

SCALE = 1.5


def snap(v):
    return math.floor(v + 0.5)


run = sys.argv[1]
pad = int(sys.argv[2]) if len(sys.argv) > 2 else 20
st = np.asarray(Image.open(f"{run}/studio.png").convert("RGB")).astype(int)
ou = np.asarray(Image.open(f"{run}/rur/render.png").convert("RGB")).astype(int)
h = min(st.shape[0], ou.shape[0])
w = min(st.shape[1], ou.shape[1])
diff = np.abs(st[:h, :w] - ou[:h, :w]).max(axis=2)
bad = diff > 2
print(f"total bad {bad.sum()} of {h * w} ({100 * (1 - bad.sum() / (h * w)):.3f}% exact)")
layout = json.load(open(f"{run}/layout-full.json"))
claimed = np.zeros_like(bad)
rows = []
for row in layout["rows"]:
    if row.get("missing"):
        continue
    x, y, rw, rh = row["studio"]
    X0, Y0, X1, Y1 = snap(x * SCALE) - pad, snap(y * SCALE) - pad, snap((x + rw) * SCALE) + pad, snap((y + rh) * SCALE) + pad
    X0, Y0 = max(0, X0), max(0, Y0)
    region = bad[Y0:Y1, X0:X1] & ~claimed[Y0:Y1, X0:X1]
    n = int(region.sum())
    claimed[Y0:Y1, X0:X1] = True
    big = int((diff[Y0:Y1, X0:X1] > 32).sum())
    rows.append((n, big, row["path"]))
for n, big, path in sorted(rows, reverse=True):
    if n:
        print(f"{n:6d} bad {big:6d} >32  {path}")
print("unclaimed", int((bad & ~claimed).sum()))
