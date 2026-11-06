"""Check the (S+1) ppem + FreeType LIGHT advances + HarfBuzz kerning model on full strings."""
import json, math, os, sys
import freetype
import uharfbuzz as hb

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
        self.ft = freetype.Face(path)
        blob = hb.Blob.from_file_path(path)
        self.hbface = hb.Face(blob)
        self.hbfont = hb.Font(self.hbface)
        self.upm = self.ft.units_per_EM
        self.L = self.ft.ascender - self.ft.descender

cache = {}
def fnt(key):
    if FILES[key] not in cache:
        cache[FILES[key]] = Fnt(os.path.join(V, FILES[key]))
    return cache[FILES[key]]

def measure(F, text, S, kern_mode, extra=1):
    ppem = (S + extra) * F.upm / F.L
    ppem26 = round(ppem * 64)
    F.ft.set_char_size(ppem26, ppem26, 72, 72)
    buf = hb.Buffer(); buf.add_str(text); buf.guess_segment_properties()
    F.hbfont.scale = (F.upm, F.upm)
    hb.shape(F.hbfont, buf, {"kern": True, "liga": True})
    infos, poss = buf.glyph_infos, buf.glyph_positions
    total = 0
    for i, (info, pos) in enumerate(zip(infos, poss)):
        F.ft.load_glyph(info.codepoint, freetype.FT_LOAD_TARGET_LIGHT)
        adv = F.ft.glyph.advance.x / 64
        # HarfBuzz advance (font units) minus the unkerned advance = kerning (units)
        base = F.hbfont.get_glyph_h_advance(info.codepoint)
        kern_units = pos.x_advance - base
        kern_px = kern_units * ppem / F.upm
        if kern_mode == "round":
            kern_px = math.floor(kern_px + 0.5)
        elif kern_mode == "26.6":
            kern_px = round(kern_px * 64) / 64
        elif kern_mode == "trunc":
            kern_px = math.trunc(kern_px)
        elif kern_mode == "none":
            kern_px = 0
        total += adv + kern_px
    return total

def check(path, scale, strings_key="strings"):
    data = json.load(open(path))
    results = {}
    for kern_mode in ["none", "exact", "round", "26.6", "trunc"]:
        for total_mode in ["ceil", "round"]:
            hits = n = 0
            misses = []
            for row in data["rows"]:
                key = (row["font"], row.get("weight", "Regular"))
                if key not in FILES:
                    continue
                F = fnt(key)
                if "S" in row:
                    TS = math.floor((row["S"] + 0.25) / scale)
                else:
                    TS = math.floor(row["size"])
                S = math.floor(TS * scale + 1e-9)
                for text, w in zip(data[strings_key], row["widths"]):
                    target = w if "S" in row else round(w * scale)
                    got = measure(F, text, S, kern_mode)
                    got = math.ceil(got - 1e-6) if total_mode == "ceil" else math.floor(got + 0.5)
                    n += 1
                    if got == target:
                        hits += 1
                    elif len(misses) < 12:
                        misses.append((key[0], key[1], TS, text, target, got))
            results[(kern_mode, total_mode)] = (hits, n, misses)
    for k, (h, n, m) in sorted(results.items(), key=lambda kv: -kv[1][0])[:3]:
        print(k, f"{h}/{n}")
        for x in m:
            print("    miss", x)

check(sys.argv[1], float(sys.argv[2]))
