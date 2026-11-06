"""Per-glyph advances of a string under the rur text model (for probing TextBounds rules).

    python tools/analysis/layout_advances.py <font file> <S (physical px size)> "<text>"

Prints each glyph's HarfBuzz advance in pixels (ppem = (S + 1) * upm / (asc - desc)), the 26.6 and
whole-pixel rounded values, and the totals under several accumulation models.
"""
import glob
import math
import os
import sys

import uharfbuzz as hb
from fontTools.ttLib import TTFont


def fonts_dir():
    return glob.glob(os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "*", "content", "fonts"))[0]


def main():
    path, size, text = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    if not os.path.exists(path):
        path = os.path.join(fonts_dir(), path)
    tt = TTFont(path)
    upm = tt["head"].unitsPerEm
    ascender, descender = tt["hhea"].ascent, tt["hhea"].descent
    ppem = (size + 1) * upm / (ascender - descender)
    scale = ppem / upm
    blob = hb.Blob.from_file_path(path)
    font = hb.Font(hb.Face(blob))
    buffer = hb.Buffer()
    buffer.add_str(text)
    buffer.guess_segment_properties()
    hb.shape(font, buffer, {})
    exact = fixed = rounded = 0.0
    for info, pos in zip(buffer.glyph_infos, buffer.glyph_positions):
        advance = pos.x_advance * scale
        advance26 = math.floor(advance * 64 + 0.5) / 64
        pixel = math.floor(advance26 + 0.5)
        exact += advance
        fixed += advance26
        rounded += pixel
        print(f"  {text[info.cluster]!r:6} {advance:8.4f} {advance26:8.4f} {pixel:4d}")
    print(f"ppem {ppem:.4f} exact {exact:.4f} fixed26.6 {fixed:.4f} rounded {rounded:.0f}")


if __name__ == "__main__":
    main()
