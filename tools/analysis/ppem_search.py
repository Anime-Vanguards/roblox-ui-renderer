import json, math, os
import freetype
V = os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "version-3afcc74cf8b04b5d", "content", "fonts")
FILES = {"SourceSansPro": "SourceSansPro-Regular.ttf", "BuilderSans": "BuilderSans-Regular.otf", "FredokaOne": "FredokaOne-Regular.ttf"}
d = json.load(open("data/studio/sweep-1.5x.json"))
strings = d["strings"]
single = [i for i, s in enumerate(strings) if len(s) == 1]
FLAGS = {"default": freetype.FT_LOAD_DEFAULT, "light": freetype.FT_LOAD_TARGET_LIGHT, "nohint": freetype.FT_LOAD_NO_HINTING, "autohint": freetype.FT_LOAD_FORCE_AUTOHINT}
for fontname, file in FILES.items():
    face = freetype.Face(os.path.join(V, file))
    upm = face.units_per_EM; lh = face.ascender - face.descender
    seen = set()
    print("==", fontname, "upm", upm, "asc-desc", lh)
    for row in d["rows"]:
        if row["font"] != fontname: continue
        TS = math.floor((row["S"] + 0.25) / 1.5)
        if TS in seen: continue
        seen.add(TS)
        S = math.floor(TS * 1.5)
        target = [row["widths"][i] for i in single]
        matches = []
        for flagname, flag in FLAGS.items():
            for ppem26 in range(max(64, int(S * upm / lh * 64 * 0.8)), int(S * upm / lh * 64 * 1.2) + 1):
                face.set_char_size(ppem26, ppem26, 72, 72)
                got = []
                for i in single:
                    face.load_char(strings[i], flag)
                    got.append(face.glyph.advance.x / 64)
                if got == target:
                    matches.append((flagname, ppem26 / 64))
        # summarize contiguous ranges per flag
        summary = {}
        for flagname, p in matches:
            lo_hi = summary.setdefault(flagname, [p, p])
            lo_hi[0] = min(lo_hi[0], p); lo_hi[1] = max(lo_hi[1], p)
        exp = S * upm / lh
        print(f"  TS={TS:3} S={S:3} expected_ppem={exp:.3f} target={target} ->", {k: (round(v[0], 3), round(v[1], 3)) for k, v in summary.items()})
        if TS > 40: break
