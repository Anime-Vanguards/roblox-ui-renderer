# Reproduces the image sampling measurements behind the renderer's image drawing (native/src/render/images.rs) from Studio captures of
# the image probes (run `rur compare` for the fixture first; out/compare/<fixture>/ must exist).
#
#   python tools/analysis/image_fit.py arith     fixtures/image-probe-mag    bilinear weights + result precision
#   python tools/analysis/image_fit.py mips      fixtures/image-probe-min    mip generation, trilinear LOD
#   python tools/analysis/image_fit.py alpha     fixtures/image-probe-alpha  straight vs premultiplied, blend
#   python tools/analysis/image_fit.py slice     fixtures/image-probe-slice  9-slice centre rule
#   python tools/analysis/image_fit.py rotation  fixtures/image-rotation     LOD of rotated quads
#   python tools/analysis/image_fit.py dpi       fixtures/image-probe-dpi    @2x file selection
import math
import os
import sys
from collections import Counter

import numpy as np

import image_model as M

RUNS = os.path.join(M.ROOT, "out", "compare")


def run(name):
	return M.load_run(os.path.join(RUNS, name))


def image_rows(rows, scene, predicate=None):
	nodes = {n["path"].split(".")[-1]: n for n in scene["nodes"]}
	for row in rows:
		name = row["path"].split("/")[-1]
		node = nodes.get(name)
		if node and node.get("image") and (predicate is None or predicate(node)):
			yield name, node, row


def blend(s, background):
	"""s: straight RGBA in 0..1 -> 8-bit RGB over an opaque background (Studio's blend, one rounding)."""
	a = s[..., 3:4]
	return np.floor((s[..., :3] * a + np.asarray(background, float) / 255 * (1 - a)) * 255 + 0.5)


# Bilinear arithmetic: pixel-centre UVs over the snapped quad, 8-bit weights, 12-bit truncated result
def arith():
	studio, _, rows, scene = run("image-probe-mag")
	samples = {"a": [], "b": [], "c": [], "d": [], "fx": [], "fy": [], "act": []}
	for name, node, row in image_rows(rows, scene, lambda n: n["image"].get("resample") != "Pixelated"):
		tex = M.load_texture(node["image"]["uri"])
		th, tw = tex.shape[:2]
		L, T, R, B = M.snapped(row["studio"])
		W, H = R - L, B - T
		if tw / W > 1 or th / H > 1:
			continue
		u = (np.arange(W) + 0.5) / W * tw - 0.5
		v = (np.arange(H) + 0.5) / H * th - 0.5
		U, V = np.meshgrid(u, v)
		x0, y0 = np.floor(U).astype(int), np.floor(V).astype(int)
		inner = (slice(1, H - 1), slice(1, W - 1))
		for ch in range(3):
			t = tex[..., ch]
			samples["a"].append(t[np.clip(y0, 0, th - 1), np.clip(x0, 0, tw - 1)][inner].ravel())
			samples["b"].append(t[np.clip(y0, 0, th - 1), np.clip(x0 + 1, 0, tw - 1)][inner].ravel())
			samples["c"].append(t[np.clip(y0 + 1, 0, th - 1), np.clip(x0, 0, tw - 1)][inner].ravel())
			samples["d"].append(t[np.clip(y0 + 1, 0, th - 1), np.clip(x0 + 1, 0, tw - 1)][inner].ravel())
			samples["fx"].append((U - x0)[inner].ravel())
			samples["fy"].append((V - y0)[inner].ravel())
			samples["act"].append(studio[T:B, L:R, ch][inner].ravel())
	a, b, c, d, fx, fy, act = [np.concatenate(samples[k]) for k in ("a", "b", "c", "d", "fx", "fy", "act")]
	varying = ~((a == b) & (b == c) & (c == d))
	print(f"{varying.sum()} varying channel samples from magnified cells")
	for bits in [6, 8, 10, None]:
		if bits:
			wx, wy = np.floor(fx * 2**bits + 0.5) / 2**bits, np.floor(fy * 2**bits + 0.5) / 2**bits
		else:
			wx, wy = fx, fy
		P = (a + (b - a) * wx) + ((c + (d - c) * wx) - (a + (b - a) * wx)) * wy
		for label, out in [("round", np.floor(P + 0.5)), ("trunc12", M.trunc12(P))]:
			ok = out[varying] == act[varying]
			print(f"  weights {str(bits or 'exact'):>5} bits, {label:>7}: bit-exact {100 * ok.mean():.2f}%")


# Mip generation and trilinear sampling
def mips():
	studio, _, rows, scene = run("image-probe-min")
	schemes = {
		"separable h-then-v, round half up": lambda t: M.mip_chain(t),
		"separable, NPOT rounded half up": lambda t: M.mip_chain(t, npot_round="round"),
	}
	for label, make in schemes.items():
		chains = {}
		tally = Counter()
		for name, node, row in image_rows(rows, scene):
			uri = node["image"]["uri"]
			if uri not in chains:
				chains[uri] = make(M.load_texture(uri))
			L, T, R, B = M.snapped(row["studio"])
			pred = M.sample_quad(chains[uri], R - L, B - T, tri_round=False)
			out = blend(np.floor(pred / 255 * 4096) / 4096, [20, 20, 26])
			e = np.abs(out - studio[T:B, L:R]).max(axis=-1)
			tally["n"] += e.size
			tally["e0"] += int((e == 0).sum())
			tally["e2"] += int((e <= 2).sum())
		print(f"  {label:>36}: within 2 {100 * tally['e2'] / tally['n']:.3f}%  bit-exact {100 * tally['e0'] / tally['n']:.2f}%")


# Straight vs premultiplied filtering on soft-alpha textures
def alpha():
	studio, _, rows, scene = run("image-probe-alpha")
	background = np.array([230, 120, 40.0])
	tally = Counter()
	for name, node, row in image_rows(rows, scene):
		tex = M.load_texture(node["image"]["uri"])
		pre = tex.copy()
		pre[..., :3] = np.floor(tex[..., :3] * tex[..., 3:4] / 255 + 0.5)
		L, T, R, B = M.snapped(row["studio"])
		act = studio[T:B, L:R]
		straight = np.floor(M.sample_quad(M.mip_chain(tex), R - L, B - T, tri_round=False) / 255 * 4096) / 4096
		premul = np.floor(M.sample_quad(M.mip_chain(pre), R - L, B - T, tri_round=False) / 255 * 4096) / 4096
		out_s = blend(straight, background)
		a = premul[..., 3:4]
		out_p = np.floor((premul[..., :3] + background / 255 * (1 - a)) * 255 + 0.5)
		for label, out in [("straight", out_s), ("premultiplied", out_p)]:
			e = np.abs(out - act).max(axis=-1)
			tally[label + "n"] += e.size
			tally[label] += int((e <= 2).sum())
	for label in ["straight", "premultiplied"]:
		print(f"  {label:>14} filtering: within 2 {100 * tally[label] / tally[label + 'n']:.3f}%")


# 9-slice: borders scale by SliceScale x DPI x UIScale, shrink uniformly when they overflow; the centre is
# stretched down to one texel per logical pixel and cropped below that
def slice_quads(node, row, centre_rule):
	image = node["image"]
	tex = M.load_texture(image["uri"])
	ox, oy = image["rectOffset"]
	rw, rh = image["rectSize"]
	rw, rh = (rw or tex.shape[1]), (rh or tex.shape[0])
	x, y, w, h = [v * M.SCALE for v in row["studio"]]
	cx0, cy0, cx1, cy1 = image["sliceCenter"]
	k = (image.get("sliceScale") or 1) * M.SCALE * (node.get("scale") or 1)
	left, top, right, bottom = cx0, cy0, rw - cx1, rh - cy1
	shrink = min(1.0, w / ((left + right) * k) if (left + right) * k > w else 1.0, h / ((top + bottom) * k) if (top + bottom) * k > h else 1.0)
	snap = lambda v: math.floor(v + 0.5 + 1e-7)

	def axis(p, length, a, b, size, origin, extent):
		edges = [p, p + a * k * shrink, p + length - b * k * shrink, p + length]
		centre = size - a - b
		if centre_rule == "crop":
			centre = min(centre, max(0, snap(edges[2]) - snap(edges[1])) / M.SCALE)
		spans = [(0, a), (a, a + centre), (size - b, size)]
		return [(snap(edges[i]), snap(edges[i + 1]), (origin + spans[i][0]) / extent, (origin + spans[i][1]) / extent) for i in range(3)]

	return tex, axis(x, w, left, right, rw, ox, tex.shape[1]), axis(y, h, top, bottom, rh, oy, tex.shape[0])


def slice_():
	for fixture in ["image-probe-slice", "image-slice"]:
		studio, _, rows, scene = run(fixture)
		for rule in ["stretch", "crop"]:
			tally = Counter()
			for name, node, row in image_rows(rows, scene, lambda n: n["image"].get("scaleType") == "Slice" and n["image"].get("resample") != "Pixelated"):
				tex, xs, ys = slice_quads(node, row, rule)
				chain = M.mip_chain(tex)
				L, T, R, B = M.snapped(row["studio"])
				canvas = np.tile(np.array([60, 60, 72.0]), (B - T, R - L, 1))
				for (y0, y1, v0, v1) in ys:
					for (x0, x1, u0, u1) in xs:
						if x1 <= x0 or y1 <= y0:
							continue
						texels = max(tex.shape[1] * abs(u1 - u0) / (x1 - x0), tex.shape[0] * abs(v1 - v0) / (y1 - y0))
						lod = math.log2(texels) if texels > 0 else 0.0
						p = M.sample_quad(chain, x1 - x0, y1 - y0, u0, v0, u1, v1, lod=max(lod, 0.0), tri_round=False)
						s = np.floor(p / 255 * 4096) / 4096 * np.array([*(node["image"].get("color") or [1, 1, 1]), 1 - (node["image"].get("transparency") or 0)])
						ya, yb, xa, xb = max(y0, T), min(y1, B), max(x0, L), min(x1, R)
						if ya < yb and xa < xb:
							canvas[ya - T:yb - T, xa - L:xb - L] = blend(s, [60, 60, 72])[ya - y0:yb - y0, xa - x0:xb - x0]
				e = np.abs(canvas - studio[T:B, L:R]).max(axis=-1)
				tally["n"] += e.size
				tally["ok"] += int((e <= 2).sum())
				tally["cells"] += 1
				tally["exact_cells"] += int((e <= 2).all())
				if not (e <= 2).all() and rule == "crop":
					print(f"    {name}: {100 * (e <= 2).mean():.2f}% within 2")
			print(f"  {fixture:>18} centre {rule:>7}: within 2 {100 * tally['ok'] / tally['n']:.3f}%, {tally['exact_cells']}/{tally['cells']} cells fully within 2")


# LOD of rotated quads: GPU derivative length approximated as max + k * min of the components
def rotation():
	studio, _, rows, scene = run("image-rotation")
	nodes = {n["path"].split(".")[-1]: n for n in scene["nodes"]}
	for k in [None, 0.25, 0.375, 0.5]:
		tally = Counter()
		for name in ["Photo15", "Photo60", "PhotoTiny"]:
			node = nodes[name]
			chain = M.mip_chain(M.load_texture(node["image"]["uri"]))
			th, tw = chain[0].shape[:2]
			x, y, w, h = node["rect"]
			a, b, c, d, e, f = node["matrix"]
			det = a * d - b * c
			dudx, dvdx = (d / det) / (w * M.SCALE) * tw, (-b / det) / (h * M.SCALE) * th
			dudy, dvdy = (-c / det) / (w * M.SCALE) * tw, (a / det) / (h * M.SCALE) * th
			length = (lambda p, q: math.hypot(p, q)) if k is None else (lambda p, q: max(abs(p), abs(q)) + k * min(abs(p), abs(q)))
			lod = math.log2(max(length(dudx, dvdx), length(dudy, dvdy)))
			corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
			sx = [(a * px + c * py + e) * M.SCALE for px, py in corners]
			sy = [(b * px + d * py + f) * M.SCALE for px, py in corners]
			X0, X1, Y0, Y1 = int(min(sx)), int(max(sx)), int(min(sy)), int(max(sy))
			XX, YY = np.meshgrid(np.arange(X0, X1) + 0.5, np.arange(Y0, Y1) + 0.5)
			gx, gy = XX / M.SCALE - e, YY / M.SCALE - f
			u = ((d * gx - c * gy) / det - x) / w
			v = ((-b * gx + a * gy) / det - y) / h
			inside = (u > 0.02) & (u < 0.98) & (v > 0.02) & (v < 0.98)
			l0 = int(lod)
			fr = round((lod - l0) * 256) / 256
			p = M.bilinear(chain[l0], u * chain[l0].shape[1], v * chain[l0].shape[0])
			p = p + (M.bilinear(chain[l0 + 1], u * chain[l0 + 1].shape[1], v * chain[l0 + 1].shape[0]) - p) * fr
			out = blend(np.floor(p / 255 * 4096) / 4096, [20, 20, 26])
			err = np.abs(out - studio[Y0:Y1, X0:X1]).max(axis=-1)[inside]
			tally["n"] += err.size
			tally["ok"] += int((err <= 2).sum())
		label = "euclidean" if k is None else f"max + {k} min"
		print(f"  {label:>14}: interior within 2 {100 * tally['ok'] / tally['n']:.3f}%")


# High-DPI screens: which file of a built-in texture Studio samples
def dpi():
	studio, _, rows, scene = run("image-probe-dpi")
	rowmap = {r["path"].split("/")[-1]: r for r in rows}
	for name, base in [("Robux16", "ui/common/robux_small"), ("Robux64", "ui/common/robux_small"), ("Button55", "ui/Settings/Help/XButtonLight")]:
		L, T, R, B = M.snapped(rowmap[name]["studio"])
		scores = []
		for suffix in ["", "@2x", "@3x"]:
			try:
				chain = M.mip_chain(M.load_texture(f"rbxasset://textures/{base}{suffix}.png"))
			except FileNotFoundError:
				continue
			out = blend(np.floor(M.sample_quad(chain, R - L, B - T, tri_round=False) / 255 * 4096) / 4096, [40, 90, 200])
			e = np.abs(out - studio[T:B, L:R]).max(axis=-1)
			scores.append(f"{suffix or '1x'} {100 * (e <= 2).mean():.1f}%")
		print(f"  {name:>9}: " + "  ".join(scores))


if __name__ == "__main__":
	commands = {"arith": arith, "mips": mips, "alpha": alpha, "slice": slice_, "rotation": rotation, "dpi": dpi}
	for command in sys.argv[1:] or list(commands):
		print(command)
		commands[command]()
