import math, os, json
import freetype, uharfbuzz as hb
V = os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "version-3afcc74cf8b04b5d", "content", "fonts")
fonts = {"SourceSansPro": "SourceSansPro-Regular.ttf", "BuilderSans": "BuilderSans-Regular.otf", "FredokaOne": "FredokaOne-Regular.ttf"}
d = json.load(open("data/studio/sweep-1.5x.json"))
rows_out = []
for fname, file in fonts.items():
    path = os.path.join(V, file)
    ft = freetype.Face(path); hbf = hb.Font(hb.Face(hb.Blob.from_file_path(path)))
    upm = ft.units_per_EM; L = ft.ascender - ft.descender
    seen = set()
    for row in d["rows"]:
        if row["font"] != fname: continue
        TS = math.floor((row["S"] + 0.25) / 1.5)
        if TS in seen: continue
        seen.add(TS)
        S = math.floor(TS * 1.5)
        ppem = (S + 1) * upm / L
        ft.set_char_size(round(ppem*64), round(ppem*64), 72, 72)
        for idx, text in enumerate(d["strings"]):
            if len(text) != 2: continue
            buf = hb.Buffer(); buf.add_str(text); buf.guess_segment_properties()
            hbf.scale = (upm, upm); hb.shape(hbf, buf, {})
            infos, poss = buf.glyph_infos, buf.glyph_positions
            if len(infos) != 2: continue
            advs = []; lin = []; deltas = []
            for info in infos:
                ft.load_glyph(info.codepoint, freetype.FT_LOAD_TARGET_LIGHT)
                raw = ft.glyph._FT_GlyphSlot.contents
                advs.append(ft.glyph.advance.x / 64); lin.append(raw.linearHoriAdvance / 65536)
                deltas.append((raw.lsb_delta, raw.rsb_delta))
            kern = (poss[0].x_advance - hbf.get_glyph_h_advance(infos[0].codepoint)) * ppem / upm
            target = row["widths"][idx]
            diff = target - sum(advs)
            rows_out.append((fname, TS, text, target, sum(advs), diff, round(kern, 3), round(lin[0] - advs[0], 3), deltas[0][1], deltas[1][0]))
for r in rows_out:
    if r[2] in ("AV", "To") or r[5] != 0:
        print(r)
