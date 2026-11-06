# rbxthumb textures (fixtures/image-probe-thumb.luau): reads Studio's texels back from the large Pixelated
# drawings (the centre pixel of each texel block, composited over the backdrop) and compares them with the
# thumbnails our asset pipeline fetched (same composite), per channel.
#
#   python tools/analysis/image_thumb.py [compare name]
import json
import os
import sys
import numpy as np
from PIL import Image

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def main():
	name = sys.argv[1] if len(sys.argv) > 1 else "image-probe-thumb"
	run = os.path.join(ROOT, "out", "compare", name)
	studio = np.asarray(Image.open(os.path.join(run, "studio.png")).convert("RGB"), dtype=np.float64)
	scene = json.load(open(os.path.join(run, "rur", "scene.json"), encoding="utf-8"))
	manifest = json.load(open(os.path.join(run, "rur", "manifest.json"), encoding="utf-8"))
	report = json.load(open(os.path.join(run, "report.json"), encoding="utf-8"))
	backdrop = np.array((report.get("options") or {}).get("background") or [30, 30, 30], dtype=np.float64)
	S = scene["viewport"].get("scale", 1.5)
	for node in scene["nodes"]:
		image = node.get("image")
		if not image or image.get("resample") != "Pixelated":
			continue
		entry = manifest["images"].get(image["uri"])
		tex = np.asarray(Image.open(os.path.join(run, "rur", entry["src"])).convert("RGBA"), dtype=np.float64)
		th, tw = tex.shape[:2]
		x, y, w, h = [v * S for v in node["rect"]]
		# centre of texel (i, j): device x + (i + 0.5) * w / tw
		cols = np.floor(x + (np.arange(tw) + 0.5) * w / tw).astype(int)
		rows = np.floor(y + (np.arange(th) + 0.5) * h / th).astype(int)
		seen = studio[rows][:, cols]
		a = tex[..., 3:4] / 255
		ours = np.floor(tex[..., :3] * a + backdrop * (1 - a) + 0.5)
		d = np.abs(seen - ours).max(-1)
		print(f"{node['name']}: {tw}x{th} texels, identical {100 * (d == 0).mean():.1f}%, within 2 {100 * (d <= 2).mean():.1f}%, "
			f"mean |d| {d.mean():.2f}, max {int(d.max())}; mean signed (Studio - ours) {np.round((seen - ours).mean(axis=(0, 1)), 2).tolist()}")


if __name__ == "__main__":
	main()
