// Hover-only: one posted WM_MOUSEMOVE (MK_SHIFT), then nothing for 1 s: how long does Hover last?
// Then a second move inside, 1 s, a WM_MOUSELEAVE, 0.5 s, a move outside.
import { withStudioInput, formatLog } from "../studio.mjs";

const setup = `
local Button = Instance.new("TextButton")
Button.Name = "Button"
Button.Position = UDim2.fromOffset(100, 100)
Button.Size = UDim2.fromOffset(200, 100)
Button.Parent = R.Gui
R.Watch(Button, "Button", { "GuiState" })
`;

const run = await withStudioInput("hover-post3", setup, async api => {
	const at = (x, y) => api.physical(x, y);
	const [x1, y1] = at(150, 150), [x2, y2] = at(170, 160), [x3, y3] = at(900, 600);
	return api.post([
		{ op: "move", x: x1, y: y1, keys: 4 },
		{ op: "sleep", ms: 1000 },
		{ op: "move", x: x2, y: y2, keys: 4 },
		{ op: "sleep", ms: 1000 },
		{ op: "leave" },
		{ op: "sleep", ms: 500 },
		{ op: "move", x: x3, y: y3, keys: 4 },
		{ op: "sleep", ms: 300 },
	]);
});
console.log(run.result);
console.log(formatLog(run.log));
console.log(JSON.stringify(run.safety.changed));
