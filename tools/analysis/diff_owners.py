"""Attributes the pixels that differ between Studio and rur to the node drawn on top there.

Paints every visible node's (clipped, axis-aligned bounding) rect into an owner map in draw order,
so each physical pixel belongs to the topmost node covering it (shadows and strokes spill outside
their rect and are credited to whatever is underneath). Prints the nodes with the most differing
pixels, with the visuals that live on them, and optionally writes zoomed Studio | rur | diff crops.

usage: python diff_owners.py <out/compare/name> [--top 15] [--threshold 2] [--crops dir] [--crop-count 6]
"""

import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image


def arg(name, default):
    if name in sys.argv:
        return sys.argv[sys.argv.index(name) + 1]
    return default


def draw_order(scene):
    nodes = {node["id"]: node for node in scene.get("nodes", [])}
    order = []

    def collect(entries):
        for entry in entries or []:
            if isinstance(entry, dict):
                group = nodes.get(entry.get("group"))
                if group:
                    order.append(group)
                collect(entry.get("draw"))
            elif isinstance(entry, (int, float)) and entry > 0 and entry in nodes:
                order.append(nodes[entry])

    layers = sorted(scene.get("layers") or [], key=lambda layer: layer.get("displayOrder") or 0)
    for layer in layers:
        collect(layer.get("draw"))
    return order


def bounds(node, scale):
    x, y, w, h = node["rect"]
    if node.get("matrix"):
        a, b, c, d, e, f = node["matrix"]
        corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
        points = [(a * px + c * py + e, b * px + d * py + f) for px, py in corners]
        xs = [p[0] for p in points]
        ys = [p[1] for p in points]
        x0, y0, x1, y1 = min(xs), min(ys), max(xs), max(ys)
    else:
        x0, y0, x1, y1 = x, y, x + w, y + h
    if node.get("clip"):
        cx, cy, cw, ch = node["clip"]
        x0, y0, x1, y1 = max(x0, cx), max(y0, cy), min(x1, cx + cw), min(y1, cy + ch)
    return [int(math.floor(v * scale + 0.5)) for v in (x0, y0, x1, y1)]


def visuals(node):
    parts = []
    if node.get("text"):
        text = (node.get("props") or {}).get("text", "")
        parts.append(f"text {json.dumps(text[:28])} size {node['text'].get('textSize')}")
    if node.get("image"):
        image = node["image"]
        parts.append(f"image {image.get('uri', '').replace('rbxassetid://', 'id:')} {image.get('scaleType', '')}")
    if node.get("corner") is not None:
        parts.append("corner")
    if node.get("gradient"):
        parts.append("gradient")
    for stroke in node.get("strokes") or []:
        kind = "textstroke" if stroke.get("mode") == "Contextual" and node.get("text") else "stroke"
        parts.append(f"{kind} {round(stroke.get('thickness', 0), 2)}")
    if node.get("shadows"):
        parts.append("shadow")
    if node.get("group"):
        parts.append("group")
    if node.get("rotation"):
        parts.append(f"rot {round(node['rotation'], 1)}")
    if node.get("scroll"):
        parts.append("scroll")
    return ", ".join(parts)


def main():
    folder = Path(sys.argv[1])
    top = int(arg("--top", 15))
    threshold = int(arg("--threshold", 2))
    crops = arg("--crops", None)
    crop_count = int(arg("--crop-count", 6))

    scene = json.loads((folder / "rur" / "scene.json").read_text(encoding="utf8"))
    scale = scene["viewport"]["scale"]
    studio = np.asarray(Image.open(folder / "studio.png").convert("RGB")).astype(np.int16)
    ours = np.asarray(Image.open(folder / "rur" / "render.png").convert("RGB")).astype(np.int16)
    height, width = studio.shape[:2]
    diff = np.abs(studio - ours).max(axis=2)
    bad = diff > threshold

    owner = np.full((height, width), -1, dtype=np.int32)
    order = [node for node in draw_order(scene) if node.get("visible", True) is not False]
    for index, node in enumerate(order):
        x0, y0, x1, y1 = bounds(node, scale)
        x0, y0, x1, y1 = max(0, x0), max(0, y0), min(width, x1), min(height, y1)
        if x1 > x0 and y1 > y0:
            owner[y0:y1, x0:x1] = index

    total_bad = int(bad.sum())
    print(f"{folder.name}: {total_bad} px differ by > {threshold} ({total_bad / diff.size * 100:.3f}%), > 32: {int((diff > 32).sum())}")
    ids, counts = np.unique(owner[bad], return_counts=True)
    rows = []
    for owner_index, count in zip(ids, counts):
        severe = int(((owner == owner_index) & (diff > 32)).sum())
        node = order[owner_index] if owner_index >= 0 else None
        rows.append((int(count), severe, node))
    rows.sort(key=lambda row: -row[0])
    for count, severe, node in rows[:top]:
        if node is None:
            print(f"  {count:6d} ({severe:5d} >32)  (background)")
            continue
        path = node["path"].replace("CoreGui.RURStory.StoryTarget.", "")
        print(f"  {count:6d} ({severe:5d} >32)  {node['class']} {path}  [{visuals(node)}]")

    if crops:
        out = Path(crops)
        out.mkdir(parents=True, exist_ok=True)
        for rank, (count, severe, node) in enumerate(rows[:crop_count]):
            if node is None:
                continue
            x0, y0, x1, y1 = bounds(node, scale)
            pad = 6
            x0, y0 = max(0, x0 - pad), max(0, y0 - pad)
            x1, y1 = min(width, x1 + pad), min(height, y1 + pad)
            if x1 - x0 > 420:
                # centre a 420 px window on the densest column of differences
                columns = bad[y0:y1, x0:x1].sum(axis=0)
                centre = x0 + int(np.argmax(np.convolve(columns, np.ones(60), mode="same")))
                x0, x1 = max(0, centre - 210), min(width, centre + 210)
            if y1 - y0 > 300:
                rows_sum = bad[y0:y1, x0:x1].sum(axis=1)
                centre = y0 + int(np.argmax(np.convolve(rows_sum, np.ones(60), mode="same")))
                y0, y1 = max(0, centre - 150), min(height, centre + 150)
            zoom = max(1, min(6, int(900 / max(1, (x1 - x0) * 3))))
            a = Image.fromarray(studio[y0:y1, x0:x1].astype(np.uint8))
            b = Image.fromarray(ours[y0:y1, x0:x1].astype(np.uint8))
            heat = np.zeros((y1 - y0, x1 - x0, 3), dtype=np.uint8)
            local = diff[y0:y1, x0:x1]
            heat[..., 0] = np.clip(local * 6, 0, 255)
            heat[..., 1] = np.clip(local * 2, 0, 255)
            c = Image.fromarray(heat)
            w, h = (x1 - x0) * zoom, (y1 - y0) * zoom
            sheet = Image.new("RGB", (w * 3 + 16, h), (255, 0, 255))
            for i, image in enumerate((a, b, c)):
                sheet.paste(image.resize((w, h), Image.NEAREST), (i * (w + 8), 0))
            name = node["name"].replace("/", "_")
            sheet.save(out / f"{folder.name}-{rank}-{name}.png")


if __name__ == "__main__":
    main()
