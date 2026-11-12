# The filtering of turned CanvasGroups, read from a compare of fixtures/group-probe-turned.luau.
#
#   python tools/analysis/group_turned.py out/compare/group-probe-turned [--rur]
#
# Every group of the fixture is clear and holds a white child over its left part, so the capture shows one
# edge down the group. Across that edge the red channel runs from 255 to 0; its slope is one over the width of
# the texture's texels on the screen (a level 0 texel at 1:1 gives a ramp one pixel wide), and where it
# crosses a half is where the edge lies. Per group the script prints where the edge lies, in pixels along the
# group's x axis from where the layout puts it (a small spread means the ramp is one pixel wide), and the
# ramp's fitted width.
import json
import math
import sys

import numpy as np
from PIL import Image


def main():
    arguments = [a for a in sys.argv[1:] if not a.startswith("--")]
    if not arguments:
        raise SystemExit("usage: group_turned.py <compare dir> [--rur]")
    directory = arguments[0]
    image = np.asarray(Image.open(f"{directory}/{'rur/render.png' if '--rur' in sys.argv else 'studio.png'}").convert("RGB")).astype(float)
    scene = json.load(open(f"{directory}/rur/scene.json"))
    scale = scene.get("viewport", {}).get("scale", 1.5)
    nodes = {node["id"]: node for node in scene["nodes"]}
    for node in scene["nodes"]:
        if node.get("name") != "Half":
            continue
        parent = nodes[node["parent"]]
        m = node.get("matrix") or [1, 0, 0, 1, 0, 0]
        x, y, w, h = (v * scale for v in node["rect"])
        px, py, pw, ph = (v * scale for v in parent["rect"])

        def screen(lx, ly):
            return (m[0] * lx + m[2] * ly + m[4] * scale, m[1] * lx + m[3] * ly + m[5] * scale)

        # The edge: local x = x + w, over the middle half of the child's height
        cx, cy = screen(x + w, y + h / 2)
        reach = int(max(pw, ph)) + 4
        distances, values = [], []
        for qy in range(max(0, int(cy) - reach), min(image.shape[0], int(cy) + reach + 1)):
            for qx in range(max(0, int(cx) - reach), min(image.shape[1], int(cx) + reach + 1)):
                dx, dy = qx + 0.5 - cx, qy + 0.5 - cy
                along = m[0] * dx + m[1] * dy
                down = m[2] * dx + m[3] * dy
                if abs(down) > h / 4 or abs(along) > 6:
                    continue
                distances.append(along)
                values.append(image[qy, qx, 0] / 255)
        distances, values = np.array(distances), np.array(values)
        ramp = (values > 0.04) & (values < 0.96)
        name = parent["path"].split(".")[-1]
        angle = math.degrees(math.atan2(m[1], m[0]))
        if ramp.sum() < 3:
            inside = distances[values >= 0.96]
            outside = distances[values <= 0.04]
            print(f"{name:10s} {angle:7.2f} deg  hard edge between {inside.max() if len(inside) else float('nan'):.2f} and {outside.min() if len(outside) else float('nan'):.2f} px")
            continue
        # A ramp one pixel wide (level 0 texels at 1:1) crosses a half at distance + value - 0.5
        edges = distances[ramp] + values[ramp] - 0.5
        width = float("nan")
        if np.ptp(distances[ramp]) > 0.3:
            slope = np.polyfit(distances[ramp], values[ramp], 1)[0]
            width = -1 / slope if slope else float("nan")
        print(f"{name:10s} {angle:7.2f} deg  edge at {edges.mean():+.3f} px (sd {edges.std():.3f})  ramp {width:5.2f} px  {int(ramp.sum())} px")


if __name__ == "__main__":
    main()
