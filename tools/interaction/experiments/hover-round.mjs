// Hover tint rounding: concentric stacks (all buttons under the cursor hover) of TextButtons and of
// ImageButtons (no image) with channel levels at x.5 / x.7 products; plus ImageButtons with HoverImage.
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { captureViewport } from "../../../cli/studio.mjs";
import { withStudioInput, formatLog, act, DPI } from "../studio.mjs";

const OUT = path.resolve("out/interaction/hover-round");
fs.mkdirSync(OUT, { recursive: true });
const COLORS = [[255, 245, 51], [25, 15, 5], [145, 85, 115], [175, 205, 235], [1, 2, 3], [128, 127, 129], [211, 221, 241], [65, 75, 95]];
const N = COLORS.length;
const STACKS = [{ name: "T", cls: "TextButton", x: 250, y: 250 }, { name: "I", cls: "ImageButton", x: 700, y: 250 }, { name: "F", cls: "TextButton", x: 1150, y: 250, float: true }];

const setup = `
local Backdrop = Instance.new("Frame")
Backdrop.Name = "Backdrop"
Backdrop.Size = UDim2.fromScale(1, 1)
Backdrop.BackgroundColor3 = Color3.fromRGB(20, 20, 26)
Backdrop.BorderSizePixel = 0
Backdrop.ZIndex = -1000
Backdrop.Parent = R.Gui
local Colors = { ${COLORS.map(c => `{ ${c.join(", ")} }`).join(", ")} }
for _, Stack in { ${STACKS.map(s => `{ Name = "${s.name}", Class = "${s.cls}", X = ${s.x}, Y = ${s.y}, Float = ${s.float ? "true" : "false"} }`).join(", ")} } do
	for Index, C in Colors do
		local B = Instance.new(Stack.Class)
		B.Name = Stack.Name .. Index
		if Stack.Class == "TextButton" then
			B.Text = ""
		end
		B.BorderSizePixel = 0
		B.AnchorPoint = Vector2.new(0.5, 0.5)
		B.Position = UDim2.fromOffset(Stack.X, Stack.Y)
		B.Size = UDim2.fromOffset(20 + (${N} - Index) * 24, 20 + (${N} - Index) * 24)
		B.ZIndex = Index
		if Stack.Float then
			-- non-8-bit colours: C / 255 + 0.3 / 255
			B.BackgroundColor3 = Color3.new((C[1] + 0.3) / 255, (C[2] + 0.3) / 255, (C[3] + 0.3) / 255)
		else
			B.BackgroundColor3 = Color3.fromRGB(C[1], C[2], C[3])
		end
		B.Parent = R.Gui
		R.Watch(B, B.Name, { "GuiState" }, {})
	end
end
local H = Instance.new("ImageButton")
H.Name = "HoverImage"
H.Position = UDim2.fromOffset(1500, 150)
H.Size = UDim2.fromOffset(200, 200)
H.BackgroundColor3 = Color3.fromRGB(51, 153, 230)
H.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
H.HoverImage = "rbxasset://textures/SurfacesDefault.png"
H.ImageColor3 = Color3.fromRGB(255, 128, 0)
H.ScaleType = Enum.ScaleType.Fit
H.Size = UDim2.fromOffset(300, 150)
H.Parent = R.Gui
R.Watch(H, "HoverImage", { "GuiState" }, {})
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));
const rgb = (png, x, y) => {
	const i = (Math.round(y * DPI) * png.width + Math.round(x * DPI)) * 4;
	return [png.data[i], png.data[i + 1], png.data[i + 2]];
};

const run = await withStudioInput("hover-round", setup, async api => {
	const [pw, ph] = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	api.post([act.move(1900, 1100), act.sleep(400)]);
	const idle = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "idle.png")));
	const rows = [];
	for (const stack of STACKS) {
		api.post([act.move(1900, 1100), act.move(stack.x, stack.y), act.sleep(400)]);
		await api.luau(`R.Note("capture ${stack.name}") return ""`);
		const hover = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, `hover-${stack.name}.png`)));
		for (let i = 0; i < N; i++) {
			const half = (20 + (N - 1 - i) * 24) / 2 - 5;
			rows.push({ stack: stack.name, color: COLORS[i], idle: rgb(idle, stack.x - half, stack.y), hover: rgb(hover, stack.x - half, stack.y) });
		}
	}
	api.post([act.move(1900, 1100), act.move(1600, 180), act.sleep(400)]);
	await api.luau(`R.Note("capture HoverImage") return ""`);
	const hi = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "hover-image.png")));
	rows.push({ stack: "HoverImage", bgIdle: rgb(idle, 1510, 225), bgHover: rgb(hi, 1510, 225), imgIdle: rgb(idle, 1650, 225), imgHover: rgb(hi, 1650, 225) });
	api.post([act.move(1900, 1100), act.sleep(200)]);
	return rows;
});
for (const row of run.result) console.log(JSON.stringify(row));
console.log(formatLog(run.log.filter(e => e.note || (e.p === "GuiState" && /1$|HoverImage/.test(e.o)))));
console.log(JSON.stringify(run.safety.changed));
