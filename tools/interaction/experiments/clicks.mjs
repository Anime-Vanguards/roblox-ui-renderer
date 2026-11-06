// Activated's clickCount for fast repeated clicks (separate bursts), and whether the caret shows while
// a TextBox selection exists (timed capture series after a double click). Active cover underneath.
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { withStudioInput, formatLog, act, DPI, COVER, postAndCaptureSeries } from "../studio.mjs";

const OUT = path.resolve("out/interaction/clicks");
fs.mkdirSync(OUT, { recursive: true });

const setup = `${COVER}
local B = Instance.new("TextButton")
B.Name = "B"
B.Position = UDim2.fromOffset(100, 100)
B.Size = UDim2.fromOffset(200, 100)
B.Parent = R.Gui
R.Watch(B, "B", {}, { "Activated", "MouseButton1Click" })
local T = Instance.new("TextBox")
T.Name = "T"
T.Position = UDim2.fromOffset(100, 300)
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
R.Watch(T, "T", { "CursorPosition", "SelectionStart" }, {})
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));

const run = await withStudioInput("clicks", setup, async api => {
	const viewport = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	const click = (x, y) => act.batch(act.move(x, y), act.down(x, y), act.up(x, y));
	await api.luau(`R.Note("4 clicks 80 ms apart") return ""`);
	api.post([click(150, 150), act.sleep(80), click(150, 150), act.sleep(80), click(150, 150), act.sleep(80), click(150, 150), act.sleep(600)]);
	await api.luau(`R.Note("2 clicks 400 ms apart") return ""`);
	api.post([click(150, 150), act.sleep(400), click(150, 150), act.sleep(700)]);
	await api.luau(`R.Note("2 clicks 80 ms apart, 10 px apart") return ""`);
	api.post([click(150, 150), act.sleep(80), click(160, 150), act.sleep(700)]);
	await api.luau(`R.Note("textbox double click then capture series") return ""`);
	api.post([click(400, 340), act.sleep(700)]);
	const series = postAndCaptureSeries(api.studio.name, viewport, [act.move(150, 340), act.down(150, 340), act.up(150, 340)], { count: 1, intervalMs: 10, crop: [140, 440, 20, 20], outPrefix: path.join(OUT, "first-") });
	void series;
	const second = postAndCaptureSeries(api.studio.name, viewport, [act.down(150, 340), act.up(150, 340)], { count: 40, intervalMs: 40, crop: [260, 450, 30, 90], outPrefix: path.join(OUT, "sel-") });
	const blink = second.map(({ file, t }) => {
		const png = readPng(file);
		let caret = 0;
		for (let i = 0; i < png.data.length; i += 4) if (png.data[i] === 200 && png.data[i + 1] === 30 && png.data[i + 2] === 30) caret++;
		return [Math.round(t), caret];
	});
	await api.luau(`R.Note("done") return ""`);
	return blink;
});
console.log("caret px in [260..290] x [450..540] after double click:", JSON.stringify(run.result));
console.log(formatLog(run.log.filter(e => e.note || e.e === "Activated" || e.e === "MouseButton1Click" || e.p === "CursorPosition" || e.p === "SelectionStart")));
console.log(JSON.stringify(run.safety.changed));
