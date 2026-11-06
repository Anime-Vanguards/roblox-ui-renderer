// Stacks Studio, rur and a >2-level diff mask for one node of a compare, zoomed.
//   node tools/analysis/compare_crop.mjs out/compare/<name> <NodeName> <out.png> [zoom]
import fs from "node:fs"; import path from "node:path";
import { PNG } from "pngjs";
const [dir, name, out, zoomArg] = process.argv.slice(2); const zoom = Number(zoomArg || 2);
const scene = JSON.parse(fs.readFileSync(path.join(dir, "rur/scene.json")));
const s = scene.viewport?.scale || 1.5;
const node = scene.nodes.find(n => n.path.endsWith("." + name));
const [x, y, w, h] = node.rect.map(v => Math.round(v * s)); const pad = 6;
const X = Math.max(0, x - pad), Y = Math.max(0, y - pad), W = w + 2 * pad, H = h + 2 * pad;
const A = PNG.sync.read(fs.readFileSync(path.join(dir, "studio.png"))), B = PNG.sync.read(fs.readFileSync(path.join(dir, "rur/render.png")));
const o = new PNG({ width: W * zoom, height: H * 3 * zoom });
for (let yy = 0; yy < H * 3; yy++) for (let xx = 0; xx < W; xx++) {
  const band = Math.floor(yy / H), sy = Y + (yy % H), sx = X + xx; let px;
  const a = (sy * A.width + sx) * 4, b = (sy * B.width + sx) * 4;
  if (band === 0) px = [A.data[a], A.data[a+1], A.data[a+2]];
  else if (band === 1) px = [B.data[b], B.data[b+1], B.data[b+2]];
  else { const d = Math.max(...[0,1,2].map(c => Math.abs(A.data[a+c]-B.data[b+c]))); px = d > 2 ? [255, Math.max(0,255-d*4), 0] : [A.data[a]/4, A.data[a+1]/4, A.data[a+2]/4]; }
  for (let zy = 0; zy < zoom; zy++) for (let zx = 0; zx < zoom; zx++) { const k = ((yy*zoom+zy) * o.width + xx*zoom+zx) * 4; o.data[k]=px[0]; o.data[k+1]=px[1]; o.data[k+2]=px[2]; o.data[k+3]=255; }
}
fs.writeFileSync(out, PNG.sync.write(o)); console.log(out, W, H);
