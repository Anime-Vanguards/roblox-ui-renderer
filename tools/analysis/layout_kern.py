"""Evaluate per-glyph advance rounding models against Studio prefix widths.

    python tools/analysis/layout_kern.py out/probe/layout-probe-glyphs/studio.json

The probe (tools/studio/layout-probe-glyphs.luau) measures TextBounds of every prefix of a few
strings (suffixed with "|"). Each model computes the 26.6 advance of every glyph (HarfBuzz shaping
for kerning) and rounds it to whole pixels; the script prints how many prefixes each model matches.
"""
import glob
import json
import math
import os
import sys

import uharfbuzz as hb
from fontTools.ttLib import TTFont

CASES = {
    "SSB18": ("SourceSansPro-Bold.ttf", 18, "SourceSansBold Hamburgefonstiv 0123"),
    "Cre18": ("Creepster-Regular.ttf", 18, "Creepster Hamburgefonstiv 0123"),
    "Cre24": ("Creepster-Regular.ttf", 24, "The quick brown fox"),
    "SS14": ("SourceSansPro-Regular.ttf", 14, "Supercalifragilisticexpialidocious word"),
    "SS14b": ("SourceSansPro-Regular.ttf", 14, "one-two-three four-five"),
}


def fonts_dir():
    return glob.glob(os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "*", "content", "fonts"))[0]


def mulfix(a, b):
    sign = -1 if (a < 0) != (b < 0) else 1
    return sign * ((abs(a) * abs(b) + 0x8000) >> 16)


class Font:
    def __init__(self, file):
        path = os.path.join(fonts_dir(), file)
        tt = TTFont(path)
        self.upm = tt["head"].unitsPerEm
        self.ascender = tt["hhea"].ascent
        self.descender = tt["hhea"].descent
        self.hmtx = tt["hmtx"]
        self.order = tt.getGlyphOrder()
        self.hb = hb.Font(hb.Face(hb.Blob.from_file_path(path)))

    def shape(self, text):
        buffer = hb.Buffer()
        buffer.add_str(text)
        buffer.guess_segment_properties()
        hb.shape(self.hb, buffer, {})
        glyphs = []
        for info, pos in zip(buffer.glyph_infos, buffer.glyph_positions):
            nominal = self.hmtx[self.order[info.codepoint]][0]
            glyphs.append((nominal, pos.x_advance - nominal, pos.x_offset))
        return glyphs


def models(font, size):
    s = math.floor(math.floor(size) * 1.5 + 1e-9) if size == math.floor(size) else math.floor(size * 1.5 + 1e-9)
    d = font.ascender - font.descender
    line26 = (s + 1) * 64
    x_scale = (line26 * 65536 + d // 2) // d
    ppem = (s + 1) * font.upm / d
    scale = ppem / font.upm
    hb_scale = (x_scale * font.upm + 0x8000) >> 16

    def float_combined(nominal, kern, offset):
        return math.floor((nominal + kern) * scale * 64 + 0.5)

    def fix_separate(nominal, kern, offset):
        return mulfix(nominal, x_scale) + mulfix(kern, x_scale)

    def fix_combined(nominal, kern, offset):
        return mulfix(nominal + kern, x_scale)

    def hb_em(nominal, kern, offset):
        # HarfBuzz em_scale: v * scale / upem, rounded (hb font scale derived from x_scale)
        def em(value):
            return math.floor(value * hb_scale / font.upm + 0.5)
        return mulfix(nominal, x_scale) + em(kern)

    def fix_kern_floor(nominal, kern, offset):
        return mulfix(nominal, x_scale) + math.floor(kern * scale * 64)

    def hb_floor(nominal, kern, offset):
        # kerning scaled with the HarfBuzz font scale (x_scale * upem >> 16, rounded) and floored
        return mulfix(nominal, x_scale) + math.floor(kern * hb_scale / font.upm)

    def hb_trunc_scale_floor(nominal, kern, offset):
        truncated = (x_scale * font.upm) >> 16
        return mulfix(nominal, x_scale) + math.floor(kern * truncated / font.upm)

    def fix_kern_mulfix_floor(nominal, kern, offset):
        return mulfix(nominal, x_scale) + ((kern * x_scale) >> 16)

    return {
        "hb_floor": hb_floor,
        "hb_trunc_scale_floor": hb_trunc_scale_floor,
        "fix_kern_mulfix_floor": fix_kern_mulfix_floor,
        "float": float_combined,
        "fix_separate": fix_separate,
        "fix_combined": fix_combined,
        "hb_em": hb_em,
        "fix_kern_floor": fix_kern_floor,
    }


def pixel_sum(glyphs, model, rounding):
    total = 0
    for nominal, kern, offset in glyphs:
        advance26 = model(nominal, kern, offset)
        if rounding == "half_up":
            total += math.floor(advance26 / 64 + 0.5)
        elif rounding == "half_down":
            total += math.ceil(advance26 / 64 - 0.5)
        elif rounding == "sum26":
            total += advance26 / 64
    if rounding == "sum26":
        return math.floor(total + 0.5)
    return total


FAMILY_FILES = {
    ("Arimo", "Regular"): "Arimo-Regular.ttf",
    ("BuilderSans", "Bold"): "BuilderSans-Bold.otf",
    ("BuilderSans", "Regular"): "BuilderSans-Regular.otf",
    ("FredokaOne", "Regular"): "FredokaOne-Regular.ttf",
    ("GothamSSm", "Bold"): "Montserrat-Bold.ttf",
    ("GothamSSm", "Heavy"): "Montserrat-Black.ttf",
    ("GothamSSm", "Medium"): "Montserrat-Medium.ttf",
    ("GothamSSm", "Regular"): "Montserrat-Regular.ttf",
    ("LegacyArial", "Regular"): "Arimo-Regular.ttf",
    ("Montserrat", "Bold"): "Montserrat-Bold.ttf",
    ("Montserrat", "Regular"): "Montserrat-Regular.ttf",
    ("Roboto", "Regular"): "Roboto-Regular.ttf",
    ("SourceSansPro", "Bold"): "SourceSansPro-Bold.ttf",
    ("SourceSansPro", "Regular"): "SourceSansPro-Regular.ttf",
}


def bounds_dataset(path):
    """Scores the models on data/studio/text-bounds-1.5x.json (font x size x string widths)."""
    data = json.load(open(path))
    fonts = {}
    scores = {}
    misses = {}
    for row in data["rows"]:
        file = FAMILY_FILES[(row["font"], row["weight"])]
        font = fonts.setdefault(file, Font(file))
        size = row["size"] * (1.5 if row["font"] == "LegacyArial" else 1)
        for name, model in models(font, size).items():
            for rounding in ("half_up",):
                label = f"{name}/{rounding}"
                hits, total = scores.get(label, (0, 0))
                for index, text in enumerate(data["strings"]):
                    target = round(row["widths"][index] * 1.5)
                    value = pixel_sum(font.shape(text), model, rounding)
                    if value == target:
                        hits += 1
                    else:
                        misses.setdefault(label, []).append(f"{row['font']}-{row['weight']} {row['size']} {text!r} studio {target} model {value}")
                    total += 1
                scores[label] = (hits, total)
    for label, (hits, total) in sorted(scores.items(), key=lambda item: -item[1][0]):
        print(f"{label:28} {hits}/{total}")
        for miss in misses.get(label, [])[:8]:
            print("    " + miss)


def main():
    if sys.argv[1].endswith("text-bounds-1.5x.json"):
        bounds_dataset(sys.argv[1])
        return
    studio = json.load(open(sys.argv[1]))
    fonts = {}
    scores = {}
    for key, (file, size, text) in CASES.items():
        font = fonts.setdefault(file, Font(file))
        for name, model in models(font, size).items():
            for rounding in ("half_up", "half_down", "sum26"):
                label = f"{name}/{rounding}"
                hits, total = scores.get(label, (0, 0))
                for length in range(1, len(text) + 1):
                    target = round(studio[f"{key}_{length}"][0] * 1.5)
                    glyphs = font.shape(text[:length] + "|")
                    if pixel_sum(glyphs, model, rounding) == target:
                        hits += 1
                    total += 1
                scores[label] = (hits, total)
    for label, (hits, total) in sorted(scores.items(), key=lambda item: -item[1][0]):
        print(f"{label:28} {hits}/{total}")


if __name__ == "__main__":
    main()
