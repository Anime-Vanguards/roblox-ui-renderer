// Hover-only: WM_MOUSEMOVE posted to the Studio viewport HWND. Does it move the engine mouse and
// hover CoreGui buttons? (No clicks.)
import { withStudioInput, formatLog } from "../studio.mjs";

const setup = `
local Button = Instance.new("TextButton")
Button.Name = "Button"
Button.Position = UDim2.fromOffset(100, 100)
Button.Size = UDim2.fromOffset(200, 100)
Button.Parent = R.Gui
R.Watch(Button, "Button", { "GuiState", "BackgroundColor3" })
local Frame = Instance.new("Frame")
Frame.Name = "Frame"
Frame.Position = UDim2.fromOffset(400, 100)
Frame.Size = UDim2.fromOffset(200, 100)
Frame.Parent = R.Gui
R.Watch(Frame, "Frame")
`;

const run = await withStudioInput("hover-post", setup, async api => {
	const out = [];
	const at = (x, y) => api.physical(x, y);
	out.push(api.post([
		{ op: "move", x: at(150, 150)[0], y: at(150, 150)[1] },
		{ op: "sleep", ms: 250 },
		{ op: "move", x: at(160, 155)[0], y: at(160, 155)[1] },
		{ op: "sleep", ms: 250 },
		{ op: "move", x: at(500, 150)[0], y: at(500, 150)[1] },
		{ op: "sleep", ms: 250 },
		{ op: "move", x: at(900, 600)[0], y: at(900, 600)[1] },
		{ op: "sleep", ms: 250 },
	]));
	out.push(await api.luau(`task.wait(0.1) return tostring(UIS:GetMouseLocation())`));
	return out;
});
console.log(run.result.join("\n"));
console.log(formatLog(run.log));
console.log(JSON.stringify(run.safety));
