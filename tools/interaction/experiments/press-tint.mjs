// AutoButtonColor press tint: GuiService.SelectedObject = button + Return held through VirtualInput
// (GuiState Press), one capture per colour; also the selected (Hover) state.
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { captureViewport } from "../../../cli/studio.mjs";
import { withStudioInput, formatLog, act, DPI } from "../studio.mjs";

const OUT = path.resolve("out/interaction/press-tint");
fs.mkdirSync(OUT, { recursive: true });
const COLORS = [[255, 245, 51], [25, 15, 5], [145, 85, 115], [175, 205, 235], [128, 127, 129], [211, 221, 241], [65, 75, 95], [100, 200, 150]];

const setup = `
local Backdrop = Instance.new("Frame")
Backdrop.Name = "Backdrop"
Backdrop.Size = UDim2.fromScale(1, 1)
Backdrop.BackgroundColor3 = Color3.fromRGB(20, 20, 26)
Backdrop.BorderSizePixel = 0
Backdrop.ZIndex = -1000
Backdrop.Parent = R.Gui
local Colors = { ${COLORS.map(c => `{ ${c.join(", ")} }`).join(", ")} }
for Index, C in Colors do
	local B = Instance.new("TextButton")
	B.Name = "B" .. Index
	B.Text = ""
	B.BorderSizePixel = 0
	B.Position = UDim2.fromOffset(60 + ((Index - 1) % 4) * 260, 80 + math.floor((Index - 1) / 4) * 200)
	B.Size = UDim2.fromOffset(200, 120)
	B.BackgroundColor3 = Color3.fromRGB(C[1], C[2], C[3])
	B.Parent = R.Gui
	R.Watch(B, B.Name, { "GuiState" }, {})
end
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));
const rgb = (png, x, y) => {
	const i = (Math.round(y * DPI) * png.width + Math.round(x * DPI)) * 4;
	return [png.data[i], png.data[i + 1], png.data[i + 2]];
};
const center = index => [60 + (index % 4) * 260 + 100, 80 + Math.floor(index / 4) * 200 + 60];

const run = await withStudioInput("press-tint", setup, async api => {
	const [pw, ph] = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	api.post([act.move(1900, 1100), act.sleep(300)]);
	const idle = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "idle.png")));
	const rows = [];
	for (let i = 0; i < COLORS.length; i++) {
		await api.luau(`game:GetService("GuiService").SelectedObject = R.Watched.B${i + 1} task.wait(0.3) R.Note("capture selected ${i + 1}") return ""`);
		const selected = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, `selected-${i + 1}.png`)));
		await api.luau(`R.SendKey(Enum.KeyCode.Return, true) task.wait(0.3) R.Note("capture pressed ${i + 1} " .. tostring(R.Watched.B${i + 1}.GuiState)) return ""`);
		let pressed;
		try {
			pressed = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, `pressed-${i + 1}.png`)));
		} finally {
			await api.luau(`R.SendKey(Enum.KeyCode.Return, false) task.wait(0.2) game:GetService("GuiService").SelectedObject = nil task.wait(0.1) return ""`);
		}
		const [cx, cy] = center(i);
		rows.push({ color: COLORS[i], idle: rgb(idle, cx, cy), selected: rgb(selected, cx, cy), pressed: rgb(pressed, cx, cy) });
	}
	return rows;
});
fs.writeFileSync(path.join(OUT, "results.json"), JSON.stringify(run.result, null, 1));
for (const row of run.result) console.log(JSON.stringify(row));
console.log(formatLog(run.log.filter(e => e.note || e.p === "GuiState")));
console.log(JSON.stringify(run.safety.changed));
