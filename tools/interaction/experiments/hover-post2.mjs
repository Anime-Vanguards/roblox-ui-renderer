// Hover-only: WM_MOUSEMOVE with wParam = MK_SHIFT (Qt 5 discards button-less moves while it has no
// window under the mouse, which is always the case when the real cursor is elsewhere).
import { withStudioInput, formatLog } from "../studio.mjs";

const setup = `
local Button = Instance.new("TextButton")
Button.Name = "Button"
Button.Position = UDim2.fromOffset(100, 100)
Button.Size = UDim2.fromOffset(200, 100)
Button.Parent = R.Gui
R.Watch(Button, "Button", { "GuiState" })
`;

const keys = Number(process.argv[2] || 4);
const run = await withStudioInput("hover-post2", setup, async api => {
	const out = [];
	const at = (x, y) => api.physical(x, y);
	const actions = [];
	for (let i = 0; i < 10; i++) {
		const [x, y] = at(150 + i * 3, 150 + i);
		actions.push({ op: "move", x, y, keys }, { op: "sleep", ms: 40 });
	}
	actions.push({ op: "sleep", ms: 300 });
	const [x2, y2] = at(900, 600);
	actions.push({ op: "move", x: x2, y: y2, keys }, { op: "sleep", ms: 300 });
	out.push(api.post(actions));
	out.push(await api.luau(`task.wait(0.1) return tostring(UIS:GetMouseLocation())`));
	return out;
});
console.log(run.result.join("\n"));
console.log(formatLog(run.log));
console.log(JSON.stringify(run.safety.changed));
