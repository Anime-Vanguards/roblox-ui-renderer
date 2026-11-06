# Image sampling model for ImageLabel parity: loads textures and Studio captures (rur compare runs)
# and predicts the pixels Studio draws for an image quad.
#
# Measured (image_fit_*.py on fixtures/image-probe-*.luau):
#   - the quad covers the snapped rect [floor(x+0.5)...); UV runs 0..1 across the snapped rect
#     (pixel centre (i + 0.5) / W), clamp-to-edge addressing, hard edges
#   - bilinear with 8-bit weights round(frac * 256); result truncated to 12 bits (x/255 -> floor(x*4096)/4096)
#     then rounded to 8 bits
#   - minification: trilinear over a mip chain, LOD = log2(max(texW / W, texH / H)) (no anisotropy)
#   - mip k+1 = separable 2:1 reduction of mip k, horizontal pass first, each pass rounded half up
import json
import math
import os
import numpy as np
from PIL import Image

SCALE = 1.5
ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
VERSIONS = os.path.join(os.environ.get("LOCALAPPDATA", ""), "Roblox", "Versions")


def studio_root():
	best = None
	for name in os.listdir(VERSIONS):
		d = os.path.join(VERSIONS, name)
		if os.path.exists(os.path.join(d, "RobloxStudioBeta.exe")):
			if best is None or os.path.getmtime(d) > os.path.getmtime(best):
				best = d
	return best


def texture_path(uri):
	if uri.startswith("rbxasset://"):
		rel = uri[len("rbxasset://"):]
		root = studio_root()
		for base in ["content", "ExtraContent", os.path.join("PlatformContent", "pc")]:
			p = os.path.join(root, base, rel)
			if os.path.exists(p):
				return p
	if uri.startswith("rbxassetid://"):
		aid = uri[len("rbxassetid://"):]
		for ext in [".png", ".jpg"]:
			p = os.path.join(ROOT, ".cache", "assets", aid + ext)
			if os.path.exists(p):
				return p
	raise FileNotFoundError(uri)


def load_texture(uri):
	"""RGBA float64 array in 0..255, straight alpha."""
	return np.asarray(Image.open(texture_path(uri)).convert("RGBA"), dtype=np.float64)


def load_run(run):
	studio = np.asarray(Image.open(os.path.join(run, "studio.png")).convert("RGB"), dtype=np.float64)
	ours_path = os.path.join(run, "rur", "render.png")
	ours = np.asarray(Image.open(ours_path).convert("RGB"), dtype=np.float64) if os.path.exists(ours_path) else None
	layout = json.load(open(os.path.join(run, "layout-full.json")))
	scene = json.load(open(os.path.join(run, "rur", "scene.json")))
	return studio, ours, layout["rows"], scene


def snapped(rect, scale=SCALE):
	x, y, w, h = rect
	L, T = math.floor(x * scale + 0.5), math.floor(y * scale + 0.5)
	R, B = math.floor((x + w) * scale + 0.5), math.floor((y + h) * scale + 0.5)
	return L, T, R, B


# Mip chain

def half_axis(img, axis, npot_round="floor"):
	n = img.shape[axis]
	m = max(1, n // 2)
	if n == 1:
		return img
	if n == 2 * m:
		a = np.take(img, np.arange(m) * 2, axis=axis)
		b = np.take(img, np.arange(m) * 2 + 1, axis=axis)
		return np.floor((a + b + 1) / 2)
	pos = (np.arange(m) + 0.5) * n / m - 0.5
	i0 = np.clip(np.floor(pos).astype(int), 0, n - 1)
	i1 = np.clip(i0 + 1, 0, n - 1)
	f = pos - np.floor(pos)
	a = np.take(img, i0, axis=axis)
	b = np.take(img, i1, axis=axis)
	shape = [1] * img.ndim
	shape[axis] = m
	v = a + (b - a) * f.reshape(shape)
	return np.floor(v) if npot_round == "floor" else np.floor(v + 0.5)


def mip_chain(tex, npot_round="floor"):
	chain = [tex]
	cur = tex
	while cur.shape[0] > 1 or cur.shape[1] > 1:
		cur = half_axis(half_axis(cur, 1, npot_round), 0, npot_round)
		chain.append(cur)
	return chain


# Sampling

def trunc12(v):
	return np.floor(np.floor(v / 255 * 4096) / 4096 * 255 + 0.5)


def bilinear(tex, sx, sy, wrap="clamp", frac_bits=8, exact=False):
	"""Sample tex (H, W, C) at texel-space coordinates (texel centres at i + 0.5). Unrounded result."""
	h, w = tex.shape[:2]
	fx = sx - 0.5
	fy = sy - 0.5
	x0 = np.floor(fx)
	y0 = np.floor(fy)
	ax = fx - x0
	ay = fy - y0
	if frac_bits and not exact:
		q = float(1 << frac_bits)
		ax = np.floor(ax * q + 0.5) / q
		ay = np.floor(ay * q + 0.5) / q
	x0 = x0.astype(np.int64)
	y0 = y0.astype(np.int64)
	x1 = x0 + 1
	y1 = y0 + 1
	if wrap == "clamp":
		x0c, x1c = np.clip(x0, 0, w - 1), np.clip(x1, 0, w - 1)
		y0c, y1c = np.clip(y0, 0, h - 1), np.clip(y1, 0, h - 1)
	else:
		x0c, x1c = x0 % w, x1 % w
		y0c, y1c = y0 % h, y1 % h
	ax = ax[..., None]
	ay = ay[..., None]
	a = tex[y0c, x0c]
	b = tex[y0c, x1c]
	c = tex[y1c, x0c]
	d = tex[y1c, x1c]
	top = a + (b - a) * ax
	bot = c + (d - c) * ax
	return top + (bot - top) * ay


def nearest(tex, sx, sy):
	h, w = tex.shape[:2]
	x = np.clip(np.floor(sx).astype(np.int64), 0, w - 1)
	y = np.clip(np.floor(sy).astype(np.int64), 0, h - 1)
	return tex[y, x]


def lod_of(tw, th, W, H):
	rho = max(tw / W, th / H)
	return math.log2(rho) if rho > 0 else 0.0


def sample_quad(chain, W, H, u0=0.0, v0=0.0, u1=1.0, v1=1.0, lod=None, lod_bits=8, tri_round=True):
	"""Predicted (unquantized-then-quantized) RGBA for a W x H quad mapping UV [u0,u1]x[v0,v1]."""
	th, tw = chain[0].shape[:2]
	U = u0 + (np.arange(W) + 0.5) / W * (u1 - u0)
	V = v0 + (np.arange(H) + 0.5) / H * (v1 - v0)
	UU, VV = np.meshgrid(U, V)
	if lod is None:
		lod = lod_of(tw * (u1 - u0), th * (v1 - v0), W, H)
	lod = max(0.0, min(lod, len(chain) - 1))
	l0 = int(math.floor(lod))
	f = lod - l0
	if lod_bits:
		f = math.floor(f * (1 << lod_bits) + 0.5) / (1 << lod_bits)
	t0 = chain[l0]
	p0 = bilinear(t0, UU * t0.shape[1], VV * t0.shape[0])
	if f <= 0 or l0 + 1 >= len(chain):
		return trunc12(p0) if tri_round else p0
	t1 = chain[l0 + 1]
	p1 = bilinear(t1, UU * t1.shape[1], VV * t1.shape[0])
	p = p0 + (p1 - p0) * f
	return trunc12(p) if tri_round else p


def over(rgba, background):
	a = rgba[..., 3:4] / 255
	return np.floor(rgba[..., :3] * a + np.asarray(background, dtype=float) * (1 - a) + 0.5)


def compare_region(pred, actual):
	d = np.abs(pred - actual).max(axis=-1)
	return {
		"n": int(d.size),
		"exact0": float((d <= 0.5).mean()),
		"exact2": float((d <= 2).mean()),
		"within8": float((d <= 8).mean()),
		"maxdiff": float(d.max()) if d.size else 0.0,
		"mean": float(d.mean()) if d.size else 0.0,
	}
