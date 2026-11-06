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
cache = {}
def fnt(key):
    f = FILES[key]
    if f not in cache:
        tt = TTFont(os.path.join(V, f))
        cache[f] = (hb.Font(hb.Face(hb.Blob.from_file_path(os.path.join(V, f)))), tt["head"].unitsPerEm, tt["hhea"].ascent - tt["hhea"].descent)
    return cache[f]

def glyph_px(units, S, upm, L, v):
    ppem = (S + 1) * upm / L
    if v == "round26":
        x = round(units * ppem / upm * 64)
        return math.floor(x / 64 + 0.5)
    if v == "floor26":
        x = math.floor(units * ppem / upm * 64)
        return math.floor(x / 64 + 0.5)
    if v == "trunc26":
        x = int(units * ppem / upm * 64)
        return math.floor(x / 64 + 0.5)
    if v == "scale_floor":  # x_scale in 26.6 truncated, then advance rounded
        xs = math.floor(ppem * 64)
        x = round(units * xs / upm)
        return math.floor(x / 64 + 0.5)
    if v == "scale_floor_trunc":
        xs = math.floor(ppem * 64)
        x = (units * xs) // upm if units >= 0 else -((-units * xs) // upm)
        return math.floor(x / 64 + 0.5)
    if v == "scale_round_trunc":
        xs = round(ppem * 64)
        x = (units * xs) // upm if units >= 0 else -((-units * xs) // upm)
        return math.floor(x / 64 + 0.5)
    if v == "hb16":  # hb em_mult 16.16 of 26.6 scale
        xs = round(ppem * 64)
        mult = (xs << 16) // upm
        x = (units * mult + 32768) >> 16
        return math.floor(x / 64 + 0.5)
    if v == "hb16f":
        xs = math.floor(ppem * 64)
        mult = (xs << 16) // upm
        x = (units * mult + 32768) >> 16
        return math.floor(x / 64 + 0.5)
    if v == "f32":
        import numpy as np
        p = np.float32((S + 1)) * np.float32(upm) / np.float32(L)
        x = np.float32(units) * p / np.float32(upm)
        return int(math.floor(float(x) + 0.5))

variants = ["round26", "floor26", "trunc26", "scale_floor", "scale_floor_trunc", "scale_round_trunc", "hb16", "hb16f", "f32"]
datasets = []
for path in ["data/studio/sweep-1.5x.json", "data/studio/text-bounds-1.5x.json"]:
    data = json.load(open(path))
    for row in data["rows"]:
        key = (row["font"], row.get("weight", "Regular"))
        if key not in FILES: continue
        TS = math.floor((row["S"] + 0.25) / 1.5) if "S" in row else math.floor(row["size"])
        S = math.floor(TS * 1.5 + 1e-9)
        for text, w in zip(data["strings"], row["widths"]):
            target = w if "S" in row else round(w * 1.5)
            datasets.append((key, S, text, target))
for r in json.load(open("data/studio/wrapab-1.5x.json")):
    datasets.append(((r["f"], "Regular"), math.floor(r["s"] * 1.5), r["t"], r["u"]))

shaped = {}
for key, S, text, target in datasets:
    if (key, text) not in shaped:
        font, upm, L = fnt(key)
        buf = hb.Buffer(); buf.add_str(text); buf.guess_segment_properties()
        font.scale = (upm, upm); hb.shape(font, buf, {})
        shaped[(key, text)] = [p.x_advance for p in (buf.glyph_positions or [])]
for v in variants:
    hits = 0
    for key, S, text, target in datasets:
        font, upm, L = fnt(key)
        got = sum(glyph_px(u, S, upm, L, v) for u in shaped[(key, text)])
        hits += got == target
    print(f"{v:20} {hits}/{len(datasets)}")

import freetype
ftcache = {}
def ftface(key):
    f = FILES[key]
    if f not in ftcache:
        ftcache[f] = freetype.Face(os.path.join(V, f))
    return ftcache[f]

def shaped_full(key, text):
    font, upm, L = fnt(key)
    buf = hb.Buffer(); buf.add_str(text); buf.guess_segment_properties()
    font.scale = (upm, upm); hb.shape(font, buf, {})
    out = []
    for info, pos in zip(buf.glyph_infos or [], buf.glyph_positions or []):
        base = font.get_glyph_h_advance(info.codepoint)
        out.append((info.codepoint, base, pos.x_advance - base))
    return out

full = {}
for key, S, text, target in datasets:
    if (key, text) not in full:
        full[(key, text)] = shaped_full(key, text)

adv_cache = {}
def ft_adv(key, S, gid, flags):
    k = (key, S, gid, flags)
    if k not in adv_cache:
        face = ftface(key)
        font, upm, L = fnt(key)
        ppem = (S + 1) * upm / L
        p26 = round(ppem * 64)
        face.set_char_size(p26, p26, 72, 72)
        face.load_glyph(gid, flags)
        adv_cache[k] = face.glyph.advance.x
    return adv_cache[k]

for label, flags in [("light", freetype.FT_LOAD_TARGET_LIGHT), ("default", freetype.FT_LOAD_DEFAULT), ("nohint", freetype.FT_LOAD_NO_HINTING)]:
    for kmode in ["sep", "joint"]:
        hits = 0
        for key, S, text, target in datasets:
            font, upm, L = fnt(key)
            ppem = (S + 1) * upm / L
            total = 0
            for gid, base, kern in full[(key, text)]:
                a26 = ft_adv(key, S, gid, flags)
                k26 = round(kern * ppem / upm * 64)
                if kmode == "sep":
                    total += math.floor(a26 / 64 + 0.5) + math.floor(k26 / 64 + 0.5)
                else:
                    total += math.floor((a26 + k26) / 64 + 0.5)
            hits += total == target
        print(f"ft-{label}-{kmode:5} {hits}/{len(datasets)}")

print("--- split advance/kern variants")
for label in ["adv_float+kern26", "adv_round26+kern26", "adv_float+kernfloat", "adv_f32+kern26"]:
    hits = 0
    miss = 0
    for key, S, text, target in datasets:
        font, upm, L = fnt(key)
        ppem = (S + 1) * upm / L
        k = ppem / upm
        total = 0
        for gid, base, kern in full[(key, text)]:
            if label == "adv_float+kern26":
                v = base * k + round(kern * k * 64) / 64
            elif label == "adv_round26+kern26":
                v = round(base * k * 64) / 64 + round(kern * k * 64) / 64
            elif label == "adv_float+kernfloat":
                v = (base + kern) * k
            else:
                import numpy as np
                v = float(np.float32(base) * np.float32(ppem) / np.float32(upm)) + round(kern * k * 64) / 64
            total += math.floor(v + 0.5)
        hits += total == target
    print(f"{label:24} {hits}/{len(datasets)}")
