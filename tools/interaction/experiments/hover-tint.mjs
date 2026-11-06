// AutoButtonColor hover tint: each variant is hovered in turn (posted moves only, no clicks) and the
// viewport captured losslessly; crops + sampled colours go to out/interaction/hover-tint/.
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { captureViewport } from "../../../cli/studio.mjs";
import { withStudioInput, formatLog, act, DPI } from "../studio.mjs";

const OUT = path.resolve("out/interaction/hover-tint");
fs.mkdirSync(OUT, { recursive: true });

// [name, luau constructor body using `B` (already parented, positioned)]
export const VARIANTS = [
	["white", `B.BackgroundColor3 = Color3.new(1, 1, 1)`],
	["gray", `B.BackgroundColor3 = Color3.new(0.5, 0.5, 0.5)`],
	["red", `B.BackgroundColor3 = Color3.new(1, 0, 0)`],
	["black", `B.BackgroundColor3 = Color3.new(0, 0, 0)`],
	["blueHalf", `B.BackgroundColor3 = Color3.fromRGB(51, 153, 230) B.BackgroundTransparency = 0.5`],
	["gradient", `B.BackgroundColor3 = Color3.new(1, 1, 1) local G = Instance.new("UIGradient") G.Color = ColorSequence.new(Color3.new(1, 0, 0), Color3.new(0, 0, 1)) G.Parent = B`],
	["imageFit", `B.BackgroundColor3 = Color3.fromRGB(51, 153, 230) B.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png" B.ScaleType = Enum.ScaleType.Fit`],
	["imageTint", `B.BackgroundTransparency = 1 B.Image = "rbxasset://textures/ui/InGameMenu/WhiteSquare.png" B.ImageColor3 = Color3.fromRGB(51, 153, 230)`],
	["noAuto", `B.BackgroundColor3 = Color3.new(1, 1, 1) B.AutoButtonColor = false`],
	["text", `B.BackgroundColor3 = Color3.new(0, 0, 0) B.Text = "MMMM" B.TextSize = 40 B.TextColor3 = Color3.new(1, 1, 1) B.Font = Enum.Font.SourceSansBold`],
];

const cell = index => ({ x: 40 + (index % 5) * 240, y: 60 + Math.floor(index / 5) * 160, w: 200, h: 100 });

const setup = VARIANTS.map(([name, body], index) => {
	const { x, y, w, h } = cell(index);
	const className = name.startsWith("image") ? "ImageButton" : "TextButton";
	return `do
	local B = Instance.new("${className}")
	B.Name = "${name}"
	B.Position = UDim2.fromOffset(${x}, ${y})
	B.Size = UDim2.fromOffset(${w}, ${h})
	B.BorderSizePixel = 0
	${className === "TextButton" ? `B.Text = ""` : ""}
	${body}
	B.Parent = R.Gui
	R.Watch(B, "${name}", { "GuiState", "BackgroundColor3" }, {})
end`;
}).join("\n") + `
local Backdrop = Instance.new("Frame")
Backdrop.Name = "Backdrop"
Backdrop.Size = UDim2.fromScale(1, 1)
Backdrop.BackgroundColor3 = Color3.fromRGB(20, 20, 26)
Backdrop.BorderSizePixel = 0
Backdrop.ZIndex = -1000
Backdrop.Parent = R.Gui
`;

function readPng(file) {
	return PNG.sync.read(fs.readFileSync(file));
}

function cropCell(png, index, file) {
	const { x, y, w, h } = cell(index);
	const X = Math.round(x * DPI), Y = Math.round(y * DPI), W = Math.round(w * DPI), H = Math.round(h * DPI);
	const out = new PNG({ width: W, height: H });
	for (let row = 0; row < H; row++) png.data.copy(out.data, row * W * 4, ((Y + row) * png.width + X) * 4, ((Y + row) * png.width + X + W) * 4);
	fs.writeFileSync(file, PNG.sync.write(out));
	return out;
}

const sample = (png, fx, fy) => {
	const i = (Math.floor(fy * png.height) * png.width + Math.floor(fx * png.width)) * 4;
	return [png.data[i], png.data[i + 1], png.data[i + 2]];
};

const run = await withStudioInput("hover-tint", setup, async api => {
	const [pw, ph] = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	const results = {};
	api.post([act.move(1900, 1100), act.sleep(400)]);
	await api.luau(`R.Note("capture idle") return ""`);
	const idle = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, "idle-full.png")));
	for (const [index, [name]] of VARIANTS.entries()) {
		const { x, y, w, h } = cell(index);
		api.post([act.move(1900, 1100), act.move(x + w * 0.25, y + h * 0.2), act.sleep(350)]);
		await api.luau(`R.Note("capture ${name}") return ""`);
		const hover = readPng(captureViewport(api.studio.name, pw, ph, path.join(OUT, `hover-${name}-full.png`)));
		const a = cropCell(idle, index, path.join(OUT, `${name}-idle.png`));
		const b = cropCell(hover, index, path.join(OUT, `${name}-hover.png`));
		results[name] = {};
		for (const [label, fx, fy] of [["left", 0.08, 0.5], ["center", 0.5, 0.5], ["right", 0.92, 0.5], ["c2", 0.62, 0.62]]) {
			results[name][label] = { idle: sample(a, fx, fy), hover: sample(b, fx, fy) };
		}
		fs.rmSync(path.join(OUT, `hover-${name}-full.png`));
	}
	api.post([act.move(1900, 1100), act.sleep(200)]);
	return results;
});
fs.writeFileSync(path.join(OUT, "results.json"), JSON.stringify(run.result, null, 1));
fs.writeFileSync(path.join(OUT, "log.txt"), formatLog(run.log));
for (const [name, samples] of Object.entries(run.result)) console.log(name.padEnd(10), Object.entries(samples).map(([k, v]) => `${k} ${v.idle.join(",")} -> ${v.hover.join(",")}`).join(" | "));
console.log(formatLog(run.log.filter(e => e.note || e.p === "GuiState")));
console.log(JSON.stringify(run.safety.changed));
