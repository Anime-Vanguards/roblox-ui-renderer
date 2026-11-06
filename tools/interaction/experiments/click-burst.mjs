// Posted clicks in bursts: messages posted back to back are handled before the WM_MOUSELEAVE that
// Qt's TrackMouseEvent queues (the real cursor is elsewhere), so a burst sees one enter/leave pair.
import { withStudioInput, formatLog, act, COVER } from "../studio.mjs";

const setup = `${COVER}
local Button = Instance.new("TextButton")
Button.Name = "Button"
Button.Position = UDim2.fromOffset(100, 100)
Button.Size = UDim2.fromOffset(200, 100)
Button.Parent = R.Gui
R.Watch(Button, "Button", { "GuiState", "BackgroundColor3" })
`;

const run = await withStudioInput("click-burst", setup, async api => {
	const out = [];
	const note = text => api.luau(`R.Note(${JSON.stringify(text)}) return ""`);
	await note("A: burst [move out, move in]");
	out.push(api.post([act.move(900, 600), act.move(150, 150), act.sleep(400)]));
	await note("B: burst [move in 2, down]");
	out.push(api.post([act.move(152, 151), act.down(152, 151), act.sleep(400)]));
	await note("C: burst [up]");
	out.push(api.post([act.up(152, 151), act.sleep(400)]));
	await note("D: burst [move out, move in, down, up]");
	out.push(api.post([act.move(900, 600), act.move(160, 160), act.down(160, 160), act.up(160, 160), act.sleep(400)]));
	await note("E: burst [move out]");
	out.push(api.post([act.move(900, 600), act.sleep(300)]));
	return out;
});
console.log(formatLog(run.log));
console.log(JSON.stringify(run.safety.changed), run.safety.after.selection);
