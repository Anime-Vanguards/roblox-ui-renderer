// Wheel scrolling through VirtualInput:SendPointerAction (no cover; a full-screen "Catcher"
// ScrollingFrame behind everything takes any wheel the test frames do not use, so the Studio camera
// never zooms). Step per notch vs frame size, direction, accumulation, nesting, ScrollingEnabled.
import fs from "node:fs";
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
	R.Watch(S, Name, { "CanvasPosition" }, { "MouseWheelForward", "MouseWheelBackward" })
	return S
end
Scroller("Catcher", 0, 0, 2060, 1150, 2060, 40000, { ZIndex = -10 })
Scroller("S1", 50, 50, 300, 300, 300, 3000)
Scroller("S2", 400, 50, 600, 150, 600, 3000)
Scroller("S8", 400, 250, 300, 100, 300, 3000)
Scroller("S3", 1050, 50, 300, 300, 3000, 300)
Scroller("S5", 1400, 50, 300, 600, 300, 20000)
local Outer = Scroller("Outer", 50, 450, 400, 400, 400, 4000)
Scroller("Inner", 20, 20, 200, 200, 200, 400, nil, Outer)
Scroller("S6", 500, 450, 300, 300, 3000, 3000)
Scroller("S7", 850, 450, 300, 300, 300, 3000, { ScrollingEnabled = false })
Scroller("S9", 850, 800, 300, 300, 300, 3000, { ScrollingDirection = Enum.ScrollingDirection.X })
`;

const run = await withStudioInput("wheel2", setup, async api => api.luau(`
local Virtual = UIS:CreateVirtualInput()
local Out = {}
local function Wheel(Label, X, Y, Amount, Count, Gap)
	Virtual:SendMousePosition(Vector2.new(X, Y))
	task.wait(0.05)
	R.Note(Label)
	for Index = 1, Count or 1 do
		local Ok, Error = pcall(function()
			Virtual:SendPointerAction(Vector2.new(X, Y), { Wheel = Amount })
		end)
		if not Ok then
			table.insert(Out, Label .. ": " .. tostring(Error))
		end
		if Index < (Count or 1) then
			task.wait(Gap or 0.05)
		end
	end
	task.wait(0.6)
end
Wheel("S1 down 1", 200, 200, -1)
Wheel("S1 up 1", 200, 200, 1)
Wheel("S1 down 3 fast", 200, 200, -1, 3, 0.05)
Wheel("S1 down 1 wheel=-2", 200, 200, -2)
Wheel("S1 down 1 wheel=-0.5", 200, 200, -0.5)
Wheel("S2 down 1 (600x150)", 700, 125, -1)
Wheel("S8 down 1 (300x100)", 550, 300, -1)
Wheel("S5 down 1 (300x600)", 1550, 300, -1)
Wheel("S3 down 1 (horizontal canvas)", 1200, 200, -1)
Wheel("S6 down 1 (XY canvas)", 650, 600, -1)
Wheel("S9 down 1 (direction X, canvas Y)", 1000, 900, -1)
Wheel("Inner down 1", 150, 550, -1)
Wheel("Inner down 3 more", 150, 550, -1, 3, 0.3)
Wheel("S7 down 1 (ScrollingEnabled false)", 1000, 600, -1)
Wheel("S1 up 20 (to top, then over)", 200, 200, 1, 20, 0.02)
Virtual:SendMousePosition(Vector2.new(1900, 1100))
task.wait(0.1)
return table.concat(Out, " | ")
`));
fs.writeFileSync("out/interaction/wheel2.json", JSON.stringify(run.log));
console.log(run.result);
// last CanvasPosition per note section
let section = "start";
const last = {};
for (const e of run.log) {
	if (e.note) {
		section = e.note;
		continue;
	}
	if (e.p === "CanvasPosition") last[`${section} :: ${e.o}`] = e.v;
}
for (const [k, v] of Object.entries(last)) console.log(k.padEnd(60), JSON.stringify(v));
console.log(formatLog(run.log.filter(e => e.e)).split("\n").slice(0, 60).join("\n"));
console.log(JSON.stringify(run.safety.changed));
