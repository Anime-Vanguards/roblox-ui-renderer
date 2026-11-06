// Caret blink (timed PrintWindow series right after a focusing click) and the click -> CursorPosition
// map at every whole logical x (clicks interleaved with a far click so none counts as a double click).
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { withStudioInput, act, DPI, postAndCaptureSeries } from "../studio.mjs";

const OUT = path.resolve("out/interaction/caret2");
fs.mkdirSync(OUT, { recursive: true });

const setup = `
local T = Instance.new("TextBox")
T.Name = "T"
T.Position = UDim2.fromOffset(100, 100)
T.Size = UDim2.fromOffset(600, 80)
T.Font = Enum.Font.SourceSans
T.TextSize = 48
T.TextXAlignment = Enum.TextXAlignment.Left
T.ClearTextOnFocus = false
T.Text = "hello world"
T.TextColor3 = Color3.fromRGB(200, 30, 30)
T.BackgroundColor3 = Color3.fromRGB(255, 255, 255)
T.BorderSizePixel = 0
T.Parent = R.Gui
R.Watch(T, "T", { "CursorPosition", "SelectionStart" }, { "Focused", "FocusLost" })
R.Clicks = {}
T.InputBegan:Connect(function(Input)
	if Input.UserInputType == Enum.UserInputType.MouseButton1 then
		task.defer(function()
			table.insert(R.Clicks, { Input.Position.X, T.CursorPosition, T.SelectionStart })
		end)
	end
end)
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));

const run = await withStudioInput("caret2", setup, async api => {
	const viewport = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	const result = {};
	// 1. blink: focus by clicking at the end, capture the caret area every 40 ms for 3.2 s
	const series = postAndCaptureSeries(api.studio.name, viewport, [act.move(400, 140), act.down(400, 140), act.up(400, 140)], { count: 80, intervalMs: 40, crop: [380, 150, 90, 120], outPrefix: path.join(OUT, "blink-") });
	result.blink = series.map(({ file, t }) => {
		const png = readPng(file);
		let caret = 0;
		for (let i = 0; i < png.data.length; i += 4) if (png.data[i] === 200 && png.data[i + 1] === 30 && png.data[i + 2] === 30) caret++;
		return [Math.round(t), caret];
	});
	// 2. click map
	const actions = [];
	for (let x = 98; x <= 300; x++) {
		actions.push(act.batch(act.move(x, 140), act.down(x, 140), act.up(x, 140)), act.sleep(35), act.batch(act.move(650, 140), act.down(650, 140), act.up(650, 140)), act.sleep(35));
	}
	api.post(actions);
	await api.luau(`task.wait(0.3) return ""`);
	result.clicks = JSON.parse(await api.luau(`return HttpService:JSONEncode(R.Clicks)`));
	api.post([act.batch(act.move(1900, 1100), act.down(1900, 1100), act.up(1900, 1100)), act.sleep(200)]);
	return result;
});
fs.writeFileSync(path.join(OUT, "result.json"), JSON.stringify(run.result));
console.log("blink", JSON.stringify(run.result.blink));
const map = run.result.clicks.filter(c => c[0] < 640).map(c => `${c[0]}:${c[1]}${c[2] !== -1 ? "/" + c[2] : ""}`).join(" ");
console.log("clicks", map);
console.log(JSON.stringify(run.safety.changed));
