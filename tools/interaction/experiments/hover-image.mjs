// ImageButton HoverImage / PressedImage: idle, hovered (posted move) and pressed (selection + Return)
// captures of an ImageButton whose three images differ (white / 70%-black / grey), background visible.
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { captureViewport } from "../../../cli/studio.mjs";
import { withStudioInput, formatLog, act, DPI, COVER } from "../studio.mjs";

const OUT = path.resolve("out/interaction/hover-image");
fs.mkdirSync(OUT, { recursive: true });
const setup = `${COVER}
Cover.BackgroundTransparency = 0
Cover.BackgroundColor3 = Color3.fromRGB(20, 20, 26)
local B = Instance.new("ImageButton")
B.Name = "B"
B.Position = UDim2.fromOffset(100, 100)
B.Size = UDim2.fromOffset(300, 150)
B.BackgroundColor3 = Color3.fromRGB(51, 153, 230)
B.BorderSizePixel = 0
B.ScaleType = Enum.ScaleType.Fit
B.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
B.HoverImage = "rbxasset://textures/blackBkg_square.png"
B.PressedImage = "rbxasset://textures/ui/GuiImagePlaceholder.png"
B.Parent = R.Gui
R.Watch(B, "B", { "GuiState", "Image" }, {})
local N = Instance.new("ImageButton")
N.Name = "N"
N.Position = UDim2.fromOffset(500, 100)
N.Size = UDim2.fromOffset(300, 150)
N.BackgroundColor3 = Color3.fromRGB(51, 153, 230)
N.BorderSizePixel = 0
N.ScaleType = Enum.ScaleType.Fit
N.AutoButtonColor = false
N.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png"
N.HoverImage = "rbxasset://textures/blackBkg_square.png"
N.PressedImage = "rbxasset://textures/ui/GuiImagePlaceholder.png"
N.Parent = R.Gui
R.Watch(N, "N", { "GuiState", "Image" }, {})
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));
const rgb = (png, x, y) => {
	const i = (Math.round(y * DPI) * png.width + Math.round(x * DPI)) * 4;
	return [png.data[i], png.data[i + 1], png.data[i + 2]];
};

const run = await withStudioInput("hover-image", setup, async api => {
	const [pw, ph] = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	const shots = {};
	const sample = (name, png) => {
		shots[name] = { Bbg: rgb(png, 120, 175), Bimg: rgb(png, 250, 175), Nbg: rgb(png, 520, 175), Nimg: rgb(png, 650, 175) };
	};
	api.post([act.batch(act.move(1900, 1100)), act.sleep(250)]);
	sample("idle", readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "idle.png"))));
	api.post([act.batch(act.move(1900, 1100)), act.sleep(120), act.batch(act.move(250, 175)), act.sleep(350)]);
	sample("hoverB", readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "hoverB.png"))));
	api.post([act.batch(act.move(1900, 1100)), act.sleep(120), act.batch(act.move(650, 175)), act.sleep(350)]);
	sample("hoverN", readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "hoverN.png"))));
	api.post([act.batch(act.move(1900, 1100)), act.sleep(250)]);
	for (const name of ["B", "N"]) {
		await api.luau(`game:GetService("GuiService").SelectedObject = R.Watched.${name} task.wait(0.25) R.SendKey(Enum.KeyCode.Return, true) task.wait(0.3) return ""`);
		try {
			sample(`press${name}`, readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, `press${name}.png`))));
		} finally {
			await api.luau(`R.SendKey(Enum.KeyCode.Return, false) task.wait(0.15) game:GetService("GuiService").SelectedObject = nil task.wait(0.15) return ""`);
		}
	}
	return shots;
});
for (const [k, v] of Object.entries(run.result)) console.log(k.padEnd(8), JSON.stringify(v));
console.log(formatLog(run.log.filter(e => e.p === "GuiState" || e.p === "Image")));
console.log(JSON.stringify(run.safety.changed));
