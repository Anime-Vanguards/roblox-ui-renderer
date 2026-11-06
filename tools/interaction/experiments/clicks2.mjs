// Multi-click window: Activated's clickCount for pairs of clicks at growing intervals / distances, a
// run of 6 fast clicks, and TextBox double clicks 10 px apart. Active cover underneath.
import { withStudioInput, formatLog, act, COVER } from "../studio.mjs";

const setup = `${COVER}
local B = Instance.new("TextButton")
B.Name = "B"
B.Position = UDim2.fromOffset(100, 100)
B.Size = UDim2.fromOffset(400, 100)
B.Parent = R.Gui
R.Watch(B, "B", {}, { "Activated" })
local T = Instance.new("TextBox")
T.Name = "T"
T.Position = UDim2.fromOffset(100, 300)
T.Size = UDim2.fromOffset(600, 80)
T.Font = Enum.Font.SourceSans
T.TextSize = 48
T.TextXAlignment = Enum.TextXAlignment.Left
T.ClearTextOnFocus = false
T.Text = "hello world"
T.Parent = R.Gui
R.Watch(T, "T", { "CursorPosition", "SelectionStart" }, {})
`;

const run = await withStudioInput("clicks2", setup, async api => {
	const click = (x, y) => act.batch(act.move(x, y), act.down(x, y), act.up(x, y));
	const note = text => api.luau(`R.Note(${JSON.stringify(text)}) return ""`);
	for (const gap of [450, 500, 550, 650]) {
		await note(`pair ${gap} ms`);
		api.post([click(150, 150), act.sleep(gap), click(150, 150), act.sleep(900)]);
	}
	for (const dist of [20, 40, 100]) {
		await note(`pair 80 ms, ${dist} px`);
		api.post([click(150, 150), act.sleep(80), click(150 + dist, 150), act.sleep(900)]);
	}
	await note("6 fast clicks");
	api.post([click(150, 150), act.sleep(80), click(150, 150), act.sleep(80), click(150, 150), act.sleep(80), click(150, 150), act.sleep(80), click(150, 150), act.sleep(80), click(150, 150), act.sleep(900)]);
	await note("textbox: focus click, then double click 10 px apart");
	api.post([click(400, 340), act.sleep(900), click(150, 340), act.sleep(80), click(160, 340), act.sleep(900)]);
	await note("textbox: double click 300 ms apart");
	api.post([click(400, 340), act.sleep(900), click(150, 340), act.sleep(300), click(150, 340), act.sleep(900)]);
	await note("textbox: double click 450 ms apart");
	api.post([click(400, 340), act.sleep(900), click(150, 340), act.sleep(450), click(150, 340), act.sleep(900)]);
	await note("textbox: double click 20 px apart");
	api.post([click(400, 340), act.sleep(900), click(150, 340), act.sleep(80), click(170, 340), act.sleep(900)]);
	await note("done");
	return "";
});
console.log(formatLog(run.log.filter(e => e.note || e.e === "Activated" || e.p === "CursorPosition" || e.p === "SelectionStart")).replace(/#\d+\{MouseButton1 End key=None pos=[\d,]+ d=[-\d,]+\}, /g, ""));
console.log(JSON.stringify(run.safety.changed));
