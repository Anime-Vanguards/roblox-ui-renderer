# UIStroke gradient probe (fixtures/stroke-gradient.luau): per stroked frame, the parity inside its stroke's
# outer rect (grown by 2 px) between Studio and a render (default out/compare/<name>/rur/render.png).
#
#   python tools/analysis/stroke_gradient.py [compare name] [--render png]
import json
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def main():
	args = [a for a in sys.argv[1:] if not a.startswith("--")]
	name = args[0] if args else "stroke-gradient"
	run = os.path.join(ROOT, "out", "compare", name)
	render = sys.argv[sys.argv.index("--render") + 1] if "--render" in sys.argv else os.path.join(run, "rur", "render.png")
	studio = np.asarray(Image.open(os.path.join(run, "studio.png")).convert("RGB"), dtype=np.int64)
	ours = np.asarray(Image.open(render).convert("RGB"), dtype=np.int64)
	scene = json.load(open(os.path.join(run, "rur", "scene.json"), encoding="utf-8"))
	S = scene["viewport"].get("scale", 1.5)
	total = [0, 0, 0]
	for node in scene["nodes"]:
		strokes = [s for s in (node.get("strokes") or []) if isinstance(s, dict)]
		if not strokes:
			continue
		t = max(s.get("thickness", 0) for s in strokes) * S
		x, y, w, h = [v * S for v in node["rect"]]
		x0, y0 = int(max(0, x - t - 2)), int(max(0, y - t - 2))
		x1, y1 = int(min(studio.shape[1], x + w + t + 3)), int(min(studio.shape[0], y + h + t + 3))
		d = np.abs(studio[y0:y1, x0:x1] - ours[y0:y1, x0:x1]).max(-1)
		painted = (studio[y0:y1, x0:x1] != studio[2, 2]).any(-1) | (ours[y0:y1, x0:x1] != studio[2, 2]).any(-1)
		n = int(painted.sum())
		exact = int(((d <= 2) & painted).sum())
		bit = int(((d == 0) & painted).sum())
		total[0] += n
		total[1] += exact
		total[2] += bit
		print(f"{node['name']:10} painted {n:6d}  exact {100 * exact / max(1, n):6.2f}%  bit {100 * bit / max(1, n):6.2f}%  max {int(d.max())}")
	print(f"all strokes: painted {total[0]}  exact {100 * total[1] / max(1, total[0]):.2f}%  bit {100 * total[2] / max(1, total[0]):.2f}%")


if __name__ == "__main__":
	main()
