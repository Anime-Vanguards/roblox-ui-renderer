"""Fits TweenService:GetValue: a float32 model of every EasingStyle x EasingDirection, checked
against the Studio grid (tools/studio/anim-probe-easing.luau -> out/probe/anim-probe-easing).

    python tools/analysis/anim_easing.py [--ours]

--ours also compares the offline runtime's values (ours.json) with Studio.
Prints, per curve, the number of grid points matching bit-exactly (as float32) and the worst error
in float32 ulps.
"""

import json
import math
import os
import sys

import numpy as np

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
F = np.float32


def f(x):
    return F(x)


def In(style, t):
    """The In curve of `style` for a float32 alpha in (0, 1). Measured: Back and Circular run in
    float32; the others run in double from the float32 alpha (Exponential/Elastic take their
    exponent 10 * (t - 1) and Bounce its 1 - t in float32) and round the result to float32."""
    t = f(t)
    d = float(t)
    if style == "Linear":
        return t
    if style == "Quad":
        return f(d * d)
    if style == "Cubic":
        return f(d * d * d)
    if style == "Quart":
        return f(d * d * d * d)
    if style == "Quint":
        return f(d * d * d * d * d)
    if style == "Sine":
        return f(1 - math.cos(d * math.pi / 2))
    if style == "Exponential":
        return f(2.0 ** float(f(f(t - f(1)) * f(10))))
    if style == "Circular":
        return f(f(1) - f(math.sqrt(float(f(f(1) - f(t * t))))))
    if style == "Back":
        s = f(1.70158)
        return f(f(t * t) * f(f(f(s + f(1)) * t) - s))
    if style == "Elastic":
        return f(math.sin(13 * math.pi / 2 * d) * 2.0 ** float(f(f(t - f(1)) * f(10))))
    if style == "Bounce":
        return f(1 - BounceOut(float(f(f(1) - t))))
    raise KeyError(style)


def BounceOut(t):
    if t < 1 / 2.75:
        return 7.5625 * t * t
    if t < 2 / 2.75:
        t -= 1.5 / 2.75
        return 7.5625 * t * t + 0.75
    if t < 2.5 / 2.75:
        t -= 2.25 / 2.75
        return 7.5625 * t * t + 0.9375
    t -= 2.625 / 2.75
    return 7.5625 * t * t + 0.984375


def Ease(style, direction, alpha):
    t = f(alpha)
    if t <= 0:
        return f(0)
    if t >= 1:
        return f(1)
    if direction == "In":
        return In(style, t)
    if direction == "Out":
        return f(f(1) - In(style, f(f(1) - t)))
    if t < f(0.5):
        return f(In(style, f(t * f(2))) * f(0.5))
    return f(f(1) - f(In(style, f(f(f(1) - t) * f(2))) * f(0.5)))


def ulps(a, b):
    a, b = F(a), F(b)
    if a == b:
        return 0
    ia = int(np.frombuffer(a.tobytes(), dtype=np.int32)[0])
    ib = int(np.frombuffer(b.tobytes(), dtype=np.int32)[0])
    if ia < 0:
        ia = -2**31 - ia
    if ib < 0:
        ib = -2**31 - ib
    return abs(ia - ib)


def main():
    folder = os.path.join(ROOT, "out", "probe", "anim-probe-easing")
    studio = json.load(open(os.path.join(folder, "studio.json")))
    ours = json.load(open(os.path.join(folder, "ours.json"))) if "--ours" in sys.argv else None
    alphas = [float(a) for a in studio["Alphas"]]
    total = exact = 0
    for key in sorted(studio["Values"]):
        style, direction = key.split(".")
        values = [float(v) for v in studio["Values"][key].split()]
        errors = [ulps(Ease(style, direction, a), v) for a, v in zip(alphas, values)]
        worst = max(range(len(errors)), key=lambda i: errors[i])
        line = f"{key:20s} model: exact {sum(1 for e in errors if e == 0):4d}/{len(errors)}, <=1ulp {sum(1 for e in errors if e <= 1):4d}, worst {errors[worst]} ulp at {alphas[worst]!r} (studio {values[worst]!r} model {float(Ease(style, direction, alphas[worst]))!r})"
        if ours:
            mine = [float(v) for v in ours["Values"][key].split()]
            ours_errors = [ulps(m, v) for m, v in zip(mine, values)]
            line += f" | ours exact {sum(1 for e in ours_errors if e == 0)}, worst {max(ours_errors)} ulp"
        print(line)
        total += len(errors)
        exact += sum(1 for e in errors if e == 0)
    print(f"model exact: {exact}/{total}")


if __name__ == "__main__":
    main()
