# Per-element pixel parity for image fixtures: compares out/compare/<name>/{studio.png, rur/render.png}
# inside the bounding box of every image node (grown by 2 px for edge fringes), so the empty backdrop does
# not inflate the score.
#
#   python tools/analysis/image_report.py image-stretch image-tile ...   [--elements] [--render <png>]
import json
import math
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SCALE = 1.5
# Measured capture artifact: the viewport capture shows RGB(13, 11, 12) as black
KEY = np.array([13, 11, 12])


def boxes(scene):
	result = []
	for node in scene["nodes"]:
		if not node.get("image"):
			continue
		x, y, w, h = node["rect"]
		pts = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
		m = node.get("matrix")
		if m:
			pts = [(m[0] * px + m[2] * py + m[4], m[1] * px + m[3] * py + m[5]) for px, py in pts]
		xs = [p[0] * SCALE for p in pts]
		ys = [p[1] * SCALE for p in pts]
		box = [math.floor(min(xs)) - 2, math.floor(min(ys)) - 2, math.ceil(max(xs)) + 2, math.ceil(max(ys)) + 2]
		clip = node.get("clip")
		if clip:
			cx0, cy0 = math.floor(clip[0] * SCALE + 0.5), math.floor(clip[1] * SCALE + 0.5)
			cx1, cy1 = math.floor((clip[0] + clip[2]) * SCALE + 0.5), math.floor((clip[1] + clip[3]) * SCALE + 0.5)
			box = [max(box[0], cx0), max(box[1], cy0), min(box[2], cx1), min(box[3], cy1)]
		result.append((node["path"].split(".")[-1], box))
	return result


def report(name, render=None, elements=False):
	run = os.path.join(ROOT, "out", "compare", name)
	studio = np.asarray(Image.open(os.path.join(run, "studio.png")).convert("RGB"), dtype=np.int32)
	ours = np.asarray(Image.open(render or os.path.join(run, "rur", "render.png")).convert("RGB"), dtype=np.int32)
	scene = json.load(open(os.path.join(run, "rur", "scene.json")))
	h, w = studio.shape[:2]
	diff = np.abs(studio - ours).max(axis=-1)
	keyed = np.all(ours == KEY, axis=-1) & np.all(studio == 0, axis=-1)
	diff[keyed] = 0
	mask = np.zeros((h, w), bool)
	rows = []
	for label, (x0, y0, x1, y1) in boxes(scene):
		x0, y0, x1, y1 = max(0, x0), max(0, y0), min(w, x1), min(h, y1)
		if x1 <= x0 or y1 <= y0:
			continue
		mask[y0:y1, x0:x1] = True
		d = diff[y0:y1, x0:x1]
		rows.append((label, d.size, float((d <= 2).mean()), float((d <= 8).mean()), int(d.max())))
	d = diff[mask]
	total = {"pixels": int(d.size), "exact": float((d <= 2).mean()) if d.size else 1.0, "within8": float((d <= 8).mean()) if d.size else 1.0, "exact0": float((d == 0).mean()) if d.size else 1.0}
	print(f"{name:<22} image px {total['pixels']:>8}  exact {100 * total['exact']:.3f}%  within8 {100 * total['within8']:.3f}%  bit-exact {100 * total['exact0']:.2f}%")
	if elements:
		for label, n, e2, w8, mx in sorted(rows, key=lambda r: r[2]):
			if e2 < 0.9995:
				print(f"    {label:<20} {n:>7} px  exact {100 * e2:.2f}%  within8 {100 * w8:.2f}%  max {mx}")
	return total


if __name__ == "__main__":
	args = [a for a in sys.argv[1:] if not a.startswith("--")]
	render = None
	if "--render" in sys.argv:
		render = sys.argv[sys.argv.index("--render") + 1]
		args.remove(render)
	for name in args:
		report(name, render, "--elements" in sys.argv)
