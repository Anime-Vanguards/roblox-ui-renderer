// Colour quantization per render path (for the lead): colour channels (200.3, 250.3, 180.35) / 255 render
// as floor(c * 256) = (201, 251, 181), plain rounding would give (200, 250, 180). Also: do GuiState
// changes fire Changed / GetPropertyChangedSignal("GuiState")? (posted hover, no clicks)
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { captureViewport } from "../../../cli/studio.mjs";
import { withStudioInput, formatLog, act, DPI } from "../studio.mjs";

const OUT = path.resolve("out/interaction/quantize");
fs.mkdirSync(OUT, { recursive: true });

const setup = `
local C = Color3.new(200.3 / 255, 250.3 / 255, 180.35 / 255)
local Backdrop = Instance.new("Frame")
Backdrop.Name = "Backdrop"
Backdrop.Size = UDim2.fromScale(1, 1)
Backdrop.BackgroundColor3 = Color3.new(0, 0, 0)
Backdrop.BorderSizePixel = 0
Backdrop.ZIndex = -1000
Backdrop.Parent = R.Gui
local function Box(Class, Name, X, Y, W, H)
	local O = Instance.new(Class)
	O.Name = Name
	O.Position = UDim2.fromOffset(X, Y)
	O.Size = UDim2.fromOffset(W, H)
	O.BorderSizePixel = 0
	O.Parent = R.Gui
	return O
end
local Bg = Box("Frame", "Background", 50, 50, 120, 120)
Bg.BackgroundColor3 = C
local Half = Box("Frame", "HalfTransparent", 200, 50, 120, 120)
Half.BackgroundColor3 = C
Half.BackgroundTransparency = 0.5
local Text = Box("TextLabel", "Text", 350, 50, 200, 160)
Text.BackgroundTransparency = 1
Text.Text = "I"
Text.TextSize = 160
Text.Font = Enum.Font.ArialBold
Text.TextColor3 = C
local Img = Box("ImageLabel", "ImageTint", 600, 50, 120, 120)
Img.BackgroundTransparency = 1
Img.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
Img.ImageColor3 = C
local Stroked = Box("Frame", "Stroke", 780, 70, 100, 100)
Stroked.BackgroundTransparency = 1
local Stroke = Instance.new("UIStroke")
Stroke.Thickness = 20
Stroke.Color = C
Stroke.Parent = Stroked
local Grad = Box("Frame", "Gradient", 950, 50, 120, 120)
Grad.BackgroundColor3 = Color3.new(1, 1, 1)
local G = Instance.new("UIGradient")
G.Color = ColorSequence.new(C, C)
G.Parent = Grad
local Border = Box("Frame", "Border", 1120, 70, 80, 80)
Border.BackgroundTransparency = 1
Border.BorderSizePixel = 20
Border.BorderColor3 = C
local Group = Box("CanvasGroup", "Group", 1270, 50, 120, 120)
Group.BackgroundColor3 = Color3.new(1, 1, 1)
Group.GroupColor3 = C
local TextStroke = Box("TextLabel", "TextStroke", 1420, 50, 200, 160)
TextStroke.BackgroundTransparency = 1
TextStroke.Text = "I"
TextStroke.TextSize = 160
TextStroke.Font = Enum.Font.ArialBold
TextStroke.TextTransparency = 1
TextStroke.TextStrokeTransparency = 0
TextStroke.TextStrokeColor3 = C
local Button = Box("TextButton", "Button", 50, 400, 200, 100)
Button.Text = ""
local Count = { changed = 0, state = 0 }
R.Counts = Count
Button.Changed:Connect(function(Property)
	if Property == "GuiState" then
		Count.changed += 1
	end
end)
Button:GetPropertyChangedSignal("GuiState"):Connect(function()
	Count.state += 1
end)
R.Watch(Button, "Button", { "GuiState" }, {})
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));
const rgb = (png, x, y) => {
	const i = (Math.round(y * DPI) * png.width + Math.round(x * DPI)) * 4;
	return [png.data[i], png.data[i + 1], png.data[i + 2]];
};

const run = await withStudioInput("quantize", setup, async api => {
	const [pw, ph] = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	api.post([act.move(1900, 1100), act.sleep(300)]);
	const shot = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "shot.png")));
	const samples = {
		Background: rgb(shot, 110, 110), HalfTransparent: rgb(shot, 260, 110), Text: null, ImageTint: rgb(shot, 660, 110),
		Stroke: rgb(shot, 770, 120), Gradient: rgb(shot, 1010, 110), Border: rgb(shot, 1110, 110), Group: rgb(shot, 1330, 110), TextStroke: null,
	};
	// text: brightest pixel inside the label box (stem interior)
	const brightest = (x0, y0, x1, y1) => {
		let best = [0, 0, 0];
		for (let y = Math.round(y0 * DPI); y < Math.round(y1 * DPI); y++) for (let x = Math.round(x0 * DPI); x < Math.round(x1 * DPI); x++) {
			const i = (y * shot.width + x) * 4;
			if (shot.data[i] + shot.data[i + 1] + shot.data[i + 2] > best[0] + best[1] + best[2]) best = [shot.data[i], shot.data[i + 1], shot.data[i + 2]];
		}
		return best;
	};
	samples.Text = brightest(350, 50, 550, 210);
	samples.TextStroke = brightest(1420, 50, 1620, 210);
	api.post([act.move(1900, 1100), act.move(150, 450), act.sleep(300)]);
	api.post([act.move(1900, 1100), act.sleep(300)]);
	samples.GuiStateSignals = JSON.parse(await api.luau(`return HttpService:JSONEncode(R.Counts)`));
	return samples;
});
for (const [k, v] of Object.entries(run.result)) console.log(k.padEnd(16), JSON.stringify(v));
console.log(formatLog(run.log.filter(e => e.p === "GuiState")));
console.log(JSON.stringify(run.safety.changed));
