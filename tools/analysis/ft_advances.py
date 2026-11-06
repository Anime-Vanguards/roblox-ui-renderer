"""Compare FreeType hinted advances (various load flags / sizing) against Studio per-glyph steps."""
import json, math, os, sys
import freetype

V = os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "version-3afcc74cf8b04b5d", "content", "fonts")
FILES = {"SourceSansPro": "SourceSansPro-Regular.ttf", "FredokaOne": "FredokaOne-Regular.ttf", "Montserrat": "Montserrat-Bold.ttf", "BuilderSans": "BuilderSans-Regular.otf", "Roboto": "Roboto-Regular.ttf"}
d = json.load(open("data/studio/glyphs-1.5x.json"))
scale = 1.5

FLAGS = {
    "default": freetype.FT_LOAD_DEFAULT,
    "light": freetype.FT_LOAD_TARGET_LIGHT,
    "nohint": freetype.FT_LOAD_NO_HINTING,
    "autohint": freetype.FT_LOAD_FORCE_AUTOHINT,
    "mono": freetype.FT_LOAD_TARGET_MONO,
}

def metrics_for(face, S, sizing):
    upm = face.units_per_EM
    asc, desc = face.ascender, face.descender
    if sizing == "char_frac":
        ppem26 = round(S * upm / (asc - desc) * 64)
        face.set_char_size(ppem26, ppem26, 72, 72)
    elif sizing == "pixel_round":
        ppem = max(1, round(S * upm / (asc - desc)))
        face.set_pixel_sizes(0, ppem)
    elif sizing == "pixel_floor":
        ppem = max(1, math.floor(S * upm / (asc - desc)))
        face.set_pixel_sizes(0, ppem)
    elif sizing == "request_real":
        # FT_Request_Size with FT_SIZE_REQUEST_TYPE_REAL_DIM: height = ascender - descender in pixels
        req = freetype.FT_Size_RequestRec()
        req.type = freetype.FT_SIZE_REQUEST_TYPE_REAL_DIM
        req.width = 0
        req.height = S * 64
        req.horiResolution = 0
        req.vertResolution = 0
        freetype.FT_Request_Size(face._FT_Face, freetype.ctypes.byref(req))

def main():
    for fontname, file in FILES.items():
        face = freetype.Face(os.path.join(V, file))
        rows = [r for r in d["rows"] if r["font"] == fontname]
        results = {}
        for sizing in ["char_frac", "pixel_round", "pixel_floor", "request_real"]:
            for flagname, flag in FLAGS.items():
                hit = n = 0
                for row in rows:
                    S = math.floor(row["size"] * scale)
                    try:
                        metrics_for(face, S, sizing)
                    except Exception as e:
                        continue
                    for ch, (one, two, five) in row["glyphs"].items():
                        step = round((five - one) * scale) / 4
                        face.load_char(ch, flag)
                        adv = face.glyph.advance.x / 64
                        n += 1
                        if abs(adv - step) < 0.01:
                            hit += 1
                results[(sizing, flagname)] = (hit, n)
        ranked = sorted(results.items(), key=lambda kv: -kv[1][0])
        print(fontname, [(k, f"{v[0]}/{v[1]}") for k, v in ranked[:5]])

main()
