# Compare Studio text rasters against FreeType load modes using the exported scene's glyph layout.
import sys, json, math
import numpy as np
import freetype
from PIL import Image
import ctypes
from freetype.raw import FT_Request_Size


class FT_Size_RequestRec(ctypes.Structure):
    _fields_ = [("type", ctypes.c_int), ("width", ctypes.c_long), ("height", ctypes.c_long), ("horiResolution", ctypes.c_uint), ("vertResolution", ctypes.c_uint)]


FT_Request_Size.argtypes = [ctypes.c_void_p, ctypes.POINTER(FT_Size_RequestRec)]


def set_ppem(face, ppem):
    # Studio sizing: exact 16.16 scales for a line height of S + 1 px
    d = face.ascender - face.descender
    line26 = int(round(ppem * d / face.units_per_EM * 64))
    scale = (line26 * 65536 + d // 2) // d
    request = FT_Size_RequestRec(4, scale, scale, 0, 0)
    FT_Request_Size(ctypes.cast(face._FT_Face, ctypes.c_void_p), ctypes.byref(request))

run = sys.argv[1]
scene = json.load(open(f"{run}/rur/scene.json"))
studio = np.asarray(Image.open(f"{run}/studio.png").convert("RGB")).astype(float)[:, :, 0] / 255
S = scene["viewport"]["scale"]
fonts = scene["fonts"]
faces = {}
snap = lambda v: math.floor(v + 0.5)
MODES = {
    "default": freetype.FT_LOAD_DEFAULT,
    "autohint": freetype.FT_LOAD_FORCE_AUTOHINT,
    "light": freetype.FT_LOAD_TARGET_LIGHT,
    "nohint": freetype.FT_LOAD_NO_HINTING,
}
def face_for(i):
    f = faces.get(i)
    if f is None:
        f = freetype.Face(fonts[i]["file"])
        faces[i] = f
    return f

def render(node, mode, ppem_scale=1.0):
    img = np.zeros_like(studio)
    t = node["text"]
    for line in t["lines"]:
        for fi, gid, gx, ppem in line["glyphs"]:
            ppem = ppem * ppem_scale
            face = face_for(fi)
            set_ppem(face, ppem)
            try:
                face.load_glyph(gid, MODES[mode] | freetype.FT_LOAD_RENDER)
            except freetype.FT_Exception:
                continue
            bm = face.glyph.bitmap
            if bm.width == 0:
                continue
            a = np.array(bm.buffer, dtype=float).reshape(bm.rows, bm.pitch)[:, :bm.width] / 255
            font = fonts[fi]
            base = snap(line["top"] * 1) + snap(font["ascender"] * ppem / font["unitsPerEm"])
            x0 = snap(line["x"]) + snap(gx) + face.glyph.bitmap_left
            y0 = base - face.glyph.bitmap_top
            if y0 < 0 or x0 < 0:
                continue
            region = img[y0:y0 + bm.rows, x0:x0 + bm.width]
            np.maximum(region, a[:region.shape[0], :region.shape[1]], out=region)
    return img

rows = []
only = sys.argv[2].split(",") if len(sys.argv) > 2 else None
for node in scene["nodes"]:
    if not node.get("text") or not node["text"].get("lines"):
        continue
    if only and node.get("name") not in only:
        continue
    x, y, w, h = node["rect"]
    X0, Y0, X1, Y1 = snap(x * S), snap(y * S), snap((x + w) * S), snap((y + h) * S)
    res = {}
    for mode in MODES:
        img = render(node, mode)
        err = (img[Y0:Y1, X0:X1] - studio[Y0:Y1, X0:X1]) * 255
        res[mode] = (np.sqrt((err ** 2).mean()), np.abs(err).max(), np.mean(np.abs(err) < 0.5) * 100)
    best = min(res, key=lambda m: res[m][0])
    fam = fonts[node["text"]["lines"][0]["glyphs"][0][0]]
    print(f"{node.get('name', '?'):28s} {fam['file'].split('/')[-1]:32s} size {node['text']['textSize']:3} ppem {node['text']['lines'][0]['glyphs'][0][3]:7.3f} best {best:9s} " +
          " ".join(f"{m}:{r[0]:.2f}/{r[1]:.0f}" for m, r in res.items()))
