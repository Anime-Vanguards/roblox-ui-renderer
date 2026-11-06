# Which span a Stretch image's UVs run over (fixtures/image-probe-span.luau): per banner row, the Studio pixels
# against the banner sampled bilinearly (8-bit weights, 12-bit truncation, gpu8, float blend over the backdrop)
# with u = (X + 0.5 - L + offset) / span for candidate spans, the best vertical offset fitted too (captures may
# carry the vertical projection stretch). Also prints the white boxes' covered columns (the snapped edges).
#
#   python tools/analysis/image_span.py [compare name]
import json
import math
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
F = np.float32


def snap(v):
	return math.floor(v + 0.5)


def gpu8(v):
	v = np.clip(v, 0, 1)
	return np.floor((np.floor(v * 4096) * 255 + 2047) / 4096)


def bilinear(t, sx, sy):
	"""t (H, W, C) sampled at texel coordinates (centres at i + 0.5): 8-bit weights, 12-bit truncated, 0..1."""
	fx = sx - 0.5
	fy = sy - 0.5
	x0 = np.floor(fx)
	y0 = np.floor(fy)
	ax = np.floor((fx - x0) * 256 + 0.5) / 256
	ay = np.floor((fy - y0) * 256 + 0.5) / 256
	x0 = np.where(ax >= 1, x0 + 1, x0).astype(int)
	ax = np.where(ax >= 1, 0, ax)
	y0 = np.where(ay >= 1, y0 + 1, y0).astype(int)
	ay = np.where(ay >= 1, 0, ay)
	h, w = t.shape[:2]
	xa, xb = np.clip(x0, 0, w - 1), np.clip(x0 + 1, 0, w - 1)
	ya, yb = np.clip(y0, 0, h - 1), np.clip(y0 + 1, 0, h - 1)
	ax = ax[..., None]
	ay = ay[..., None]
	top = t[ya, xa] * (1 - ax) + t[ya, xb] * ax
	bottom = t[yb, xa] * (1 - ax) + t[yb, xb] * ax
	v = top * (1 - ay) + bottom * ay
	exact = (ax == 0) & (ay == 0)
	return np.where(exact, v / 255, np.floor(v / 255 * 4096) / 4096)


def main():
	name = sys.argv[1] if len(sys.argv) > 1 else "image-probe-span"
	run = os.path.join(ROOT, "out", "compare", name)
	studio = np.asarray(Image.open(os.path.join(run, "studio.png")).convert("RGB"), dtype=np.float64)
	scene = json.load(open(os.path.join(run, "rur", "scene.json"), encoding="utf-8"))
	report = json.load(open(os.path.join(run, "report.json"), encoding="utf-8"))
	backdrop = np.array((report.get("options") or {}).get("background") or [40, 40, 48], dtype=np.float64)
	S = F(scene["viewport"].get("scale", 1.5))
	HW = studio.shape[1]
	for node in scene["nodes"]:
		image = node.get("image")
		if not image:
			continue
		uri = image["uri"].split("//")[-1]
		tex = np.asarray(Image.open(os.path.join(run, "rur", "assets", f"{uri}.png")).convert("RGBA"), dtype=np.float64)
		TH, TW = tex.shape[:2]
		x, y, w, h = node["rect"]
		x0 = F(F(x) * S)
		x1 = F(x0 + F(F(w) * S))
		y0 = F(F(y) * S)
		y1 = F(y0 + F(F(h) * S))
		L, R, T, B = snap(float(x0)), snap(float(x1)), snap(float(y0)), snap(float(y1))
		cols = np.arange(max(L, 0), min(R, HW))
		rows = np.arange(T + 2, B - 2)
		PX, PY = np.meshgrid(cols + 0.5, rows + 0.5)
		actual = studio[rows][:, cols]
		# the white box below: covered columns
		box_row = B + 4
		line = studio[box_row].min(-1)
		covered = np.nonzero(line > 200)[0]
		near = covered[(covered >= L - 3) & (covered < R + 3)]
		box = (int(near.min()), int(near.max()) + 1) if len(near) else None
		print(f"{node['name']:14} x {float(x0):.5f} w {float(F(F(w) * S)):.4f} right {float(x1):.5f}  edges {L}..{R} ({R - L})  box {box}")
		results = []
		for span in (R - L - 1, R - L - 0.5, R - L, R - L + 0.5, R - L + 1, float(F(F(w) * S))):
			for off in (-0.25, 0, 0.25):
				best = None
				for dy in np.arange(-0.3, 0.31, 0.05):
					u = (PX - L + off) / span
					v = (PY - T + dy) / (B - T)
					s = bilinear(tex, u * TW, v * TH)
					A = gpu8(s[..., 3])[..., None]
					C = gpu8(s[..., :3])
					pred = np.floor((C * A + backdrop * (255 - A)) / 255 + 0.5)
					d = np.abs(pred - actual).max(-1)
					score = ((d == 0).mean(), (d <= 2).mean(), round(float(dy), 2))
					if best is None or score > best:
						best = score
				results.append((best[0], best[1], span, off, best[2]))
		results.sort(reverse=True)
		for bit, exact, span, off, dy in results[:3]:
			print(f"    span {span:8.3f} offset {off:+.2f} (dy {dy:+.2f}): bit {100 * bit:.2f}%  exact {100 * exact:.2f}%")


if __name__ == "__main__":
	main()
