"""Build-time clocks: fixtures/anim-mount.luau, Studio vs offline.

    python tools/analysis/anim_mount.py [out/compare/anim-mount] [--scene other/scene.json]

Row i of the fixture reads os.clock() and starts task.delay(0.2 + i * 0.002) while the story is being
built. At the capture its Clock bar is (capture os.clock - row i's build clock) * 100 px wide and its
Delay bar has grown Heartbeat dt * 100 px since the delay resumed. Studio's widths come from the
compare's layout dump (layout-full.json, 3 decimals); ours from the offline scene (rur/scene.json, or
--scene to score another offline run against the same capture). Prints, per row, the build time each
side implies (ms before the end of the build) and the differences.
"""

import json
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def studio_widths(folder):
    with open(os.path.join(folder, "layout-full.json"), encoding="utf8") as handle:
        layout = json.load(handle)
    widths = {}
    for row in layout["rows"]:
        match = re.search(r"/Row(\d+)/(ClockBar|DelayBar)$", row["path"])
        if match and "studio" in row:
            widths[(int(match.group(1)), match.group(2))] = row["studio"][2]
    return widths


def scene_widths(path):
    with open(path, encoding="utf8") as handle:
        scene = json.load(handle)
    widths = {}
    for node in scene["nodes"]:
        match = re.search(r"\.Row(\d+)\.(ClockBar|DelayBar)$", node["path"])
        if match:
            widths[(int(match.group(1)), match.group(2))] = node["rect"][2]
    return widths


def main():
    argv = sys.argv[1:]
    scene = None
    if "--scene" in argv:
        index = argv.index("--scene")
        scene = argv[index + 1]
        del argv[index:index + 2]
    folder = os.path.abspath(argv[0]) if argv else os.path.join(ROOT, "out", "compare", "anim-mount")
    scene = scene or os.path.join(folder, "rur", "scene.json")
    studio = studio_widths(folder)
    ours = scene_widths(scene)
    rows = sorted({index for index, _ in studio})
    last = max(rows)
    print("row | Studio build ms before end | ours | clock bar diff px | delay bar Studio / ours / diff px")
    worst_clock = worst_delay = 0.0
    for index in rows:
        clock_studio, clock_ours = studio[(index, "ClockBar")], ours.get((index, "ClockBar"), float("nan"))
        delay_studio, delay_ours = studio[(index, "DelayBar")], ours.get((index, "DelayBar"), float("nan"))
        # Clock bar px = 100 x seconds since the row was built: relative to the last row
        built_studio = (clock_studio - studio[(last, "ClockBar")]) * 10
        built_ours = (clock_ours - ours.get((last, "ClockBar"), float("nan"))) * 10
        worst_clock = max(worst_clock, abs(clock_studio - clock_ours))
        worst_delay = max(worst_delay, abs(delay_studio - delay_ours))
        if index % 4 == 1 or index == last:
            print(f"{index:3d} | {built_studio:7.2f} | {built_ours:7.2f} | {clock_ours - clock_studio:+.3f} | "
                  f"{delay_studio:.3f} / {delay_ours:.3f} / {delay_ours - delay_studio:+.3f}")
    print(f"worst: clock bar {worst_clock:.3f} px ({worst_clock * 10:.2f} ms), delay bar {worst_delay:.3f} px")


if __name__ == "__main__":
    main()
