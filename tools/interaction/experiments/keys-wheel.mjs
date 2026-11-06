// Keyboard gameProcessed / receivers over an Active frame (posted hover + VirtualInput F14), and which
// objects get wheel events when the wheel is over a plain Frame / a child of a ScrollingFrame
// (VirtualInput wheel; a full-screen "Catcher" ScrollingFrame takes unused wheel so the camera never zooms).
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
	R.Watch(O, Name, {}, { "InputBegan", "InputEnded", "InputChanged", "MouseWheelForward", "MouseWheelBackward" })
	return O
end
local Catcher = Make("ScrollingFrame", "Catcher", R.Gui, 0, 0, 2060, 1150, { CanvasSize = UDim2.fromOffset(2060, 40000), ZIndex = -10 })
Make("Frame", "ActiveFrame", R.Gui, 100, 100, 200, 200, { Active = true })
Make("Frame", "Plain", R.Gui, 400, 100, 200, 200)
local S = Make("ScrollingFrame", "S", R.Gui, 700, 100, 300, 300, { CanvasSize = UDim2.fromOffset(300, 3000) })
Make("Frame", "Child", S, 10, 10, 200, 100)
`;

const run = await withStudioInput("keys-wheel", setup, async api => {
	const note = text => api.luau(`R.Note(${JSON.stringify(text)}) return ""`);
	await note("hover ActiveFrame, F14");
	api.post([act.batch(act.move(1900, 1100)), act.sleep(150), act.batch(act.move(200, 200)), act.sleep(250)]);
	await api.luau(`R.SendKey(Enum.KeyCode.F14, true) task.wait(0.1) R.SendKey(Enum.KeyCode.F14, false) task.wait(0.2) return ""`);
	await note("wheel over Plain (VirtualInput)");
	await api.luau(`local V = UIS:CreateVirtualInput() V:SendMousePosition(Vector2.new(500, 200)) task.wait(0.15) V:SendPointerAction(Vector2.new(500, 200), { Wheel = -1 }) task.wait(0.5) return ""`);
	await note("wheel over S.Child (VirtualInput)");
	await api.luau(`local V = UIS:CreateVirtualInput() V:SendMousePosition(Vector2.new(800, 150)) task.wait(0.15) V:SendPointerAction(Vector2.new(800, 150), { Wheel = -1 }) task.wait(0.5) V:SendMousePosition(Vector2.new(1900, 1100)) task.wait(0.1) return ""`);
	await note("done");
	return "";
});
console.log(formatLog(run.log.filter(e => e.note || (e.e && !(e.a && e.a[0] && e.a[0].input === "MouseMovement")))));
console.log(JSON.stringify(run.safety.changed));
