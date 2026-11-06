import json, math, os, sys, itertools
import uharfbuzz as hb
from fontTools.ttLib import TTFont
V = os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "version-3afcc74cf8b04b5d", "content", "fonts")
FILES = {"SourceSansPro": "SourceSansPro-Regular.ttf", "FredokaOne": "FredokaOne-Regular.ttf"}
class F:
    def __init__(s, path):
        s.hb = hb.Font(hb.Face(hb.Blob.from_file_path(path)))
        tt = TTFont(path); s.upm = tt["head"].unitsPerEm; s.L = tt["hhea"].ascent - tt["hhea"].descent
        s.cmap = tt.getBestCmap()
fonts = {k: F(os.path.join(V, v)) for k, v in FILES.items()}

def shape(f, text, S):
    if not text: return []
    k = (S + 1) / f.L
    buf = hb.Buffer(); buf.add_str(text); buf.guess_segment_properties()
    f.hb.scale = (f.upm, f.upm); hb.shape(f.hb, buf, {})
    out = []
    for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
        adv = math.floor(round(pos.x_advance * k * 64) / 64 + 0.5)
        out.append((info.cluster, adv))
    return out

def layout(f, text, S, maxw, rule):
    lines_out = []
    for para in text.split("\n"):
        if rule["shape"] == "whole":
            glyphs = shape(f, para, S)
            advs = [0] * len(para)
            for cl, a in glyphs: advs[cl] += a
        else:  # per word: shape runs split at spaces separately
            advs = [0] * len(para)
            i = 0
            while i < len(para):
                j = i
                if para[i] == " ":
                    while j < len(para) and para[j] == " ": j += 1
                else:
                    while j < len(para) and para[j] != " ": j += 1
                for cl, a in shape(f, para[i:j], S): advs[i + cl] += a
                i = j
        # tokens: words and spaces
        n = len(para)
        lines = []
        start = 0
        while True:
            if start >= n:
                if not lines or n == 0: lines.append((start, start))
                break
            width = 0; i = start; last_break = None
            while i < n:
                ch = para[i]
                if ch == " ":
                    last_break = i
                    width += advs[i]; i += 1; continue
                w2 = width + advs[i]
                over = w2 > maxw if rule["fit"] == "gt" else w2 >= maxw
                if over and i > start:
                    if rule["overflow"] == "allow-crossing" and last_break is None:
                        width = w2; i += 1
                        break
                    break
                width = w2; i += 1
            if i >= n:
                lines.append((start, n)); break
            if last_break is not None and last_break >= start:
                end = last_break
                nxt = last_break + 1
                if rule["spaces"] == "skip-all":
                    while nxt < n and para[nxt] == " ": nxt += 1
            else:
                end = i; nxt = i
            lines.append((start, end))
            start = nxt
            if start >= n: break
        for a, b in lines:
            seg = para[a:b]
            w = sum(advs[a:b])
            if rule["trim"]:
                t = b
                while t > a and para[t - 1] == " ": t -= 1
                w = sum(advs[a:t])
            lines_out.append(w)
    return lines_out

d = json.load(open("data/studio/wrap-1.5x.json", encoding="utf8"))
rules = [dict(shape=s, fit=fi, overflow=o, spaces=sp, trim=t) for s in ["whole", "word"] for fi in ["gt", "ge"] for o in ["strict", "allow-crossing"] for sp in ["single", "skip-all"] for t in [True, False]]
best = []
for rule in rules:
    hits = 0; n = 0; miss = []
    for r in d["wrap"]:
        text = d["texts"][r["t"] - 1]
        if r["t"] == 7: continue  # CJK font missing offline
        f = fonts[r["f"]]; S = math.floor(r["s"] * 1.5)
        lines = layout(f, text, S, r["w"] * 1.5, rule)
        gx = max(lines) if lines else 0
        gy = S * len(lines)
        sx = round(r["bx"] * 1.5); sy = round(r["by"] * 1.5)
        n += 1
        if gx == sx and gy == sy: hits += 1
        elif len(miss) < 12: miss.append((r["f"], r["s"], r["w"], text[:22], (sx, sy // S), (gx, len(lines))))
    best.append((hits, n, rule, miss))
best.sort(key=lambda x: -x[0])
for hits, n, rule, miss in best[:3]:
    print(hits, n, rule)
    for m in miss: print("   ", m)
