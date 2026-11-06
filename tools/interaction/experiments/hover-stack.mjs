// Which buttons get Hover when several are under the cursor? Concentric sibling TextButtons (smaller on
// top) and a chain of nested TextButtons, each a different grey. Posted moves only; one capture each.
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { captureViewport } from "../../../cli/studio.mjs";
import { withStudioInput, formatLog, act, DPI } from "../studio.mjs";

const OUT = path.resolve("out/interaction/hover-stack");
fs.mkdirSync(OUT, { recursive: true });
const LEVELS = [255, 254, 250, 245, 240, 230, 220, 200, 180, 160, 140, 120, 100, 80, 60, 40];
const N = LEVELS.length;
const S = { x: 300, y: 300 }; // centre of the sibling stack
const T = { x: 900, y: 300 }; // centre of the nested chain

const setup = `
local Backdrop = Instance.new("Frame")
Backdrop.Name = "Backdrop"
Backdrop.Size = UDim2.fromScale(1, 1)
Backdrop.BackgroundColor3 = Color3.fromRGB(20, 20, 26)
Backdrop.BorderSizePixel = 0
Backdrop.ZIndex = -1000
Backdrop.Parent = R.Gui
local Levels = { ${LEVELS.join(", ")} }
for Index, Level in Levels do
	local B = Instance.new("TextButton")
	B.Name = "S" .. Index
	B.Text = ""
	B.BorderSizePixel = 0
	B.AnchorPoint = Vector2.new(0.5, 0.5)
	B.Position = UDim2.fromOffset(${S.x}, ${S.y})
	B.Size = UDim2.fromOffset(20 + (${N} - Index) * 16, 20 + (${N} - Index) * 16)
	B.ZIndex = Index
	B.BackgroundColor3 = Color3.fromRGB(Level, Level, Level)
	B.Parent = R.Gui
	R.Watch(B, "S" .. Index, { "GuiState" }, { "MouseEnter" })
end
local Parent = R.Gui
for Index, Level in Levels do
	local B = Instance.new("TextButton")
	B.Name = "N" .. Index
	B.Text = ""
	B.BorderSizePixel = 0
	B.AnchorPoint = Vector2.new(0.5, 0.5)
	if Index == 1 then
		B.Position = UDim2.fromOffset(${T.x}, ${T.y})
	else
		B.Position = UDim2.fromScale(0.5, 0.5)
	end
	B.Size = UDim2.fromOffset(20 + (${N} - Index) * 16, 20 + (${N} - Index) * 16)
	B.BackgroundColor3 = Color3.fromRGB(Level, Level, Level)
	B.Parent = Parent
	Parent = B
	R.Watch(B, "N" .. Index, { "GuiState" }, { "MouseEnter" })
end
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));
const px = (png, x, y) => {
	const i = (Math.round(y * DPI) * png.width + Math.round(x * DPI)) * 4;
	return png.data[i];
};

const run = await withStudioInput("hover-stack", setup, async api => {
	const [pw, ph] = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	api.post([act.move(1900, 1100), act.sleep(400)]);
	const idle = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "idle.png")));
	api.post([act.move(1900, 1100), act.move(S.x, S.y), act.sleep(400)]);
	await api.luau(`R.Note("capture siblings") return ""`);
	const siblings = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "siblings.png")));
	api.post([act.move(1900, 1100), act.move(T.x, T.y), act.sleep(400)]);
	await api.luau(`R.Note("capture nested") return ""`);
	const nested = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "nested.png")));
	api.post([act.move(1900, 1100), act.sleep(200)]);
	const rows = [];
	for (let i = 0; i < N; i++) {
		// sample in the visible ring of element i (between its half size and the next one's)
		const half = (20 + (N - 1 - i) * 16) / 2 - 4;
		rows.push({ level: LEVELS[i], sIdle: px(idle, S.x - half, S.y), sHover: px(siblings, S.x - half, S.y), nIdle: px(idle, T.x - half, T.y), nHover: px(nested, T.x - half, T.y) });
	}
	return rows;
});
for (const row of run.result) console.log(JSON.stringify(row));
console.log(formatLog(run.log.filter(e => e.note || e.p === "GuiState" || e.e === "MouseEnter")));
console.log(JSON.stringify(run.safety.changed));
