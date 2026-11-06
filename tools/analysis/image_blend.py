# Compositing arithmetic on fixtures/group-blend.luau: fits Studio's blend of frames, faded CanvasGroups and
# transparent groups holding semi-transparent frames, per 8-bit channel sample (256 colour steps per row, two
# backdrops).
#
#   python tools/analysis/image_blend.py [compare name]
#
# Prints, per row and backdrop, how many of the 256 steps each model reproduces exactly (all three channels).
# The winning models (every sample):
#   - frames and faded groups: out = round(c * a8 / 255 + D * (255 - a8) / 255), a8 = min(255, floor(a * 256))
#   - groups: texture C = round(c * a8 / 255), A = a8 (premultiplied render target); the composite shader
#     unpremultiplies in float32, converts colour and alpha with gpu8 (12-bit truncation, round half down) and
#     blends in float with one rounding
import math
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
F = np.float32


def f32(v):
	return float(np.float32(v))


def rnd(v):
	return np.floor(np.asarray(v, dtype=np.float64) + 0.5)


def alpha8(v):
	return min(255, math.floor(v * 256))


def gpu8(x):
	"""This GPU's float -> 8-bit conversion of shader outputs (x in 0..1)."""
	x = np.asarray(x, dtype=np.float64)
	return np.floor((np.floor(x * 4096) * 255 + 2047) / 4096)


def colour(k):
	return np.array([k, 255 - k, (k * 7) % 256], dtype=np.float64)


# Rows of the fixture: (kind, transparency, row index); FadedClear holds 0.5-transparent frames
ROWS = []
_row = 0
for _t in [0.1, 0.3, 0.5, 0.7, 0.9, 0.25]:
	ROWS.append(("Faded", _t, _row))
	_row += 1
for _t in [0.9, 0.5, 0.25, 0.75]:
	ROWS.append(("Clear", _t, _row))
	_row += 1
for _t in [0.9, 0.5, 0.25, 0.75]:
	ROWS.append(("Plain", _t, _row))
	_row += 1
for _t in [0.5, 0.3]:
	ROWS.append(("FadedClear", _t, _row))
	_row += 1

BACKDROPS = ((20, np.array([20, 20, 26.0])), (620, np.array([200, 120, 60.0])))


def sample(image, x0, row, k):
	x = int((x0 + (k % 128) * 2) * 1.5) + 1
	y = int((20 + row * 24 + (0 if k < 128 else 8)) * 1.5) + 6
	return image[y, x]


def models(kind, transparency, d, colours):
	out = {}
	if kind in ("Plain", "Faded"):
		a = 1 - f32(transparency)
		a8 = alpha8(a)
		out["alpha8, float blend"] = rnd(colours * a8 / 255 + d * (1 - a8 / 255))
		out["float alpha, float blend"] = rnd(colours * a + d * (1 - a))
		out["round(a * 255), float blend"] = rnd(colours * rnd(255 * a) / 255 + d * (1 - rnd(255 * a) / 255))
		pre = rnd(colours * a8 / 255)
		out["alpha8, quantized source (canvas)"] = rnd(pre + d * (1 - a8 / 255))
		return out
	content = alpha8(1 - f32(transparency if kind == "Clear" else 0.5))
	group = 1 if kind == "Clear" else alpha8(1 - f32(transparency)) / 255
	C = rnd(colours * content / 255)
	A = content
	Cf = (C / 255).astype(F)
	Af = F(A / 255)
	cp = gpu8(Cf / Af)
	ap = gpu8(Af * F(group))
	out["texture, unpremultiply, gpu8, float blend"] = rnd(cp * ap / 255 + d * (1 - ap / 255))
	out["texture drawn premultiplied"] = rnd(C * group + d * (1 - A / 255 * group))
	out["unpremultiply without gpu8"] = rnd(rnd(C * 255 / A) * rnd(A * group) / 255 + d * (1 - rnd(A * group) / 255))
	return out


def main():
	args = [a for a in sys.argv[1:] if not a.startswith("--")]
	name = args[0] if args else "group-blend"
	image = np.asarray(Image.open(os.path.join(ROOT, "out", "compare", name, "studio.png")).convert("RGB"), dtype=np.int64)
	colours = np.array([colour(k) for k in range(256)])
	totals = {}
	for kind, transparency, row in ROWS:
		for x0, d in BACKDROPS:
			studio = np.array([sample(image, x0, row, k) for k in range(256)])
			line = f"{kind:10} T={transparency:<5} {'panel' if x0 == 620 else 'dark '}"
			for label, predicted in models(kind, transparency, d, colours).items():
				hits = int(np.all(predicted == studio, axis=1).sum())
				totals[label] = totals.get(label, 0) + hits
				line += f"  {label}: {hits}"
			print(line)
	print()
	for label, hits in sorted(totals.items(), key=lambda kv: -kv[1]):
		print(f"{label:45} {hits}")


if __name__ == "__main__":
	main()
