// Fresh Studio MCP measurements of wheel position, velocity API and frame cadence.
// node tools/interaction/experiments/scroll-physics.mjs [output.json] [Studio name or id]
import fs from "node:fs";
import path from "node:path";
import { withStudioInput } from "../studio.mjs";
import { DEFAULT_STUDIO } from "../../../cli/studio.mjs";

const output = path.resolve(process.argv[2] || "out/interaction/scroll-physics.json");
const studio = process.argv[3] || DEFAULT_STUDIO;
const setup = `
local Catcher = Instance.new("ScrollingFrame")
Catcher.Name = "Catcher"
Catcher.Size = UDim2.fromScale(1, 1)
Catcher.CanvasSize = UDim2.fromOffset(100000, 100000)
Catcher.ZIndex = -10
Catcher.Parent = R.Gui
local Scroll = Instance.new("ScrollingFrame")
Scroll.Name = "MeasuredScroll"
Scroll.Position = UDim2.fromOffset(100, 100)
Scroll.Size = UDim2.fromOffset(400, 300)
Scroll.CanvasSize = UDim2.fromOffset(400, 6000)
Scroll.BorderSizePixel = 0
Scroll.ScrollBarThickness = 0
Scroll.Parent = R.Gui
R.Scroll = Scroll
`;
const experiment = fs.readFileSync(new URL("./scroll-physics.luau", import.meta.url), "utf8");
const run = await withStudioInput("scroll-physics", setup, api => api.luau(experiment), { studio });
const capture = { ...JSON.parse(run.result), safety: run.safety };
fs.mkdirSync(path.dirname(output), { recursive: true });
fs.writeFileSync(output, JSON.stringify(capture));
console.log(JSON.stringify({ output, version: capture.version, cases: capture.cases.map(item => ({
	name: item.name, frames: item.samples.length, final: item.samples.at(-1),
	maxApiVelocity: Math.max(...item.samples.map(sample => Math.hypot(sample.vx, sample.vy))),
})), safety: run.safety.changed }));
