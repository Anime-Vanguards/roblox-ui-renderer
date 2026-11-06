// TextBox caret: click -> CursorPosition for every whole logical x across "hello world" (SourceSans 48),
// then captures of the focused box over ~2 s (caret colour / width / blink), a double-click word
// selection capture, and a focused empty box with PlaceholderText.
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
import { captureViewport } from "../../../cli/studio.mjs";
import { withStudioInput, formatLog, act, DPI } from "../studio.mjs";

const OUT = path.resolve("out/interaction/caret");
fs.mkdirSync(OUT, { recursive: true });

const setup = `
local Backdrop = Instance.new("Frame")
Backdrop.Name = "Backdrop"
Backdrop.Size = UDim2.fromScale(1, 1)
Backdrop.BackgroundColor3 = Color3.fromRGB(20, 20, 26)
Backdrop.BorderSizePixel = 0
Backdrop.ZIndex = -1000
Backdrop.Parent = R.Gui
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
T.InputEnded:Connect(function(Input)
	if Input.UserInputType == Enum.UserInputType.MouseButton1 then
		table.insert(R.Clicks, { Input.Position.X, T.CursorPosition, T.SelectionStart })
	end
end)
local P = Instance.new("TextBox")
P.Name = "P"
P.Position = UDim2.fromOffset(100, 300)
P.Size = UDim2.fromOffset(600, 80)
P.Font = Enum.Font.SourceSans
P.TextSize = 48
P.TextXAlignment = Enum.TextXAlignment.Left
P.ClearTextOnFocus = false
P.Text = ""
P.PlaceholderText = "placeholder"
P.PlaceholderColor3 = Color3.fromRGB(30, 30, 200)
P.TextColor3 = Color3.fromRGB(200, 30, 30)
P.BackgroundColor3 = Color3.fromRGB(255, 255, 255)
P.BorderSizePixel = 0
P.Parent = R.Gui
local C = Instance.new("TextBox")
C.Name = "C"
C.Position = UDim2.fromOffset(100, 500)
C.Size = UDim2.fromOffset(600, 80)
C.Font = Enum.Font.SourceSans
C.TextSize = 48
C.TextXAlignment = Enum.TextXAlignment.Center
C.ClearTextOnFocus = false
C.Text = "centred"
C.TextColor3 = Color3.fromRGB(30, 160, 30)
C.BackgroundColor3 = Color3.fromRGB(255, 255, 255)
C.BorderSizePixel = 0
C.Parent = R.Gui
`;

const readPng = file => PNG.sync.read(fs.readFileSync(file));

const run = await withStudioInput("caret", setup, async api => {
	const [pw, ph] = [Math.round(api.info.viewport[0] * DPI), Math.round(api.info.viewport[1] * DPI)];
	const result = {};
	// idle capture for reference
	api.post([act.batch(act.move(1900, 1100)), act.sleep(200)]);
	captureViewport(api.studio.name, pw, ph, path.join(OUT, "idle.png"));
	// sweep
	const actions = [];
	for (let x = 96; x <= 330; x++) {
		actions.push(act.batch(act.move(x, 140), act.down(x, 140), act.up(x, 140)), act.sleep(45));
	}
	api.post(actions);
	await api.luau(`task.wait(0.2) return ""`);
	result.clicks = JSON.parse(await api.luau(`return HttpService:JSONEncode(R.Clicks)`));
	// caret blink: click at the end, then capture repeatedly with Studio's clock
	api.post([act.batch(act.move(400, 140), act.down(400, 140), act.up(400, 140)), act.sleep(30)]);
	const focusClock = Number(await api.luau(`return tostring(os.clock())`));
	result.captures = [];
	for (let i = 0; i < 12; i++) {
		const before = Number(await api.luau(`return tostring(os.clock())`));
		const file = path.join(OUT, `blink-${i}.png`);
		captureViewport(api.studio.name, pw, ph, file);
		const after = Number(await api.luau(`return tostring(os.clock())`));
		result.captures.push({ file, before: before - focusClock, after: after - focusClock });
	}
	// word selection (double click on "hello")
	api.post([act.batch(act.move(150, 140), act.down(150, 140), act.up(150, 140)), act.sleep(60), act.batch(act.down(150, 140), act.up(150, 140)), act.sleep(300)]);
	captureViewport(api.studio.name, pw, ph, path.join(OUT, "selection.png"));
	result.selection = await api.luau(`return R.Watched.T.CursorPosition .. "," .. R.Watched.T.SelectionStart`);
	// placeholder box focused
	api.post([act.batch(act.move(300, 340), act.down(300, 340), act.up(300, 340)), act.sleep(300)]);
	captureViewport(api.studio.name, pw, ph, path.join(OUT, "placeholder-focused.png"));
	// centred box focused at the end
	api.post([act.batch(act.move(650, 540), act.down(650, 540), act.up(650, 540)), act.sleep(300)]);
	captureViewport(api.studio.name, pw, ph, path.join(OUT, "centred-focused.png"));
	result.centred = await api.luau(`local C = R.Gui.C return C.CursorPosition .. "," .. C.SelectionStart`);
	api.post([act.batch(act.move(1900, 1100), act.down(1900, 1100), act.up(1900, 1100)), act.sleep(200)]);
	return result;
});
fs.writeFileSync(path.join(OUT, "result.json"), JSON.stringify(run.result, null, 1));
console.log(JSON.stringify(run.result.clicks));
console.log(JSON.stringify(run.result.captures.map(c => [c.before.toFixed(3), c.after.toFixed(3)])));
console.log(run.result.selection, run.result.centred);
console.log(JSON.stringify(run.safety.changed));
