"""Fit Roblox's text advance model against Studio TextBounds (physical pixel scale S)."""
import json, math, os, sys
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
    ("LegacyArial", "Regular"): "Arimo-Regular.ttf",
    ("GothamSSm", "Regular"): "Montserrat-Regular.ttf",
    ("GothamSSm", "Bold"): "Montserrat-Bold.ttf",
    ("GothamSSm", "Medium"): "Montserrat-Medium.ttf",
    ("GothamSSm", "Heavy"): "Montserrat-Black.ttf",
}

def load(name):
    f = TTFont(os.path.join(V, name))
    cmap = f.getBestCmap()
    hmtx = f["hmtx"].metrics
    upm = f["head"].unitsPerEm
    hhea = f["hhea"]
    os2 = f["OS/2"]
    kern = {}
    if "kern" in f:
        for t in f["kern"].kernTables:
            kern.update(t.kernTable)
    # GPOS pair kerning (formats 1 and 2) for default features.
    gpos = {}
    if "GPOS" in f:
        table = f["GPOS"].table
        for lookup in table.LookupList.Lookup:
            for sub in lookup.SubTable:
                st = sub
                if lookup.LookupType == 9:
                    st = sub.ExtSubTable
                if getattr(st, "LookupType", lookup.LookupType) != 2:
                    continue
                if st.Format == 1:
                    cov = st.Coverage.glyphs
                    for i, g1 in enumerate(cov):
                        for pvr in st.PairSet[i].PairValueRecord:
                            v = pvr.Value1
                            if v is not None and getattr(v, "XAdvance", 0):
                                gpos.setdefault((g1, pvr.SecondGlyph), v.XAdvance)
                elif st.Format == 2:
                    cov = st.Coverage.glyphs
                    cd1 = st.ClassDef1.classDefs
                    cd2 = st.ClassDef2.classDefs
                    gpos_class = (cov, cd1, cd2, st.Class1Record)
                    gpos.setdefault("__classes__", []).append(gpos_class)
    return dict(font=f, cmap=cmap, hmtx=hmtx, upm=upm, hhea=hhea, os2=os2, kern=kern, gpos=gpos)

def pair_kern(F, a, b, use_gpos=True):
    k = F["kern"].get((a, b))
    if k is not None:
        return k
    if not use_gpos:
        return 0
    g = F["gpos"]
    v = g.get((a, b))
    if v is not None:
        return v
    for cov, cd1, cd2, recs in g.get("__classes__", []):
        if a in cov:
            c1 = cd1.get(a, 0)
            c2 = cd2.get(b, 0)
            rec = recs[c1].Class2Record[c2].Value1
            if rec is not None and getattr(rec, "XAdvance", 0):
                return rec.XAdvance
            return 0
    return 0

cache = {}
def F(key):
    name = FILES[key]
    if name not in cache:
        cache[name] = load(name)
    return cache[name]

def glyphs(Fn, text):
    return [Fn["cmap"].get(ord(c), ".notdef") for c in text]

def width_model(Fn, text, S, mode):
    upm = Fn["upm"]
    if mode["vm"] == "hhea":
        asc, desc = Fn["hhea"].ascent, Fn["hhea"].descent
    elif mode["vm"] == "typo":
        asc, desc = Fn["os2"].sTypoAscender, Fn["os2"].sTypoDescender
    else:
        asc, desc = Fn["os2"].usWinAscent, -Fn["os2"].usWinDescent
    em = S * upm / (asc - desc)
    gs = glyphs(Fn, text)
    total = 0.0
    for i, g in enumerate(gs):
        adv = Fn["hmtx"][g][0]
        if mode["kern"] and i + 1 < len(gs):
            adv += pair_kern(Fn, g, gs[i + 1])
        px = adv * em / upm
        if mode["round"] == "each-round":
            px = math.floor(px + 0.5)
        elif mode["round"] == "each-ceil":
            px = math.ceil(px - 1e-9)
        elif mode["round"] == "each-floor":
            px = math.floor(px + 1e-9)
        elif mode["round"] == "26.6":
            px = round(px * 64) / 64
        total += px
    if mode["total"] == "ceil":
        total = math.ceil(total - 1e-6)
    elif mode["total"] == "round":
        total = math.floor(total + 0.5)
    return total

def main():
    data = json.load(open(sys.argv[1]))
    scale = float(sys.argv[2]) if len(sys.argv) > 2 else 1.5
    strings = data["strings"]
    modes = []
    for vm in ["hhea", "typo", "win"]:
        for kern in [False, True]:
            for rnd in ["none", "each-round", "each-ceil", "each-floor", "26.6"]:
                for total in ["ceil", "round", "none"]:
                    modes.append(dict(vm=vm, kern=kern, round=rnd, total=total))
    per_font = {}
    for row in data["rows"]:
        key = (row["font"], row["weight"])
        if key not in FILES:
            continue
        S = math.floor(row["size"] * scale + 1e-9)
        Fn = F(key)
        for text, w in zip(strings, row["widths"]):
            target = round(w * scale)
            for mi, mode in enumerate(modes):
                got = width_model(Fn, text, S, mode)
                stats = per_font.setdefault(key, {}).setdefault(mi, [0, 0, 0.0])
                stats[1] += 1
                if abs(got - target) < 0.51:
                    stats[0] += 1
                stats[2] += abs(got - target)
    for key, stats in per_font.items():
        ranked = sorted(stats.items(), key=lambda kv: (-kv[1][0], kv[1][2]))
        print(key)
        for mi, (hit, n, err) in ranked[:4]:
            print("   ", f"{hit}/{n}", f"err={err:.1f}", modes[mi])

main()
