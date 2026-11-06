# Load a UIShadow probe capture (white shadow on black) and expose per-frame alpha maps.
import json
import math
import numpy as np
from PIL import Image

SCALE = 1.5

def load(run, source="studio"):
    path = f"{run}/studio.png" if source == "studio" else f"{run}/rur/render.png"
    img = np.asarray(Image.open(path).convert("RGB"), dtype=float)
    report = json.load(open(f"{run}/report.json"))
    frames = []
    for row in report["layout"]["rows"]:
        x, y, w, h = row["studio"]
        # Studio snaps rect edges with floor(v + 0.5)
        X0, Y0 = math.floor(x * SCALE + 0.5), math.floor(y * SCALE + 0.5)
        X1, Y1 = math.floor((x + w) * SCALE + 0.5), math.floor((y + h) * SCALE + 0.5)
        frames.append(dict(w=w, h=h, X0=X0, Y0=Y0, X1=X1, Y1=Y1))
    # alpha = red channel / 255 (shadow white, background black, frame blue)
    return img[:, :, 0] / 255.0, frames
