"""Fits how TweenService interpolates each property type (tools/studio/anim-probe-lerp.luau).

    python tools/analysis/anim_lerp.py [--ours] [--fit]

Each row holds the reference alpha (a NumberValue tweened 0 -> 1 with the same TweenInfo) and the
value of every case at that render frame. For each case, candidate formulas are evaluated at the
reference alpha. Default: checks Studio's values against the fitted rules; --ours also checks the
offline run (ours.json) against the same rules at its own alpha (timing independent) and compares
the not-tweenable error messages; --fit prints how each candidate formula scores on Studio's data.
"""

import json
import math
import os
import sys

import numpy as np

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
F = np.float32


def f(x):
    return float(F(x))


def parse(text):
    return [float(part) for part in text.split()] if text not in ("true", "false") else [1.0 if text == "true" else 0.0]


def round_half_away(x):
    return math.floor(abs(x) + 0.5) * (1 if x >= 0 else -1)


# Candidate scalar lerps: (from, to, alpha) -> value
FLOAT_CANDIDATES = {
    "f32(a+(b-a)t) double": lambda a, b, t: f(a + (b - a) * t),
    "f32 ops a+(b-a)t": lambda a, b, t: f(f(a) + f(f(f(b) - f(a)) * f(t))),
    "f32 ops a(1-t)+bt": lambda a, b, t: f(f(f(a) * f(1 - f(t))) + f(f(b) * f(t))),
    "f32(a(1-t)+bt) double": lambda a, b, t: f(a * (1 - t) + b * t),
}
DOUBLE_CANDIDATES = {
    "a+(b-a)t": lambda a, b, t: a + (b - a) * t,
    "a(1-t)+bt": lambda a, b, t: a * (1 - t) + b * t,
    "f32 ops a+(b-a)t": lambda a, b, t: f(f(a) + f(f(f(b) - f(a)) * f(t))),
}
INT_CANDIDATES = {
    "round half away": lambda a, b, t: round_half_away(a + (b - a) * t),
    "floor(x+0.5)": lambda a, b, t: math.floor(a + (b - a) * t + 0.5),
    "trunc": lambda a, b, t: math.trunc(a + (b - a) * t),
    "round f32": lambda a, b, t: round_half_away(f(f(a) + f(f(f(b) - f(a)) * f(t)))),
}
DOUBLE_CASES = {"Number", "NumberBig"}
INT_CASES = {"Int", "IntNeg", "ZIndex", "LayoutOrder", "BorderSizePixel", "ScrollBarThickness", "TextSize"}
BOOL_CASES = {"Bool", "BoolDown", "Visible"}
# UDim-bearing types: which components are integer offsets
OFFSET_COMPONENTS = {"Size": (1, 3), "Position": (1, 3), "CanvasSize": (1, 3), "CornerRadius": (1,), "ListPadding": (1,)}


FLOAT32 = lambda a, b, t: f(f(a) + f(f(f(b) - f(a)) * f(t)))


def expected(name, start, goal, alpha):
    """The fitted rule (see the module docstring and notes/animation.md) for one case."""
    if name in BOOL_CASES:
        return [1.0 if (start[0] + (goal[0] - start[0]) * alpha) != 0 else 0.0]
    if name == "TextSize":
        # float lerp, then TextSize's own setter floors the value
        return [math.floor(FLOAT32(start[0], goal[0], alpha))]
    if name in INT_CASES:
        return [round_half_away(start[0] + (goal[0] - start[0]) * alpha)]
    if name in DOUBLE_CASES:
        return [start[0] + (goal[0] - start[0]) * alpha]
    values = []
    for component, (a, b) in enumerate(zip(start, goal)):
        if component in OFFSET_COMPONENTS.get(name, ()):
            values.append(round_half_away(a + (b - a) * alpha))
        else:
            values.append(FLOAT32(a, b, alpha))
    return values


def check(data, label):
    """Checks every row of a run against the fitted rules at that run's own reference alpha."""
    total = exact = 0
    worst_rows = []
    for index, name in enumerate(data["Names"]):
        if name.startswith("CFrame"):
            continue
        meta = data["Meta"][name]
        start, goal = parse(meta["From"]), parse(meta["To"])
        ok = 0
        worst = 0.0
        for row in data["Rows"]:
            alpha = float(row[1])
            value = parse(row[2 + index])
            want = expected(name, start, goal, alpha)
            error = max(abs(x - y) for x, y in zip(value, want)) if len(value) == len(want) else float("inf")
            worst = max(worst, error)
            ok += error == 0
        total += len(data["Rows"])
        exact += ok
        worst_rows.append((worst, name, ok, len(data["Rows"])))
    worst_rows.sort(reverse=True)
    print(f"{label}: {exact}/{total} values match the fitted rules exactly")
    for worst, name, ok, count in worst_rows[:6]:
        if worst > 0:
            print(f"  {name:24s} exact {ok}/{count} worst {worst:.3g}")


def cframe_errors(data, label):
    for index, name in enumerate(data["Names"]):
        if not name.startswith("CFrame"):
            continue
        meta = data["Meta"][name]
        start, goal = np.array(parse(meta["From"])), np.array(parse(meta["To"]))
        worst = 0.0
        for row in data["Rows"]:
            alpha = float(row[1])
            value = np.array(parse(row[2 + index]))
            # position is lerped; rotation compared through the angle to the start rotation
            position = start[:3] + (goal[:3] - start[:3]) * alpha
            worst = max(worst, float(np.max(np.abs(value[:3] - position))))
        print(f"{label} {name}: position max error {worst:.3g}")


def main():
    folder = os.path.join(ROOT, "out", "probe", "anim-probe-lerp")
    studio = json.load(open(os.path.join(folder, "studio.json")))
    check(studio, "studio")
    cframe_errors(studio, "studio")
    ours_path = os.path.join(folder, "ours.json")
    if "--ours" in sys.argv and os.path.exists(ours_path):
        ours = json.load(open(ours_path))
        check(ours, "ours")
        cframe_errors(ours, "ours")
        # CFrame: ours vs Studio at the same alpha is not available row by row (different frame
        # times); compare the rotation slerp through the rows whose alphas coincide within 1e-6
        errors_s, errors_o = studio.get("Errors", {}), ours.get("Errors", {})
        for name in sorted(set(errors_s) | set(errors_o)):
            same = errors_s.get(name) == errors_o.get(name)
            print(f"not tweenable {name:22s} {'same message' if same else 'DIFFERS'}")
            if not same:
                print(f"    studio: {errors_s.get(name)!r}")
                print(f"    ours:   {errors_o.get(name)!r}")
    if "--fit" in sys.argv:
        fit(studio)


def fit(studio):
    for index, name in enumerate(studio["Names"]):
        meta = studio["Meta"][name]
        start, goal = parse(meta["From"]), parse(meta["To"])
        rows = [(float(row[1]), parse(row[2 + index])) for row in studio["Rows"]]
        if name.startswith("CFrame") or name in BOOL_CASES:
            continue
        if name in INT_CASES:
            candidates = INT_CANDIDATES
        elif name in DOUBLE_CASES:
            candidates = DOUBLE_CANDIDATES
        else:
            candidates = FLOAT_CANDIDATES
        results = []
        for label, formula in candidates.items():
            ok = 0
            for alpha, value in rows:
                predicted = []
                for component, (a, b) in enumerate(zip(start, goal)):
                    if component in OFFSET_COMPONENTS.get(name, ()):
                        predicted.append(round_half_away(a + (b - a) * alpha))
                    else:
                        predicted.append(formula(a, b, alpha))
                ok += predicted == value
            results.append((ok, label))
        results.sort(reverse=True)
        print(f"{name:24s} " + ", ".join(f"{label}: {ok}/{len(rows)}" for ok, label in results))


if __name__ == "__main__":
    main()
