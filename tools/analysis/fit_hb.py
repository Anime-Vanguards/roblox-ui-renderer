"""Fit Roblox TextBounds with HarfBuzz shaping under several scale/rounding models."""
import json, math, os, sys, itertools
import uharfbuzz as hb
from fontTools.ttLib import TTFont

V = os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "version-3afcc74cf8b04b5d", "content", "fonts")
FILES = {
    ("SourceSansPro", "Regular"): "SourceSansPro-Regular.ttf",
    ("SourceSansPro", "Bold"): "SourceSansPro-Bold.ttf",
    ("FredokaOne", "Regular"): "FredokaOne-Regular.ttf",
    ("Montserrat", "Regular"): "Montserrat-Regular.ttf",
    ("Montserrat", "Bold"): "Montserrat-Bold.ttf",
    ("BuilderSans", "Regular"): "BuilderSans-Regular.otf",
    ("BuilderSans", "Bold"): "BuilderSans-Bold.otf",
    ("Roboto", "Regular"): "Roboto-Regular.ttf",
    ("Arimo", "Regular"): "Arimo-Regular.ttf",
}

class FontData:
    def __init__(self, path):
        blob = hb.Blob.from_file_path(path)
        self.face = hb.Face(blob)
        self.font = hb.Font(self.face)
        tt = TTFont(path)
        self.upm = tt["head"].unitsPerEm
        self.asc = tt["hhea"].ascent
        self.desc = tt["hhea"].descent

    def shape(self, text, features=None):
        buf = hb.Buffer()
        buf.add_str(text)
        buf.guess_segment_properties()
        self.font.scale = (self.upm, self.upm)
        hb.shape(self.font, buf, features or {})
        return [(i.codepoint, p.x_advance, p.x_offset) for i, p in zip(buf.glyph_infos, buf.glyph_positions)]

cache = {}
def fontdata(key):
    name = FILES[key]
    if name not in cache:
        cache[name] = FontData(os.path.join(V, name))
    return cache[name]

def width(fd, text, S, model):
    em = S * fd.upm / (fd.asc - fd.desc)
    if model["em"] == "round":
        em = round(em)
    elif model["em"] == "floor":
        em = math.floor(em)
    elif model["em"] == "26.6":
        em = round(em * 64) / 64
    k = em / fd.upm
    glyphs = fd.shape(text, model.get("features"))
    total = 0.0
    for gid, adv, off in glyphs:
        px = adv * k
        if model["adv"] == "round":
            px = math.floor(px + 0.5)
        elif model["adv"] == "26.6":
            px = round(px * 64) / 64
        elif model["adv"] == "ceil":
            px = math.ceil(px - 1e-9)
        total += px
    if model["total"] == "ceil":
        total = math.ceil(total - 1e-6)
    elif model["total"] == "round":
        total = math.floor(total + 0.5)
    return total

def main():
    data = json.load(open(sys.argv[1]))
    scale = float(sys.argv[2])
    models = []
    for em in ["exact", "round", "floor", "26.6"]:
        for adv in ["none", "round", "26.6", "ceil"]:
            for total in ["ceil", "round"]:
                models.append({"em": em, "adv": adv, "total": total})
    per_font = {}
    misses = {}
    for row in data["rows"]:
        key = (row["font"], row["weight"])
        if key not in FILES:
            continue
        fd = fontdata(key)
        S = math.floor(row["size"] * scale + 1e-9)
        for text, w in zip(data["strings"], row["widths"]):
            target = round(w * scale)
            for mi, model in enumerate(models):
                got = width(fd, text, S, model)
                st = per_font.setdefault(key, {}).setdefault(mi, [0, 0])
                st[1] += 1
                if abs(got - target) < 0.01:
                    st[0] += 1
                else:
                    misses.setdefault((key, mi), []).append((row["size"], text, target, got))
    for key, st in per_font.items():
        ranked = sorted(st.items(), key=lambda kv: -kv[1][0])
        best = ranked[0]
        print(key, [(models[mi], f"{h}/{n}") for mi, (h, n) in ranked[:3]])
        print("   misses:", misses.get((key, best[0]), [])[:8])

main()
