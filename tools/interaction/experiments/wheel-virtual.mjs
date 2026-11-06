// Mouse wheel through VirtualInput:SendPointerAction over (non-Active) ScrollingFrames: no cover (the
// API refuses positions over Active CoreGui objects). Camera / selection / undo checked after.
import { withStudioInput, formatLog } from "../studio.mjs";

const setup = `
local function Scroller(Name, X, Y, W, H, CW, CH, Props, Parent)
	local S = Instance.new("ScrollingFrame")
	S.Name = Name
	S.Position = UDim2.fromOffset(X, Y)
	S.Size = UDim2.fromOffset(W, H)
	S.CanvasSize = UDim2.fromOffset(CW, CH)
	for Key, Value in Props or {} do
		S[Key] = Value
	end
	S.Parent = Parent or R.Gui
	R.Watch(S, Name, { "CanvasPosition" }, { "MouseWheelForward", "MouseWheelBackward", "InputChanged" })
	return S
end
Scroller("S1", 50, 50, 300, 300, 300, 3000)
`;

const run = await withStudioInput("wheel-virtual", setup, async api => api.luau(`
local Virtual = UIS:CreateVirtualInput()
local Out = {}
local function Try(Label, Callback)
	R.Note(Label)
	local Ok, Error = pcall(Callback)
	table.insert(Out, Label .. ": " .. (if Ok then "ok" else tostring(Error)))
	task.wait(0.3)
end
Try("move", function() Virtual:SendMousePosition(Vector2.new(200, 200)) end)
Try("wheel {Wheel=-1}", function() Virtual:SendPointerAction(Vector2.new(200, 200), { Wheel = -1 }) end)
Try("wheel {wheel=-1}", function() Virtual:SendPointerAction(Vector2.new(200, 200), { wheel = -1 }) end)
Try("wheel {Wheel=-1, Pan=0, Pinch=0}", function() Virtual:SendPointerAction(Vector2.new(200, 200), { Wheel = -1, Pan = Vector2.zero, Pinch = 0 }) end)
Try("move out", function() Virtual:SendMousePosition(Vector2.new(1900, 1100)) end)
return table.concat(Out, " | ")
`));
console.log(run.result);
console.log(formatLog(run.log.filter(e => e.p !== "MouseLocation")));
console.log(JSON.stringify(run.safety.changed));
