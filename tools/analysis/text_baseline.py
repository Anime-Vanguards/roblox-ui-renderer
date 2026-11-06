"""Measures how far each text label is shifted between our render and Studio.

For every text node of a compare directory, finds the integer (dx, dy) shift of our glyph pixels
that best matches the Studio capture and prints it with the font metrics that decide the baseline
(ascender, hhea lineGap, line top). A shape match with a non-zero shift means placement differs;
no zero-mismatch shift means the glyphs themselves differ.

usage: python text_baseline.py <out/compare/name> [--only-shifted] [--filter text]
"""

import json
import math
import sys
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image

SEARCH = 2


def load(path):
    image = Image.open(path).convert("RGB")
    return image, image.load()


def mismatch(ours, studio, box, dx, dy, width, height):
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


def main():
    folder = Path(sys.argv[1])
    only_shifted = "--only-shifted" in sys.argv
    name_filter = sys.argv[sys.argv.index("--filter") + 1].lower() if "--filter" in sys.argv else None

    scene = json.loads((folder / "rur" / "scene.json").read_text(encoding="utf8"))
    scale = scene["viewport"]["scale"]
    ours_image, ours = load(folder / "rur" / "render.png")
    _, studio = load(folder / "studio.png")
    width, height = ours_image.size

    metrics = {}
    rows = []
    for node in scene["nodes"]:
        text = node.get("text")
        if not text or not text.get("lines") or not text["lines"][0].get("glyphs"):
            continue
        if name_filter and name_filter not in node["path"].lower():
            continue
        line = text["lines"][0]
        font_index, _, _, ppem = line["glyphs"][0]
        font = scene["fonts"][font_index]
        if font["file"] not in metrics:
            tt = TTFont(font["file"], lazy=True)
            metrics[font["file"]] = (tt["head"].unitsPerEm, tt["hhea"].lineGap)
        upm, line_gap = metrics[font["file"]]

        x, y, w, h = node["rect"]
        box = (
            max(0, math.floor(x * scale) - SEARCH),
            max(0, math.floor(y * scale) - SEARCH),
            min(width, math.ceil((x + w) * scale) + SEARCH),
            min(height, math.ceil((y + h) * scale) + SEARCH),
        )
        best = None
        for dy in range(-SEARCH, SEARCH + 1):
            for dx in range(-SEARCH, SEARCH + 1):
                count = mismatch(ours, studio, box, dx, dy, width, height)
                if best is None or count < best[0] or (count == best[0] and abs(dx) + abs(dy) < abs(best[1]) + abs(best[2])):
                    best = (count, dx, dy)
        base_count = mismatch(ours, studio, box, 0, 0, width, height)
        row = {
            "name": node["name"],
            "dx": best[1],
            "dy": best[2],
            "mismatch": best[0],
            "unshifted": base_count,
            "top": line["top"],
            "ascPx": font["ascender"] * ppem / upm,
            "gapPx": line_gap * ppem / upm,
            "ppem": ppem,
            "size": text["textSize"],
            "yAlign": text.get("yAlign"),
        }
        rows.append(row)

    for row in rows:
        if only_shifted and row["dx"] == 0 and row["dy"] == 0:
            continue
        print(
            f"{row['name']:32s} shift ({row['dx']:+d},{row['dy']:+d}) mismatch {row['mismatch']:4d} (unshifted {row['unshifted']:4d})"
            f"  top {row['top']:8.3f} asc {row['ascPx']:7.3f} gap {row['gapPx']:5.2f} ppem {row['ppem']:7.3f}"
        )
    json.dump(rows, open(folder / "baseline.json", "w"), indent=1)


if __name__ == "__main__":
    main()
