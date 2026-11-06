"""Text-only parity of a Studio comparison: pixels inside the glyph boxes (fill and text-stroke
bitmaps, placed like the renderer places them, native/src/render/text.rs) of every text node, bit-exact / within 1 / 2.

usage: python tools/analysis/text_parity.py <out/compare/name>... [--rur <render dir>] [--render <png>]
         [--rerender] [--nodes N] [--grow 1]
  --rur       the render dir holding scene.json + glyphs.json (default <compare>/rur)
  --render    our PNG (default <rur>/render.png)
  --rerender  first re-render <compare>/rur/scene.json with the current renderer into out/text/<name>
              (renderer and rasterizer changes only; the scene is not re-run)
  --nodes N   list the N text nodes with the most differing pixels
"""

import json
import math
import os
import subprocess
import sys

import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))


def arg(name, default=None):
    if name in sys.argv:
        return sys.argv[sys.argv.index(name) + 1]
    return default


def snap(v):
    return math.floor(v + 0.5)


def atlas_index(atlas):
    """(font, ppem, glyph) -> every atlas entry of that glyph (fill, strokes, styles)."""
    index = {}
    for glyph in atlas["glyphs"]:
        key = glyph["key"]
        index.setdefault((key[0], key[1], key[2]), []).append(glyph)
    return index


def stroke_radius(thickness, scale):
    t = np.float32(np.float32(thickness) * np.float32(scale))
    return math.floor(math.floor(np.float32(t * np.float32(10))) / 10 * 64) / 64


def glyph_boxes(scene, atlas):
    """(node, [(x, y, w, h, entry), ...]) for every text node: its glyphs' fill and stroke bitmap boxes in
    device px (every atlas variant of the glyph), placed like native/src/render/text.rs places them."""
    fonts = scene.get("fonts") or []
    index = atlas_index(atlas)
    result = []
    for node in scene.get("nodes") or []:
        text = node.get("text")
        if not text or not text.get("lines"):
            continue
        boxes = []
        for line in text["lines"]:
            ox = line.get("px")
            if ox is None:
                ox = snap(line["x"])
            top = snap(line["top"])
            # the line's baseline: the first glyph of the largest pixel size
            ascent, largest = 0, -1e9
            for glyph in line.get("glyphs") or []:
                font = fonts[glyph[0]] if glyph[0] < len(fonts) else None
                if not font:
                    continue
                span = font["ascender"] - font["descender"]
                size = round(glyph[3] * span / font["unitsPerEm"]) - 1
                if size > largest:
                    largest = size
                    ascent = snap(font["ascender"] * glyph[3] / font["unitsPerEm"] * span / (span + (font.get("lineGap") or 0)))
            for glyph in line.get("glyphs") or []:
                font_index, glyph_id, gx, ppem = glyph[0], glyph[1], glyph[2], glyph[3]
                for entry in index.get((font_index, ppem, glyph_id), []):
                    if entry["w"] == 0:
                        continue
                    x = ox + snap(gx) + entry["left"]
                    y = top + ascent - entry["top"]
                    boxes.append((x, y, entry["w"], entry["h"], entry))
        if text.get("stroke"):
            boxes = [(x - 1, y - 1, w + 2, h + 2, None) for x, y, w, h, _ in boxes]
        result.append((node, boxes))
    return result


def crop_sheets(name, pattern, rows, studio, ours, diff, zoom):
    """Zoomed Studio | ours | diff sheets of the text nodes whose path contains `pattern`, written to
    out/text/crops/<compare>-<node>.png (window: the node's glyph boxes, at most 300 x 200 px)."""
    out = os.path.join(ROOT, "out", "text", "crops")
    os.makedirs(out, exist_ok=True)
    H, W = diff.shape
    for _, _, _, _, node in rows:
        if pattern not in node.get("path", node.get("name", "")):
            continue
        box = node.get("_box")
        if not box:
            continue
        x0, y0, x1, y1 = box
        x0, y0, x1, y1 = max(0, x0 - 3), max(0, y0 - 3), min(W, x1 + 3), min(H, y1 + 3)
        x1, y1 = min(x1, x0 + 300), min(y1, y0 + 200)
        a = studio[y0:y1, x0:x1].astype(np.uint8)
        b = ours[y0:y1, x0:x1].astype(np.uint8)
        local = diff[y0:y1, x0:x1]
        heat = np.zeros(a.shape, dtype=np.uint8)
        heat[..., 0] = np.clip(local * 40, 0, 255)
        heat[..., 1] = np.clip((local - 6) * 4, 0, 255)
        heat[..., 2] = np.where(local > 0, 60, 0)
        w, h = (x1 - x0) * zoom, (y1 - y0) * zoom
        sheet = Image.new("RGB", (w, h * 3 + 8), (255, 0, 255))
        for i, image in enumerate((a, b, heat)):
            sheet.paste(Image.fromarray(image).resize((w, h), Image.NEAREST), (0, i * (h + 4)))
        file = os.path.join(out, f"{name}-{node.get('name', 'node').replace('/', '_')}-{node['id']}.png")
        sheet.save(file)
        print("  crop", file, f"window x {x0}..{x1} y {y0}..{y1}")


def rerender(compare_dir):
    name = os.path.basename(os.path.normpath(compare_dir))
    out = os.path.join(ROOT, "out", "text", name)
    scene = os.path.join(compare_dir, "rur", "scene.json")
    subprocess.run(["node", os.path.join(ROOT, "cli", "rur.mjs"), "render", "--scene", scene, "--out", out, "--offline"],
                   check=True, cwd=ROOT, stdout=subprocess.DEVNULL)
    return out


def report(compare_dir, rur_dir, render_png, top, grow):
    scene = json.load(open(os.path.join(rur_dir, "scene.json"), encoding="utf8"))
    atlas_file = os.path.join(rur_dir, "glyphs.json")
    atlas = json.load(open(atlas_file, encoding="utf8")) if os.path.exists(atlas_file) else {"glyphs": []}
    studio = np.asarray(Image.open(os.path.join(compare_dir, "studio.png")).convert("RGB")).astype(int)
    ours = np.asarray(Image.open(render_png).convert("RGB")).astype(int)
    H = min(studio.shape[0], ours.shape[0])
    W = min(studio.shape[1], ours.shape[1])
    studio, ours = studio[:H, :W], ours[:H, :W]
    diff = np.abs(studio - ours).max(axis=2)

    atlas_png = os.path.join(rur_dir, "glyphs.png")
    atlas_alpha = np.asarray(Image.open(atlas_png).convert("RGBA"))[..., 3] if os.path.exists(atlas_png) else None
    mask = np.zeros((H, W), dtype=bool)
    covered = np.zeros((H, W), dtype=bool)
    rows = []
    for node, boxes in glyph_boxes(scene, atlas):
        node_mask = np.zeros((H, W), dtype=bool)
        for x, y, w, h, entry in boxes:
            if entry is not None and atlas_alpha is not None and x >= 0 and y >= 0 and x + w <= W and y + h <= H:
                covered[y:y + h, x:x + w] |= atlas_alpha[entry["y"]:entry["y"] + h, entry["x"]:entry["x"] + w] > 0
            x0, y0 = max(0, x - grow), max(0, y - grow)
            x1, y1 = min(W, x + w + grow), min(H, y + h + grow)
            if x1 > x0 and y1 > y0:
                node_mask[y0:y1, x0:x1] = True
        if node.get("clip"):
            cx, cy, cw, ch = node["clip"]
            S = scene["viewport"]["scale"]
            L, T, R, B = snap(cx * S), snap(cy * S), snap((cx + cw) * S), snap((cy + ch) * S)
            clip = np.zeros((H, W), dtype=bool)
            clip[max(0, T):max(0, B), max(0, L):max(0, R)] = True
            node_mask &= clip
        count = int(node_mask.sum())
        if count == 0:
            continue
        ys, xs = np.nonzero(node_mask)
        node["_box"] = (int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1)
        mask |= node_mask
        d = diff[node_mask]
        rows.append((int((d > 0).sum()), int((d > 2).sum()), int(d.max()), count, node))

    d = diff[mask]
    n = max(1, d.size)
    name = os.path.basename(os.path.normpath(compare_dir))
    dc = diff[mask & covered]
    crop = arg("--crop")
    if crop:
        crop_sheets(name, crop, rows, studio, ours, diff, int(arg("--zoom", "6")))
    print(f"{name}: text px {d.size}  bit-exact {100 * (d == 0).sum() / n:.3f}%  <=1 {100 * (d <= 1).sum() / n:.3f}%  "
          f"<=2 {100 * (d <= 2).sum() / n:.3f}%  max {int(d.max()) if d.size else 0}  "
          f"(whole image bit-exact {100 * (diff == 0).mean():.3f}%, <=2 {100 * (diff <= 2).mean():.3f}%)")
    print(f"  covered px {dc.size}  bit-exact {100 * (dc == 0).mean() if dc.size else 100:.3f}%  <=1 {100 * (dc <= 1).mean() if dc.size else 100:.3f}%  "
          f"<=2 {100 * (dc <= 2).mean() if dc.size else 100:.3f}%")
    if top:
        rows.sort(key=lambda row: -row[0])
        for off, bad, worst, count, node in rows[:top]:
            text = (node.get("props") or {}).get("text", "")
            strokes = [s for s in node.get("strokes") or [] if s.get("mode") == "Contextual"]
            extra = []
            if strokes:
                extra.append(f"stroke {strokes[0]['thickness']:.3g}")
            if node.get("gradient"):
                extra.append("gradient")
            if node["text"].get("transparency"):
                extra.append(f"T {node['text']['transparency']:.3g}")
            if node["text"].get("stroke"):
                extra.append("TextStroke")
            print(f"  {off:6d} off ({bad:5d} >2, max {worst:3d}) of {count:6d}  {node.get('path', node.get('name', '?'))[-60:]:60s} "
                  f"{json.dumps(text[:24])} size {node['text'].get('textSize')} {' '.join(extra)}")
    report.covered = dc
    return d


def main():
    dirs = []
    skip = False
    for a in sys.argv[1:]:
        if skip:
            skip = False
        elif a.startswith("--"):
            skip = a not in ("--rerender",)
        else:
            dirs.append(a)
    top = int(arg("--nodes", "0"))
    grow = int(arg("--grow", "1"))
    for compare_dir in dirs:
        rur_dir = arg("--rur", os.path.join(compare_dir, "rur"))
        if "--rerender" in sys.argv:
            out = rerender(compare_dir)
            report(compare_dir, out, os.path.join(out, "render.png"), top, grow)
        else:
            report(compare_dir, rur_dir, arg("--render", os.path.join(rur_dir, "render.png")), top, grow)


if __name__ == "__main__":
    main()
