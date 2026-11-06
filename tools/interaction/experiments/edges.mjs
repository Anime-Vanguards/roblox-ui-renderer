// Edge cases: GUI appearing / hiding under a stationary cursor, Interactable = false ancestors,
// clipping, UIS.InputEnded gameProcessed after press/release on different targets (no cover; the 3D
// view is empty), GuiState Press on a selected Frame, keyboard InputBegan receivers and InputObject
// identity across key presses (VirtualInput F14).
import { withStudioInput, formatLog, act } from "../studio.mjs";

const setup = `
local function Make(Class, Name, Parent, X, Y, W, H, Props)
	local O = Instance.new(Class)
	O.Name = Name
	O.Position = UDim2.fromOffset(X, Y)
	O.Size = UDim2.fromOffset(W, H)
	for Key, Value in Props or {} do
		O[Key] = Value
	end
	O.Parent = Parent
	R.Watch(O, Name, { "GuiState" })
	return O
end
local P = Make("Frame", "P", R.Gui, 100, 100, 300, 200, { Interactable = false })
Make("TextButton", "PC", P, 50, 50, 100, 100)
local Clip = Make("Frame", "Clip", R.Gui, 500, 100, 200, 200, { ClipsDescendants = true })
Make("TextButton", "Clipped", Clip, 150, 50, 200, 100)
Make("TextButton", "Btn", R.Gui, 900, 100, 200, 100)
Make("Frame", "Sel", R.Gui, 1200, 100, 200, 100, { Selectable = true })
Make("Frame", "HoverKey", R.Gui, 100, 500, 200, 100)
`;

const run = await withStudioInput("edges", setup, async api => {
	const note = text => api.luau(`R.Note(${JSON.stringify(text)}) return ""`);
	api.post([act.batch(act.move(1900, 1100)), act.sleep(200)]);

	await note("A1 move to empty spot (1500, 600)");
	api.post([act.batch(act.move(1500, 600)), act.sleep(300)]);
	await note("A1 create button under the stationary cursor");
	await api.luau(`local B = Instance.new("TextButton") B.Name = "Late" B.Position = UDim2.fromOffset(1400, 550) B.Size = UDim2.fromOffset(200, 100) R.Watch(B, "Late", { "GuiState" }) B.Parent = R.Gui task.wait(0.5) R.Note("A1 hide it") B.Visible = false task.wait(0.5) R.Note("A1 show it") B.Visible = true task.wait(0.5) return ""`);
	await note("A1 move within (1510, 605)");
	api.post([act.batch(act.move(1510, 605)), act.sleep(300)]);
	await note("A1 hide while hovered, then move");
	await api.luau(`R.Watched.Late.Visible = false task.wait(0.4) return ""`);
	api.post([act.batch(act.move(1515, 606)), act.sleep(300)]);

	await note("A2 hover child of Interactable=false frame");
	api.post([act.batch(act.move(1900, 1100)), act.sleep(150), act.batch(act.move(200, 200)), act.sleep(300)]);
	await note("A2 click it");
	api.post([act.batch(act.move(200, 200), act.down(200, 200), act.up(200, 200)), act.sleep(300)]);

	await note("A4 hover clipped part of button (outside clip)");
	api.post([act.batch(act.move(1900, 1100)), act.sleep(150), act.batch(act.move(750, 200)), act.sleep(300)]);
	await note("A4 hover visible part");
	api.post([act.batch(act.move(1900, 1100)), act.sleep(150), act.batch(act.move(680, 200)), act.sleep(300)]);

	await note("B1 press Btn, release over empty");
	api.post([act.batch(act.move(950, 150), act.down(950, 150), act.move(1000, 800, 1), act.up(1000, 800)), act.sleep(300)]);
	await note("B1 press empty, release over Btn");
	api.post([act.batch(act.move(1000, 800), act.down(1000, 800), act.move(950, 150, 1), act.up(950, 150)), act.sleep(300)]);

	await note("B2 select Frame Sel and hold Return");
	await api.luau(`game:GetService("GuiService").SelectedObject = R.Watched.Sel task.wait(0.2) R.SendKey(Enum.KeyCode.Return, true) task.wait(0.3) R.Note("B2 state " .. tostring(R.Watched.Sel.GuiState)) R.SendKey(Enum.KeyCode.Return, false) task.wait(0.2) game:GetService("GuiService").SelectedObject = nil task.wait(0.2) return ""`);

	await note("C1 hover HoverKey then F14 twice");
	api.post([act.batch(act.move(1900, 1100)), act.sleep(150), act.batch(act.move(200, 550)), act.sleep(300)]);
	await api.luau(`R.SendKey(Enum.KeyCode.F14, true) task.wait(0.15) R.SendKey(Enum.KeyCode.F14, false) task.wait(0.15) R.SendKey(Enum.KeyCode.F14, true) task.wait(0.15) R.SendKey(Enum.KeyCode.F14, false) task.wait(0.2) return ""`);
	api.post([act.batch(act.move(1900, 1100)), act.sleep(200)]);
	return "";
});
console.log(formatLog(run.log.filter(e => e.p !== "MouseLocation" && !(e.a && e.a[0] && e.a[0].input === "MouseMovement" && e.a[0].pos[0] > 1800))));
console.log(JSON.stringify(run.safety.changed));
