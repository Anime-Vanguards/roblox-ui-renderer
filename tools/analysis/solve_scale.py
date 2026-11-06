import json, math, os
from fontTools.ttLib import TTFont
V = os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "version-3afcc74cf8b04b5d", "content", "fonts")
FILES = {"SourceSansPro": "SourceSansPro-Regular.ttf", "FredokaOne": "FredokaOne-Regular.ttf", "Montserrat": "Montserrat-Bold.ttf", "BuilderSans": "BuilderSans-Regular.otf", "Roboto": "Roboto-Regular.ttf"}
d = json.load(open("data/studio/glyphs-1.5x.json"))
for fontname, file in FILES.items():
    f = TTFont(os.path.join(V, file))
    cmap = f.getBestCmap(); hmtx = f["hmtx"].metrics; upm = f["head"].unitsPerEm
    hh = f["hhea"]; o = f["OS/2"]
    print(f"== {fontname} upm={upm} hhea={hh.ascent},{hh.descent},gap{hh.lineGap} typo={o.sTypoAscender},{o.sTypoDescender},gap{o.sTypoLineGap} win={o.usWinAscent},{o.usWinDescent}")
    for row in d["rows"]:
        if row["font"] != fontname: continue
        S = math.floor(row["size"] * 1.5)
        for mode in ["round", "ceil", "floor"]:
            lo, hi = 0.0, 1.0
            bad = []
            for ch, (one, two, five) in row["glyphs"].items():
                one_p = round(one * 1.5); two_p = round(two*1.5); five_p = round(five * 1.5)
                step = (five_p - one_p) / 4
                if step != int(step) or (two_p - one_p) != step:
                    continue  # kerning/ligature between identical glyphs
                units = hmtx[cmap[ord(ch)]][0]
                if units == 0: continue
                if mode == "round":
                    a, b = (step - 0.5) / units, (step + 0.5) / units
                elif mode == "ceil":
                    a, b = (step - 1) / units, step / units
                else:
                    a, b = step / units, (step + 1) / units
                if a > lo: lo = a
                if b < hi: hi = b
            ok = lo < hi
            em_lo, em_hi = lo * upm, hi * upm
            print(f"   size {row['size']:>3} S={S:>3} {mode:>5}: {'OK ' if ok else 'NO '} em in [{em_lo:.3f},{em_hi:.3f}]  S/em in [{S/em_hi if em_hi else 0:.4f},{S/em_lo if em_lo else 0:.4f}]")
