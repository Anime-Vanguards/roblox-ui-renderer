"""Spring libraries, Studio vs the emulation (tools/studio/anim-probe-springs.luau).

    python tools/analysis/anim_springs.py [--rows]

Compares, per render frame after the targets change, every spring value of
  studio.json       Studio (its own jittery 240 Hz Heartbeat / 60 Hz render clock)
  ours-clock.json   offline, replaying Studio's recorded clock (anim_clock.py + anim_probe.mjs --clock):
                    must match Studio exactly (the libraries integrate identically given the same dts)
  ours.json         offline, ideal clock (Heartbeat float32(1/240), render float32(1/60)): differs
                    from Studio only by Studio's frame-time jitter
The error of a value is printed in its own unit (offsets in px, colors 0..1, transparency 0..1,
rotation in degrees) and, for the ideal clock, also relative to the travel (goal - start).
"""

import json
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FOLDER = os.path.join(ROOT, "out", "probe", "anim-probe-springs")


def load(name):
    path = os.path.join(FOLDER, name)
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf8") as handle:
        return json.load(handle)


def rows(data):
    return [entry for entry in data["Log"] if entry[0] == "r"]


def numbers(text):
    return [float(part) for part in text.split()]


TRAVEL = {
    "VideCritical": 100, "VideUnder": 100, "VideOver": 100, "VideColor": 1, "VideSize": 190,
    "ControllerDefault": 100, "ControllerDuration": 100, "ControllerWobbly": 100,
    "SprPosition": 100, "SprTransparency": 0.5, "SprRotation": 45,
}


def compare(label, reference, other, relative=False):
    columns = reference["Columns"]
    ref_rows, other_rows = rows(reference), rows(other)
    count = min(len(ref_rows), len(other_rows))
    print(f"== {label} ({count} frames)")
    worst_total = 0.0
    for index, name in enumerate(columns):
        worst = 0.0
        worst_frame = None
        exact = 0
        for row_a, row_b in zip(ref_rows[:count], other_rows[:count]):
            a, b = numbers(row_a[3 + index]), numbers(row_b[3 + index])
            error = max(abs(x - y) for x, y in zip(a, b))
            exact += error == 0
            if error > worst:
                worst, worst_frame = error, row_a[2]
        scaled = f" ({worst / TRAVEL[name] * 100:.2f}% of travel)" if relative else ""
        print(f"  {name:20s} exact {exact:3d}/{count} worst {worst:.4g}{scaled} at frame {worst_frame}")
        worst_total = max(worst_total, worst / TRAVEL[name])
    return worst_total


def main():
    studio = load("studio.json")
    replay = load("ours-clock.json")
    ideal = load("ours.json")
    if replay:
        compare("recorded Studio clock replayed offline vs Studio", studio, replay)
    if ideal:
        compare("ideal clock offline vs Studio", studio, ideal, relative=True)
    if "--rows" in sys.argv and ideal:
        column = sys.argv[sys.argv.index("--rows") + 1] if len(sys.argv) > sys.argv.index("--rows") + 1 else "VideCritical"
        index = studio["Columns"].index(column)
        for row_s, row_i in zip(rows(studio), rows(ideal)):
            print(row_s[2], row_s[3 + index], row_i[3 + index])


if __name__ == "__main__":
    main()
