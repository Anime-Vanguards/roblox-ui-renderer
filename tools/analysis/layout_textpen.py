"""Measures where Studio puts the horizontal text pen (fixtures/layout-textpen.luau).

Every label's text starts with an isolated "I" stem. For each label the column-intensity centroid of
that stem is measured in our render and in the Studio capture; since both draw the same glyph, the
centroid difference is the pen difference (with sub-pixel precision if Studio renders at fractional
positions). Prints one row per label with the exported line start (line.x, device px) and the
fractional Studio pen next to ours.

  node cli/rur.mjs compare --fixture fixtures/layout-textpen.luau --viewport 1200x420 --time 0.3
  python tools/analysis/layout_textpen.py [out/compare/layout-textpen] [--csv file]

--glyphs (fixtures/layout-textpen3.luau) measures every glyph's Studio offset from the line start
against the exported glyph x.

--labels checks any compare directory rendered with exported line.px: for every text node it finds
the whole-pixel shift (dx, dy) that best matches Studio (as tools/analysis/text_baseline.py does) and
derives the shift the old placement (round(line.x)) had, so it lists labels that line.px fixed, kept,
or made worse:

  python tools/analysis/layout_textpen.py out/compare/<name> --labels
"""

import json
import math
import sys
from pathlib import Path

from PIL import Image


def load(path):
	image = Image.open(path).convert("RGB")
	return image, image.load()


def profile(pixels, x0, x1, y0, y1, background):
	values = []
	for x in range(x0, x1):
		total = 0.0
		for y in range(y0, y1):
			r, g, b = pixels[x, y]
			total += max(0.0, (r + g + b) / 3 - background)
		values.append(total)
	return values


def first_run_centroid(values, x0):
	# centroid of the first run of ink columns (the "I" stem)
	peak = max(values) if values else 0
	if peak <= 0:
		return None
	threshold = peak * 0.05
	start = next(i for i, v in enumerate(values) if v > threshold)
	end = start
	while end < len(values) and values[end] > threshold:
		end += 1
	weight = sum(values[start:end])
	return x0 + sum((start + i + 0.5) * v for i, v in enumerate(values[start:end])) / weight


SEARCH = 2


def mismatch(ours, studio, box, dx, dy, size):
	width, height = size
	x0, y0, x1, y1 = box
	count = 0
	for y in range(y0, y1):
		sy = y + dy
		if sy < 0 or sy >= height:
			continue
		for x in range(x0, x1):
			sx = x + dx
			if sx < 0 or sx >= width:
				continue
			a = ours[x, y]
			b = studio[sx, sy]
			if max(abs(a[0] - b[0]), abs(a[1] - b[1]), abs(a[2] - b[2])) > 8:
				count += 1
	return count


def labels(folder):
	scene = json.loads((folder / "rur" / "scene.json").read_text(encoding="utf8"))
	scale = scene["viewport"]["scale"]
	ours_image, ours = load(folder / "rur" / "render.png")
	_, studio = load(folder / "studio.png")
	size = ours_image.size
	counts = {"fixed": 0, "kept": 0, "worse": 0, "still": 0, "moved": 0}
	for node in scene["nodes"]:
		text = node.get("text")
		if not text or not text.get("lines") or not text["lines"][0].get("glyphs") or node.get("visible") is False:
			continue
		line = text["lines"][0]
		if "px" not in line:
			continue
		shift = line["px"] - math.floor(line["x"] + 0.5)
		x, y, w, h = node["rect"]
		box = (
			max(0, math.floor(x * scale) - SEARCH),
			max(0, math.floor(y * scale) - SEARCH),
			min(size[0], math.ceil((x + w) * scale) + SEARCH),
			min(size[1], math.ceil((y + h) * scale) + SEARCH),
		)
		if box[2] <= box[0] or box[3] <= box[1]:
			continue
		best = None
		for dy in range(-SEARCH, SEARCH + 1):
			for dx in range(-SEARCH, SEARCH + 1):
				count = mismatch(ours, studio, box, dx, dy, size)
				if best is None or count < best[0] or (count == best[0] and abs(dx) + abs(dy) < abs(best[1]) + abs(best[2])):
					best = (count, dx, dy)
		after = best[1]
		before = after + shift
		if shift != 0:
			counts["moved"] += 1
		if before != 0 and after == 0:
			state = "fixed"
		elif before == 0 and after == 0:
			state = "kept"
		elif abs(after) > abs(before):
			state = "worse"
		else:
			state = "still"
		counts[state] += 1
		if state != "kept":
			print(f"{state:5s} {node['path'][-60:]:60s} x {line['x']:9.3f} px {line['px']:5d} width {line['width']:4d} dx before {before:+d} after {after:+d} (dy {best[2]:+d}, mismatch {best[0]})")
	print(f"{folder.name}: {counts}")


def glyphs(folder):
	# fixtures/layout-textpen3.luau: Studio's pen offset of every glyph (an "I" stem) relative to the
	# line start, against the exported glyph x (the sum of the whole-pixel advances before it).
	scene = json.loads((folder / "rur" / "scene.json").read_text(encoding="utf8"))
	_, ours = load(folder / "rur" / "render.png")
	_, studio = load(folder / "studio.png")
	count, matched = 0, 0
	for node in scene["nodes"]:
		text = node.get("text")
		name = node.get("name", "")
		if not text or name.count("_") != 2 or not text.get("lines"):
			continue
		line = text["lines"][0]
		content_x, content_y, content_w, content_h = text["content"]
		y0, y1 = int(math.floor(content_y)), int(math.ceil(content_y + content_h))
		pen = line.get("px", math.floor(line["x"] + 0.5))
		row = []
		for glyph in line["glyphs"]:
			x = int(pen + glyph[2])
			x0, x1 = x - 3, x + int(glyph[3] * 0.35) + 3
			background = min(sum(ours[x0, y]) / 3 for y in range(y0, y1))
			c_o = first_run_centroid(profile(ours, x0, x1, y0, y1, background), x0)
			c_s = first_run_centroid(profile(studio, x0, x1, y0, y1, background), x0)
			if c_o is None or c_s is None:
				continue
			relative = glyph[2] + (c_s - c_o)
			count += 1
			if abs(relative - glyph[2]) < 0.3:
				matched += 1
			row.append(f"{glyph[2]}->{relative:.2f}")
		print(f"{name:16s} " + "  ".join(row))
	print(f"{count} glyphs; {matched} at the exported x (sum of whole-pixel advances)")


def main():
	if "--glyphs" in sys.argv:
		for arg in sys.argv[1:]:
			if not arg.startswith("--"):
				glyphs(Path(arg))
		return
	if "--labels" in sys.argv:
		for arg in sys.argv[1:]:
			if not arg.startswith("--"):
				labels(Path(arg))
		return
	args = [a for a in sys.argv[1:] if not a.startswith("--")]
	folder = Path(args[0] if args else "out/compare/layout-textpen")
	csv_path = sys.argv[sys.argv.index("--csv") + 1] if "--csv" in sys.argv else None
	scene = json.loads((folder / "rur" / "scene.json").read_text(encoding="utf8"))
	_, ours = load(folder / "rur" / "render.png")
	_, studio = load(folder / "studio.png")

	rows = []
	for node in scene["nodes"]:
		text = node.get("text")
		name = node.get("name", "")
		if not text or not text.get("lines") or name.count("_") != 3:
			continue
		alignment, fraction, width, text_index = name.split("_")
		line = text["lines"][0]
		if not line.get("glyphs"):
			continue
		content_x, content_y, content_w, content_h = text["content"]
		line_x = line["x"]
		pen = line.get("px", math.floor(line_x + 0.5)) + line["glyphs"][0][2]
		x0, x1 = int(pen) - 5, int(pen) + 13
		y0, y1 = int(math.floor(content_y)), int(math.ceil(content_y + content_h))
		background = min(min(sum(ours[x0, y]) / 3 for y in range(y0, y1)), min(sum(studio[x0, y]) / 3 for y in range(y0, y1)))
		ours_centroid = first_run_centroid(profile(ours, x0, x1, y0, y1, background), x0)
		studio_centroid = first_run_centroid(profile(studio, x0, x1, y0, y1, background), x0)
		if ours_centroid is None or studio_centroid is None:
			continue
		studio_pen = pen + (studio_centroid - ours_centroid)
		rows.append({
			"name": name,
			"align": alignment,
			"fraction": fraction,
			"width": width,
			"text": text_index,
			"contentX": content_x,
			"contentW": content_w,
			"lineW": line["width"],
			"lineX": line_x,
			"oursPen": pen,
			"studioPen": studio_pen,
			"dx": studio_pen - pen,
		})

	rows.sort(key=lambda r: (r["align"], r["fraction"], r["width"], r["text"]))
	for r in rows:
		print(f"{r['name']:22s} content.x {r['contentX']:9.3f} w {r['contentW']:7.2f} line.w {r['lineW']:4d} line.x {r['lineX']:9.3f} ours {r['oursPen']:6.0f} studio {r['studioPen']:9.3f} dx {r['dx']:+6.3f}")
	if csv_path:
		with open(csv_path, "w", encoding="utf8") as handle:
			json.dump(rows, handle, indent=1)


if __name__ == "__main__":
	main()
