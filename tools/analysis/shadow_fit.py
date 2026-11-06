# Fit UIShadow models against a Studio capture of fixtures/shadows-sizes.luau.
import json, sys, math
import numpy as np
from PIL import Image
from scipy.special import erf
from scipy.ndimage import gaussian_filter
from scipy.optimize import minimize

run = "out/compare/shadows-sizes"
scale = 1.5
bg = np.array([20, 20, 26], dtype=float)
studio = np.asarray(Image.open(f"{run}/studio.png").convert("RGB"), dtype=float)
report = json.load(open(f"{run}/report.json"))
M = 80

cells = []
for row in report["layout"]["rows"]:
    x, y, w, h = row["studio"]
    X0, Y0 = round(x * scale), round(y * scale)
    X1, Y1 = round((x + w) * scale), round((y + h) * scale)
    ys, xs = np.mgrid[Y0 - M:Y1 + M, X0 - M:X1 + M]
    alpha = (studio[Y0 - M:Y1 + M, X0 - M:X1 + M, 0] - bg[0]) / (255 - bg[0])
    inside = (xs >= X0) & (xs < X1) & (ys >= Y0) & (ys < Y1)
    cells.append(dict(name=f"{w}x{h}", W=X1 - X0, H=Y1 - Y0, px=xs + 0.5 - X0, py=ys + 0.5 - Y0, alpha=alpha, mask=~inside))

def erf7(x):
    x = np.clip(x, -10, 10) * (2 / math.sqrt(math.pi))
    xx = x * x
    x = x + (0.24295 + (0.03395 + 0.0104 * xx) * xx) * (x * xx)
    return x / np.sqrt(1 + x * x)

def levien(px, py, width, height, radius, s, erffn=erf7, k115=1.15, k2=2.0, k125=1.25):
    radius = min(radius, 0.5 * min(width, height))
    min_edge = min(width, height)
    rmax = 0.5 * min_edge
    r0 = min(math.hypot(radius, s * k115), rmax)
    r1 = min(math.hypot(radius, s * k2), rmax)
    e = 2 * r1 / r0
    delta = k125 * s * (math.exp(-(0.5 * width / s) ** 2) - math.exp(-(0.5 * height / s) ** 2))
    w = width + min(delta, 0)
    h = height - max(delta, 0)
    sc = 0.5 * erffn(np.array(0.5 * (max(w, h) - 0.5 * radius) / s))
    y = py - 0.5 * height
    y0 = r1 + np.abs(y) - 0.5 * h
    y1 = np.maximum(y0, 0)
    x = px - 0.5 * width
    x0 = r1 + np.abs(x) - 0.5 * w
    x1 = np.maximum(x0, 0)
    dpos = (x1 ** e + y1 ** e) ** (1 / e)
    dneg = np.minimum(np.maximum(x0, y0), 0)
    d = dpos + dneg - r1
    return sc * (erffn((min_edge + d) / s) - erffn(d / s))

def gauss_product(px, py, W, H, sigma):
    from scipy.special import ndtr
    fx = ndtr(px / sigma) - ndtr((px - W) / sigma)
    fy = ndtr(py / sigma) - ndtr((py - H) / sigma)
    return fx * fy

def score(model):
    total = 0
    count = 0
    worst = []
    for c in cells:
        pred = model(c)
        err = (pred - c["alpha"])[c["mask"]]
        total += (err ** 2).sum()
        count += err.size
        worst.append((c["name"], float(np.abs(err).max()), float(np.sqrt((err ** 2).mean()))))
    return math.sqrt(total / count), worst

rms, worst = score(lambda c: gauss_product(c["px"], c["py"], c["W"], c["H"], 24))
print("gauss product sigma 24:", round(rms * 255, 3), "levels")
for s in [24, 30, 33.94, 36, 40]:
    rms, worst = score(lambda c: levien(c["px"], c["py"], c["W"], c["H"], 0, s))
    print(f"levien s={s}:", round(rms * 255, 3), "levels", " ".join(f"{n}:{m*255:.1f}" for n, m, r in worst))
res = minimize(lambda v: score(lambda c: levien(c["px"], c["py"], c["W"], c["H"], 0, v[0]))[0], [33.94], method="Nelder-Mead")
print("best levien s", res.x, res.fun * 255)
rms, worst = score(lambda c: levien(c["px"], c["py"], c["W"], c["H"], 0, res.x[0]))
for n, m, r in worst:
    print(f"  {n}: max {m*255:.1f} rms {r*255:.2f}")
