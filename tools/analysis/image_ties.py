# Edge snapping near .5 ties (fixtures/image-probe-ties.luau): per frame / image, the device edges from
# Studio's own float32 geometry (studio-nodes.json) against the covered columns / rows in the capture, and which
# rule reproduces them: plain floor(v + 0.5), or the 1/256 px grid then half up (native/src/render/geometry.rs snap_device).
#
#   python tools/analysis/image_ties.py [compare name]
import json
import math
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
F = np.float32


def plain(v):
	return math.floor(v + 0.5)


def grid(v):
	return math.floor((math.floor(v * 256 + 0.5) + 128) / 256)


def covered(line, lo, hi):
	on = np.nonzero(line > 128)[0]
	on = on[(on >= lo - 3) & (on < hi + 3)]
	return (int(on.min()), int(on.max()) + 1) if len(on) else None


def main():
	name = sys.argv[1] if len(sys.argv) > 1 else "image-probe-ties"
	run = os.path.join(ROOT, "out", "compare", name)
	studio = np.asarray(Image.open(os.path.join(run, "studio.png")).convert("RGB"), dtype=np.int64).min(-1)
	nodes = json.load(open(os.path.join(run, "studio-nodes.json"), encoding="utf-8"))["nodes"]
	S = F(1.5)
	score = {"plain": 0, "grid": 0}
	count = 0
	for node in nodes:
		label = node["path"].split("/")[-1]
		if label[:1] not in "FIV" or label == "Holder":
			continue
		x0 = F(F(node["x"]) * S)
		x1 = F(x0 + F(F(node["w"]) * S))
		y0 = F(F(node["y"]) * S)
		y1 = F(y0 + F(F(node["h"]) * S))
		if label[0] == "V":
			a, b = float(y0), float(y1)
			seen = covered(studio[:, int(float(x0)) + 3], a, b)
		else:
			a, b = float(x0), float(x1)
			seen = covered(studio[int(float(y0)) + 5], a, b)
		for rule, snap in (("plain", plain), ("grid", grid)):
			score[rule] += int(seen == (snap(a), snap(b)))
		count += 1
		print(f"{label:4} {a:.6f} .. {b:.6f}  Studio {seen}  plain {(plain(a), plain(b))}  grid {(grid(a), grid(b))}")
	print(f"{count} elements: plain {score['plain']}, 1/256 grid {score['grid']}")


if __name__ == "__main__":
	main()
