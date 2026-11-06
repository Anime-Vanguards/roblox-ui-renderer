# Frame UIGradient per pixel: scores models of Studio's gradient fill on a compare capture (default gradients):
# t at the pixel centre over the unsnapped rect (rotation, offset, span as native/src/render/gradients.rs samples it), the
# colour interpolated linearly between keypoints, converted to 8 bits with round / gpu8 / floor; the alpha
# likewise, blended over the capture's backdrop.
#
#   python tools/analysis/image_gradient.py [compare name] [--frames N] [--verbose]
import json
import math
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def gpu8(x):
	x = np.clip(np.asarray(x, dtype=np.float64), 0, 1)
	return np.floor((np.floor(x * 4096) * 255 + 2047) / 4096)


def snapped(v):
	"""Keypoints exported with 4 decimals: back to k / 255 when that close (the text analysis's snappedColors)."""
	k = round(v * 255)
	return k / 255 if abs(v * 255 - k) < 0.0128 else v


def sample(seq, t, cols):
	"""Piecewise-linear keypoint interpolation (seq rows: [time, values...]) at t (array)."""
	seq = sorted(seq, key=lambda k: k[0])
	times = np.array([k[0] for k in seq])
	out = []
	for c in range(cols):
		values = np.array([k[1 + c] for k in seq])
		out.append(np.interp(t, times, values))
	return np.stack(out, -1)


def gradient_t(gradient, x, y, w, h, px, py, f32=False):
	angle = (gradient.get("rotation") or 0) * math.pi / 180
	dx, dy = math.cos(angle), math.sin(angle)
	ox, oy = (gradient.get("offset") or [0, 0])[:2]
	cx, cy = x + w / 2 + ox * w, y + h / 2 + oy * h
	span_x = w / abs(dx) if abs(dx) > 1e-9 else math.inf
	span_y = h / abs(dy) if abs(dy) > 1e-9 else math.inf
	span = min(span_x, span_y)
	if f32:
		F = np.float32
		t = ((F(px) - F(cx)) * F(dx) + (F(py) - F(cy)) * F(dy)) / F(span) + F(0.5)
		return np.clip(t.astype(np.float64), 0, 1)
	return np.clip(((px - cx) * dx + (py - cy) * dy) / span + 0.5, 0, 1)


def main():
	args = [a for a in sys.argv[1:] if not a.startswith("--")]
	name = args[0] if args else "gradients"
	run = os.path.join(ROOT, "out", "compare", name)
	studio = np.asarray(Image.open(os.path.join(run, "studio.png")).convert("RGB"), dtype=np.float64)
	scene = json.load(open(os.path.join(run, "rur", "scene.json")))
	report = json.load(open(os.path.join(run, "report.json")))
	# The compare's backdrop (older reports have no options: the capture's corner pixel)
	backdrop = np.array((report.get("options") or {}).get("background") or studio[2, 2], dtype=np.float64)
	scale = scene["viewport"].get("scale", 1.5)
	limit = int(sys.argv[sys.argv.index("--frames") + 1]) if "--frames" in sys.argv else 1000
	totals = {}
	count = 0
	for node in scene["nodes"]:
		g = node.get("gradient")
		if not g or node.get("matrix") or node.get("corner") or node.get("text") or node.get("image") or not node.get("bg"):
			continue
		if count >= limit:
			break
		count += 1
		x, y, w, h = [v * scale for v in node["rect"]]
		L, T, R, B = [math.floor(v + 0.5) for v in (x, y, x + w, y + h)]
		# inner pixels only (1 px margin: edges are rasterization, not the gradient)
		xs = np.arange(L + 1, R - 1) + 0.5
		ys = np.arange(T + 1, B - 1) + 0.5
		PX, PY = np.meshgrid(xs, ys)
		s = studio[T + 1:B - 1, L + 1:R - 1]
		base = np.array(node["bg"]["color"], dtype=np.float64)
		base8 = np.minimum(255, np.floor(base * 256)) / 255
		a8 = min(255, math.floor((1 - node["bg"].get("transparency", 0)) * 256)) / 255
		colors = [[k[0], snapped(k[1]), snapped(k[2]), snapped(k[3])] for k in g["color"]]
		trans = g.get("transparency") or [[0, 0, 0], [1, 0, 0]]
		results = {}
		for tname, f32 in (("t64", False), ("t32", True)):
			t = gradient_t(g, x, y, w, h, PX, PY, f32)
			c = sample(colors, t, 3) * base8
			op = (1 - sample(trans, t, 1)[..., 0]) * a8
			for cname, conv in (("gpu8", gpu8), ("round", lambda v: np.floor(np.clip(v, 0, 1) * 255 + 0.5)), ("floor", lambda v: np.floor(np.clip(v, 0, 1) * 255))):
				c8 = conv(c)
				A = conv(op)[..., None]
				out = np.floor((c8 * A + backdrop * (255 - A)) / 255 + 0.5)
				d = np.abs(out - s).max(-1)
				results[f"{tname} {cname}"] = (d == 0).mean()
		for k, v in results.items():
			totals.setdefault(k, []).append(v)
		if "--verbose" in sys.argv:
			print(node["name"], node["rect"], "  ".join(f"{k} {100 * v:.1f}" for k, v in results.items()))
	for k, v in sorted(totals.items(), key=lambda kv: -np.mean(kv[1])):
		print(f"{k:12} mean bit-exact {100 * np.mean(v):.2f}%  min {100 * np.min(v):.2f}%  ({len(v)} frames)")


if __name__ == "__main__":
	main()
