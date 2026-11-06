# Scroll bar thumb geometry (fixtures/image-probe-scroll.luau): the probe draws the thumbs with a solid white
# texture (row 1: the end caps only, row 2: the middle only), so each part's snapped quad is a white run in the
# column through the bar. Prints Studio's parts per bar and scores thumb-length / offset rounding rules
# (logical px: none / round / floor / ceil) against them.
#
#   python tools/analysis/image_scroll.py [compare name]
import json
import math
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
S = 1.5
ROUNDS = {"none": lambda v: v, "round": lambda v: math.floor(v + 0.5), "floor": math.floor, "ceil": math.ceil}


def snap(v):
	return math.floor((math.floor(v * 256 + 0.5) + 128) / 256)


def runs(column):
	on = column > 100
	out, i = [], 0
	while i < len(on):
		if on[i]:
			j = i
			while j < len(on) and on[j]:
				j += 1
			out.append((i, j))
			i = j
		else:
			i += 1
	return out


def main():
	name = sys.argv[1] if len(sys.argv) > 1 else "image-probe-scroll"
	run = os.path.join(ROOT, "out", "compare", name)
	studio = np.asarray(Image.open(os.path.join(run, "studio.png")).convert("RGB"), dtype=np.int64).max(-1)
	scene = json.load(open(os.path.join(run, "rur", "scene.json"), encoding="utf-8"))
	bars = {}
	for node in scene["nodes"]:
		scroll = node.get("scroll")
		if not scroll:
			continue
		row, index = node["name"][1:].split("_")
		fx, fy, fw, fh = node["rect"]
		t = scroll["thickness"]
		column = math.floor((fx + fw - t / 2) * S)
		parts = [(a, b) for a, b in runs(studio[:, column]) if fy * S - 2 <= a < (fy + fh) * S + 2]
		entry = bars.setdefault(index, {"rect": node["rect"], "scroll": scroll})
		entry["caps" if row == "1" else "middle"] = parts
		entry["top1" if row == "1" else "top2"] = fy
	scores = {}
	for index, entry in sorted(bars.items(), key=lambda kv: int(kv[0])):
		fx, fy, fw, fh = entry["rect"]
		scroll = entry["scroll"]
		t = scroll["thickness"]
		canvas = scroll["canvas"][3]
		position = (scroll.get("canvasPosition") or [0, 0])[1]
		caps, middle = entry.get("caps", []), entry.get("middle", [])
		print(f"bar {index}: height {fh} canvas {canvas} thickness {t} CanvasPosition {position}: caps {caps} middle {middle}")
		for length_rule, length_round in ROUNDS.items():
			for offset_rule, offset_round in ROUNDS.items():
				thumb = min(fh, max(2 * t, length_round(fh * min(1, fh / canvas))))
				progress = max(0, min(1, position / (canvas - fh))) if canvas > fh else 0
				offset = offset_round((fh - thumb) * progress)
				top, bottom = entry["top1"] + offset, entry["top1"] + offset + thumb
				predicted_caps = [(snap(top * S), snap((top + t) * S)), (snap((bottom - t) * S), snap(bottom * S))]
				top, bottom = entry["top2"] + offset, entry["top2"] + offset + thumb
				predicted_middle = [(snap((top + t) * S), snap((bottom - t) * S))]
				ok = predicted_caps == caps and predicted_middle == middle
				key = f"length {length_rule}, offset {offset_rule}"
				scores[key] = scores.get(key, 0) + int(ok)
	for key, value in sorted(scores.items(), key=lambda kv: -kv[1])[:6]:
		print(f"{key:32} {value} of {len(bars)} bars")


if __name__ == "__main__":
	main()
