// Interaction-state parity: mounts a fixture in Studio (temporary GUI under CoreGui, over an Active cover),
// drives it with posted mouse input, captures the viewport, renders the same fixture offline with the same
// input as an --actions timeline and compares the pixels.
//
//   node tools/interaction/compare.mjs --fixture fixtures/interaction-autobuttoncolor.luau --viewport 900x500
//     --background 20,20,26 --time 0.5 --hover 250,250 [--click x,y] [--select Name] [--press Name]
//
// --hover x,y: the cursor rests at x,y (posted moves: far away, then x,y)
// --click x,y: a click at x,y (posted down/up in one batch), then the cursor rests there
// --select Name / --press Name: GuiService.SelectedObject = the descendant called Name (Hover state);
//   --press also holds Return through VirtualInput while capturing (Press state). Offline these become
//   a hover / a held mouse button on the object's centre.
// Output: out/interaction/compare/<name>/{studio.png, rur/, diff.png, report.json}

import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { captureViewport } from "../../cli/studio.mjs";
import { render } from "../../cli/rur.mjs";
import { comparePngs } from "../../cli/compare.mjs";
import { withStudioInput, act, DPI } from "./studio.mjs";

const args = {};
const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i++) {
	if (argv[i].startsWith("--")) {
		args[argv[i].slice(2)] = argv[i + 1] && !argv[i + 1].startsWith("--") ? argv[++i] : true;
	}
}
if (!args.fixture) {
	process.stderr.write("usage: node tools/interaction/compare.mjs --fixture f.luau [--viewport WxH] [--hover x,y | --click x,y | --select Name | --press Name]\n");
	process.exit(2);
}

const [width, height] = String(args.viewport || "900x500").split("x").map(Number);
const background = String(args.background || "20,20,26").split(",").map(Number);
const time = Number(args.time || 0.5);
const point = value => (value ? String(value).split(",").map(Number) : null);
const hover = point(args.hover);
const click = point(args.click);
const name = path.basename(String(args.fixture)).replace(/\.luau$/, "") + (args.label ? `-${args.label}` : "");
const outDir = path.resolve("out/interaction/compare", name);
fs.mkdirSync(outDir, { recursive: true });
const source = fs.readFileSync(path.resolve(String(args.fixture)), "utf8");

const setup = `
local Cover = Instance.new("Frame")
Cover.Name = "Cover"
Cover.Size = UDim2.fromScale(1, 1)
Cover.BackgroundColor3 = Color3.fromRGB(${background.join(", ")})
Cover.BorderSizePixel = 0
Cover.Active = true
Cover.ZIndex = -1000
Cover.Parent = R.Gui
local Target = Instance.new("Frame")
Target.Name = "StoryTarget"
Target.Size = UDim2.fromOffset(${width}, ${height})
Target.BackgroundTransparency = 1
Target.BorderSizePixel = 0
Target.Parent = R.Gui
R.Target = Target
local Builder = (function()
${source}
end)()
local Result = Builder(Target)
if typeof(Result) == "Instance" and Result.Parent == nil then
	Result.Parent = Target
end
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));
function crop(png, w, h) {
	const out = new PNG({ width: w, height: h });
	for (let row = 0; row < h; row++) png.data.copy(out.data, row * w * 4, row * png.width * 4, (row * png.width + w) * 4);
	return out;
}

const studioFull = path.join(outDir, "studio-full.png");
let selectCentre = null;
await withStudioInput(`compare ${name}`, setup, async api => {
	const [pw, ph] = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	await api.luau(`task.wait(${time}) return ""`);
	const far = [Math.min(api.info.viewport[0] - 5, 2000), Math.min(api.info.viewport[1] - 5, 1100)];
	if (click) {
		api.post([act.batch(act.move(...far)), act.sleep(150), act.batch(act.move(...click), act.down(...click), act.up(...click)), act.sleep(300)]);
	} else if (hover) {
		api.post([act.batch(act.move(...far)), act.sleep(150), act.batch(act.move(...hover)), act.sleep(300)]);
	} else {
		api.post([act.batch(act.move(...far)), act.sleep(300)]);
	}
	const target = args.press || args.select;
	if (target) {
		selectCentre = JSON.parse(await api.luau(`local O = R.Target:FindFirstChild(${JSON.stringify(String(target))}, true)
game:GetService("GuiService").SelectedObject = O
task.wait(0.2)
${args.press ? "R.SendKey(Enum.KeyCode.Return, true) task.wait(0.3)" : ""}
return HttpService:JSONEncode({ O.AbsolutePosition.X + O.AbsoluteSize.X / 2, O.AbsolutePosition.Y + O.AbsoluteSize.Y / 2 })`));
	}
	try {
		captureViewport(api.studio.name, pw, ph, studioFull);
	} finally {
		if (target) {
			await api.luau(`${args.press ? "R.SendKey(Enum.KeyCode.Return, false) task.wait(0.1)" : ""} game:GetService("GuiService").SelectedObject = nil return ""`);
		}
	}
	return "";
});
const studioPng = crop(readPng(studioFull), Math.round(width * DPI), Math.round(height * DPI));
const studioFile = path.join(outDir, "studio.png");
fs.writeFileSync(studioFile, PNG.sync.write(studioPng));
fs.rmSync(studioFull, { force: true });

// Offline: the same input as a timeline (the selected object becomes a hover / held press on its centre)
const actions = [];
const at = Math.max(time - 0.2, 0.05);
if (click) {
	actions.push({ t: at, type: "move", x: click[0], y: click[1] }, { t: at + 0.01, type: "down", button: 1, x: click[0], y: click[1] }, { t: at + 0.02, type: "up", button: 1, x: click[0], y: click[1] });
} else if (hover) {
	actions.push({ t: at, type: "move", x: hover[0], y: hover[1] });
} else if (selectCentre) {
	actions.push({ t: at, type: "move", x: selectCentre[0], y: selectCentre[1] });
	if (args.press) actions.push({ t: at + 0.01, type: "down", button: 1, x: selectCentre[0], y: selectCentre[1] });
}
const summary = await render({ fixture: String(args.fixture), out: path.join(outDir, "rur"), viewport: `${width}x${height}`, scale: String(DPI), time: String(time), background: background.join(","), actions: JSON.stringify(actions) });
const pixels = comparePngs(readPng(summary.png), studioPng, path.join(outDir, "diff.png"));
const report = { fixture: args.fixture, hover, click, select: args.select || null, press: args.press || null, actions, pixels, files: { studio: studioFile, ours: summary.png, diff: path.join(outDir, "diff.png") } };
fs.writeFileSync(path.join(outDir, "report.json"), JSON.stringify(report, null, 2));
console.log(JSON.stringify({ name, exactPercent: +pixels.exactPercent.toFixed(3), within8Percent: +pixels.within8Percent.toFixed(3), psnr: +pixels.psnr.toFixed(2) }));
