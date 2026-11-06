import json, math, os, ctypes
import freetype
from freetype.raw import *

V = os.path.join(os.environ["LOCALAPPDATA"], "Roblox", "Versions", "version-3afcc74cf8b04b5d", "content", "fonts")
FILES = {"SourceSansPro": "SourceSansPro-Regular.ttf", "FredokaOne": "FredokaOne-Regular.ttf", "Montserrat": "Montserrat-Bold.ttf", "BuilderSans": "BuilderSans-Regular.otf", "Roboto": "Roboto-Regular.ttf"}
d = json.load(open("data/studio/glyphs-1.5x.json"))

lib = freetype.get_handle()
def set_interpreter(v):
    val = ctypes.c_uint(v)
    err = freetype.raw.FT_Property_Set(lib, b"truetype", b"interpreter-version", ctypes.byref(val))
    return err

FLAGS = {"default": freetype.FT_LOAD_DEFAULT, "light": freetype.FT_LOAD_TARGET_LIGHT, "mono": freetype.FT_LOAD_TARGET_MONO, "lcd": freetype.FT_LOAD_TARGET_LCD, "nohint": freetype.FT_LOAD_NO_HINTING, "autohint": freetype.FT_LOAD_FORCE_AUTOHINT}

for interp in (35, 38, 40):
    err = set_interpreter(interp)
    for fontname, file in FILES.items():
        best = []
        for flagname, flag in FLAGS.items():
            face = freetype.Face(os.path.join(V, file))
            hit = n = 0
            for row in d["rows"]:
                if row["font"] != fontname: continue
                S = math.floor(row["size"] * 1.5)
                em26 = round(S * face.units_per_EM / (face.ascender - face.descender) * 64)
                face.set_char_size(em26, em26, 72, 72)
                for ch, (one, two, five) in row["glyphs"].items():
                    face.load_char(ch, flag)
                    adv = face.glyph.advance.x / 64
                    n += 1
                    if abs(adv - round(one * 1.5)) < 0.01:
                        hit += 1
            best.append((hit, flagname))
        best.sort(reverse=True)
        print(f"interp {interp} (err {err}) {fontname}: " + ", ".join(f"{f}={h}/{n}" for h, f in best[:4]))
