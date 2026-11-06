"""Model: ppem=(S+1)*upm/(asc-desc); pen accumulates linear advance + GPOS kerning; each glyph
placed at round(pen); width = x_last + round(adv_last)."""
import json, math, os, sys
import uharfbuzz as hb
from fontTools.ttLib import TTFont

V = os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "version-3afcc74cf8b04b5d", "content", "fonts")
FILES = {
    ("SourceSansPro", "Regular"): "SourceSansPro-Regular.ttf", ("SourceSansPro", "Bold"): "SourceSansPro-Bold.ttf",
    ("FredokaOne", "Regular"): "FredokaOne-Regular.ttf",
    ("Montserrat", "Regular"): "Montserrat-Regular.ttf", ("Montserrat", "Bold"): "Montserrat-Bold.ttf",
    ("GothamSSm", "Regular"): "Montserrat-Regular.ttf", ("GothamSSm", "Bold"): "Montserrat-Bold.ttf",
    ("GothamSSm", "Medium"): "Montserrat-Medium.ttf", ("GothamSSm", "Heavy"): "Montserrat-Black.ttf",
    ("BuilderSans", "Regular"): "BuilderSans-Regular.otf", ("BuilderSans", "Bold"): "BuilderSans-Bold.otf",
    ("Roboto", "Regular"): "Roboto-Regular.ttf", ("Arimo", "Regular"): "Arimo-Regular.ttf",
}

class Fnt:
    def __init__(self, path):
        self.hbfont = hb.Font(hb.Face(hb.Blob.from_file_path(path)))
        tt = TTFont(path)
        self.upm = tt["head"].unitsPerEm
        self.L = tt["hhea"].ascent - tt["hhea"].descent

cache = {}
def fnt(key):
    if FILES[key] not in cache:
        cache[FILES[key]] = Fnt(os.path.join(V, FILES[key]))
    return cache[FILES[key]]

def rnd(x, mode):
    if mode == "round":
        return math.floor(x + 0.5)
    if mode == "floor":
        return math.floor(x + 1e-9)
    if mode == "ceil":
        return math.ceil(x - 1e-9)
    if mode == "none":
        return x

def measure(F, text, S, pos_mode, adv_mode, extra):
    ppem = (S + extra) * F.upm / F.L
    k = ppem / F.upm
    buf = hb.Buffer(); buf.add_str(text); buf.guess_segment_properties()
    F.hbfont.scale = (F.upm, F.upm)
    hb.shape(F.hbfont, buf, {})
    pen = 0.0
    x_last = 0
    adv_last = 0
    for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
        x_last = rnd(pen + pos.x_offset * k, pos_mode)
        adv_last = F.hbfont.get_glyph_h_advance(info.codepoint) * k
        pen += pos.x_advance * k
    return x_last + rnd(adv_last, adv_mode)

def check(path, scale):
    data = json.load(open(path))
    for pos_mode in ["round", "floor"]:
        for adv_mode in ["round", "ceil"]:
            for extra in [1, 0]:
                hits = n = 0
                misses = []
                perfont = {}
                for row in data["rows"]:
                    key = (row["font"], row.get("weight", "Regular"))
                    if key not in FILES: continue
                    F = fnt(key)
                    TS = math.floor((row["S"] + 0.25) / scale) if "S" in row else math.floor(row["size"])
                    S = math.floor(TS * scale + 1e-9)
                    for text, w in zip(data["strings"], row["widths"]):
                        target = w if "S" in row else round(w * scale)
                        got = measure(F, text, S, pos_mode, adv_mode, extra)
                        n += 1
                        pf = perfont.setdefault(key, [0, 0]); pf[1] += 1
                        if got == target:
                            hits += 1; pf[0] += 1
                        elif len(misses) < 10:
                            misses.append((key, TS, text, target, got))
                print(pos_mode, adv_mode, extra, f"{hits}/{n}", {f"{k[0]}-{k[1]}": f"{v[0]}/{v[1]}" for k, v in perfont.items()})
                if pos_mode == "round" and adv_mode == "round" and extra == 1:
                    for m in misses: print("    miss", m)

check(sys.argv[1], float(sys.argv[2]))
