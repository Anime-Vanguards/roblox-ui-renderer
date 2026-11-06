# CanvasGroup texture resampling (fixtures/group-resample.luau): fits, per group and axis, how Studio maps
# the group's content into its texture and the texture back onto the screen.
#
#   python tools/analysis/image_group.py [compare name] [--fit] [--model]
#
# Profiles: a row through the vertical bars (horizontal mapping) and a column through the horizontal bars
# (vertical mapping) of each group; white bars on black, so a pixel's value / 255 is the resampled coverage.
# --fit: free per-group fit of the sampling line T(X) = L + (X + 0.5 - L) * k + c (abs px) over the content
# rasterized at snapped screen pixels, to see what k and c are.
# --model: scores candidate closed-form models over every profile pixel.
import json
import math
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
F32 = np.float32


def snap(v):
	return math.floor(v + 0.5)


def bars_of(W, H):
	"""Bars (relative physical px) as fixtures/group-resample.luau places them."""
	vertical, horizontal = [], []
	index, bx = 0, 4.3
	while bx + 6 < W * 0.45:
		index += 1
		bw = 2.6 + (index % 3) * 1.37
		vertical.append((bx, bx + bw))
		bx += bw + 3.1 + (index % 4) * 1.13
	index, by = 0, 4.6
	while by + 6 < H - 4:
		index += 1
		bh = 2.4 + (index % 3) * 1.29
		horizontal.append((by, by + bh))
		by += bh + 2.9 + (index % 4) * 1.07
	return vertical, horizontal


def groups():
	result = []
	fractions = [0, 0.25, 0.5, 0.75]
	for row in range(4):
		for column in range(4):
			a, b = fractions[column], fractions[row]
			result.append((f"G{row}{column}", 30 + column * 440 + a, 30 + row * 250 + b, 330 + b, 200 + a))
	result += [("Small", 30.5, 1030.25, 61.75, 45.5), ("Large", 130.25, 1030.75, 700.5, 290.25), ("Clear", 870.75, 1030.5, 330.5, 200.25), ("Faded", 1250.5, 1030.25, 330.25, 200.75)]
	return result


def profiles(image, name, x, y, w, h):
	"""Per axis: (p0, size, bars, pixel indices, measured values 0..1)."""
	vertical, horizontal = bars_of(w, h)
	out = {}
	# Horizontal mapping: a row in the middle of the vertical bars
	row = snap(y + h / 2)
	x0, x1 = snap(x) - 2, snap(x + w * 0.45) - 2
	xs = np.arange(x0, x1)
	out["x"] = (x, w, [(x + a, x + b) for a, b in vertical], xs, image[row, x0:x1].mean(axis=-1) / 255)
	# Vertical mapping: a column in the middle of the horizontal bars
	column = snap(x + w * 0.775)
	y0, y1 = snap(y) - 2, snap(y + h) + 2
	ys = np.arange(y0, y1)
	out["y"] = (y, h, [(y + a, y + b) for a, b in horizontal], ys, image[y0:y1, column].mean(axis=-1) / 255)
	return out


def screen_content(p0, size, bars, lo, hi):
	"""Content rasterized at screen pixels [lo, hi): group background 0 inside the snapped rect, bars 1."""
	values = np.zeros(hi - lo)
	for a, b in bars:
		s, e = snap(a), snap(b)
		values[max(0, s - lo):max(0, e - lo)] = 1
	return values


def bilinear(texture, t):
	"""Samples a 1D texture at continuous texel coordinates t (texel k centre at k + 0.5), 8-bit weights,
	clamp to edge."""
	s = t - 0.5
	i0 = np.floor(s).astype(int)
	f = np.floor((s - i0) * 256 + 0.5) / 256
	carry = f >= 1
	i0 = np.where(carry, i0 + 1, i0)
	f = np.where(carry, 0, f)
	n = len(texture)
	a = texture[np.clip(i0, 0, n - 1)]
	b = texture[np.clip(i0 + 1, 0, n - 1)]
	return a + (b - a) * f


def fit_free(p0, size, bars, pixels, measured):
	"""Best line T(X) over screen-rasterized content (abs px), searched on a grid."""
	L, R = snap(p0), snap(p0 + size)
	lo = L - 8
	content = screen_content(p0, size, bars, lo, R + 8)
	inside = (pixels >= L) & (pixels < R)
	X = pixels[inside] + 0.5
	M = measured[inside]
	best = None
	for k in np.arange(0.990, 1.012, 0.00002):
		c = np.arange(-1.6, 1.6, 0.002)[:, None]
		T = (L - lo) + (X[None, :] - L) * k + c
		pred = bilinear(content, T.ravel()).reshape(T.shape)
		err = np.abs(pred - M[None, :]).max(axis=1)
		i = int(np.argmin(err))
		if best is None or err[i] < best[0]:
			best = (float(err[i]), float(k), float(c[i, 0]))
	return best


DPI = 1.5

# Candidate models, per axis. N: texture size; content: (origin, scale) of t = (A - origin) * scale, edges
# snapped floor(t + 0.5); sampling: T(X) = (X + 0.5 - origin) * scale.
MODELS = {
	"screen snap, then N/Q, centre rule": "screen",
	"logical N, raw origin, N/Q": lambda p0, W, L, Q, N: ((p0, N / Q), (L, N / Q)),
	"logical N, snapped origin, N/Q": lambda p0, W, L, Q, N: ((L, N / Q), (L, N / Q)),
	"logical N, raw origin, N/W": lambda p0, W, L, Q, N: ((p0, N / W), (L, N / Q)),
	"logical N, raw origin, sample raw": lambda p0, W, L, Q, N: ((p0, N / Q), (p0, N / W)),
}


def texture_size(Q, rule):
	if rule == "logical":
		return math.ceil(math.ceil(Q / DPI - 1e-9) * DPI - 1e-9)
	return Q


def model_profile(p0, size, bars, pixels, content, sampling, N):
	"""Predicted values 0..1 over `pixels` (inside the snapped quad)."""
	texture = np.zeros(N)
	o, s = content[:2]
	for a, b in bars:
		if content[0] == "screen":
			# Snapped on the screen grid, then scaled into the texture; texels whose centre is inside
			L, s = content[1], content[2]
			t0, t1 = (snap(a) - L) * s, (snap(b) - L) * s
			k = np.arange(N) + 0.5
			texture[(k >= t0) & (k < t1)] = 1
			continue
		t0, t1 = snap((a - o) * s), snap((b - o) * s)
		texture[max(0, t0):max(0, min(N, t1))] = 1
	o, s = sampling
	return bilinear(texture, (pixels + 0.5 - o) * s)


def main():
	args = [a for a in sys.argv[1:] if not a.startswith("--")]
	name = args[0] if args else "group-resample"
	image = np.asarray(Image.open(os.path.join(ROOT, "out", "compare", name, "studio.png")).convert("RGB"), dtype=np.float64)
	totals = {model: [0, 0] for model in MODELS}
	for gname, x, y, w, h in groups():
		for axis, (p0, size, bars, pixels, measured) in profiles(image, gname, x, y, w, h).items():
			L, R = snap(p0), snap(p0 + size)
			if "--fit" in sys.argv:
				err, k, c = fit_free(p0, size, bars, pixels, measured)
				print(f"{gname:6} {axis} p0 {p0:8.2f} size {size:7.2f} quad {L}..{R} ({R - L})  k {k:.5f}  c {c:+.3f}  (N/Q: {(R - L + 1) / (R - L):.5f})  max err {err * 255:.1f}")
			if "--model" in sys.argv and gname not in ("Clear", "Faded"):
				Q = R - L
				N = texture_size(Q, "logical")
				inside = (pixels >= L) & (pixels < R)
				line = f"{gname:6} {axis} Q {Q} N {N}:"
				for model, make in MODELS.items():
					if make == "screen":
						content, sampling = ("screen", L, N / Q), (L, N / Q)
					else:
						content, sampling = make(p0, size, L, Q, N)
					pred = model_profile(p0, size, bars, pixels[inside], content, sampling, N)
					err = np.abs(np.floor(pred * 255 + 0.5) - measured[inside] * 255)
					totals[model][0] += int((err > 1).sum())
					totals[model][1] += err.size
					line += f"  {int((err > 1).sum()):3d}/{err.size} max {err.max():5.1f}"
				print(line)
	if "--model" in sys.argv:
		for model, (bad, count) in totals.items():
			print(f"{model:40} {bad} of {count} pixels off by more than 1")


if __name__ == "__main__":
	main()
