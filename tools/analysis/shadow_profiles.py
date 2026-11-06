# Compare UIShadow edge profiles (Studio vs ours) for every frame of a compare run.
import json, sys
import numpy as np
from PIL import Image

run = sys.argv[1]
scale = 1.5
bg = np.array([20, 20, 26], dtype=float)
col = np.array([255, 50, 50], dtype=float)
studio = np.asarray(Image.open(f"{run}/studio.png").convert("RGB"), dtype=float)
ours = np.asarray(Image.open(f"{run}/rur/render.png").convert("RGB"), dtype=float)
report = json.load(open(f"{run}/report.json"))

def coverage(img, x, y):
    # Red channel coverage relative to background (shadow colour red = 255)
    p = img[y, x]
    return (p[0] - bg[0]) / (col[0] - bg[0])

for row in report["layout"]["rows"]:
    x, y, w, h = row["studio"]
    X0, Y0 = round(x * scale), round(y * scale)
    X1, Y1 = round((x + w) * scale), round((y + h) * scale)
    my = (Y0 + Y1) // 2
    mx = (X0 + X1) // 2
    right_s = [coverage(studio, X1 + d, my) for d in range(0, 60, 6)]
    right_o = [coverage(ours, X1 + d, my) for d in range(0, 60, 6)]
    below_s = [coverage(studio, mx, Y1 + d) for d in range(0, 60, 6)]
    below_o = [coverage(ours, mx, Y1 + d) for d in range(0, 60, 6)]
    diag_s = [coverage(studio, X1 + d, Y1 + d) for d in range(0, 42, 6)]
    diag_o = [coverage(ours, X1 + d, Y1 + d) for d in range(0, 42, 6)]
    print(f"{w}x{h}")
    print("  right S", " ".join(f"{v:.3f}" for v in right_s))
    print("  right O", " ".join(f"{v:.3f}" for v in right_o))
    print("  below S", " ".join(f"{v:.3f}" for v in below_s))
    print("  below O", " ".join(f"{v:.3f}" for v in below_o))
    print("  diag  S", " ".join(f"{v:.3f}" for v in diag_s))
    print("  diag  O", " ".join(f"{v:.3f}" for v in diag_o))
