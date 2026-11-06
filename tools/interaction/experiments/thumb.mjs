// Scroll bar thumb dragging and track clicks through VirtualInput (a ScrollingFrame is not Active, so the
// API accepts positions over it; nothing Active is under the points). The 3D view behind is empty.
import fs from "node:fs";
import { withStudioInput, formatLog } from "../studio.mjs";

const setup = `
local S = Instance.new("ScrollingFrame")
S.Name = "S"
S.Position = UDim2.fromOffset(100, 100)
S.Size = UDim2.fromOffset(300, 300)
S.CanvasSize = UDim2.fromOffset(300, 3000)
S.Parent = R.Gui
R.Watch(S, "S", { "CanvasPosition" }, { "InputBegan", "InputEnded", "MouseEnter", "MouseLeave" })
local H = Instance.new("ScrollingFrame")
H.Name = "H"
H.Position = UDim2.fromOffset(500, 100)
H.Size = UDim2.fromOffset(300, 300)
H.CanvasSize = UDim2.fromOffset(3000, 300)
H.Parent = R.Gui
R.Watch(H, "H", { "CanvasPosition" }, { "InputBegan", "InputEnded" })
`;

const steps = `
local V = UIS:CreateVirtualInput()
local function Try(Label, Callback)
	R.Note(Label)
	local Ok, Error = pcall(Callback)
	if not Ok then
		R.Note("ERROR " .. tostring(Error))
	end
end
local MB1 = Enum.UserInputType.MouseButton1
local function Move(X, Y) V:SendMousePosition(Vector2.new(X, Y)) end
local function Down(X, Y) V:SendMouseButton(Vector2.new(X, Y), MB1, true, 1) end
local function Up(X, Y) V:SendMouseButton(Vector2.new(X, Y), MB1, false, 1) end
local Pressed = false
local Ok, Error = pcall(function()
	Try("thumb press at (394, 115)", function() Move(394, 115) task.wait(0.1) Down(394, 115) Pressed = true end)
	task.wait(0.2)
	Try("drag to (394, 165)", function() Move(394, 165) end)
	task.wait(0.3)
	Try("drag to (394, 265)", function() Move(394, 265) end)
	task.wait(0.3)
	Try("drag outside to (600, 265)", function() Move(600, 265) end)
	task.wait(0.3)
	Try("drag to (600, 900)", function() Move(600, 900) end)
	task.wait(0.3)
	Try("release", function() Up(600, 900) Pressed = false end)
	task.wait(0.3)
	Try("track click below thumb (394, 350)", function() Move(394, 350) task.wait(0.1) Down(394, 350) task.wait(0.05) Up(394, 350) end)
	task.wait(0.5)
	Try("track press-and-hold above thumb (394, 120)", function() Move(394, 120) task.wait(0.1) Down(394, 120) Pressed = true end)
	task.wait(1.0)
	Try("release hold", function() Up(394, 120) Pressed = false end)
	task.wait(0.3)
	Try("H thumb press (515, 394) then drag right 100", function() Move(515, 394) task.wait(0.1) Down(515, 394) Pressed = true task.wait(0.1) Move(615, 394) task.wait(0.3) Up(615, 394) Pressed = false end)
	task.wait(0.3)
	Try("content press-drag (200, 200) -> (200, 100)", function() Move(200, 200) task.wait(0.1) Down(200, 200) Pressed = true task.wait(0.1) Move(200, 150) task.wait(0.1) Move(200, 100) task.wait(0.3) Up(200, 100) Pressed = false end)
	task.wait(0.5)
	Try("move out", function() Move(1900, 1100) end)
	task.wait(0.1)
end)
if Pressed then
	pcall(function() Up(1900, 1100) end)
end
if not Ok then
	return "ERROR " .. tostring(Error)
end
return "done"
`;

const run = await withStudioInput("thumb", setup, async api => api.luau(steps));
fs.writeFileSync("out/interaction/thumb.json", JSON.stringify(run.log));
console.log(run.result);
let section = "";
const lines = [];
for (const e of run.log) {
	if (e.note) {
		section = e.note;
		lines.push(`-- ${e.note} (f${e.f})`);
		continue;
	}
	if (e.p === "CanvasPosition") lines.push(`   f${e.f} ${e.o} ${JSON.stringify(e.v.map(v => Math.round(v * 100) / 100))}`);
	else if (e.e && !/UIS/.test(e.e)) lines.push(`   f${e.f} ${e.o}.${e.e} ${e.a && e.a[0] && e.a[0].input ? e.a[0].input + " " + e.a[0].state : ""}`);
}
// collapse consecutive CanvasPosition lines: keep first/last of each run
const out = [];
for (let i = 0; i < lines.length; i++) {
	if (lines[i].includes("CanvasPosition") && i > 0 && i + 1 < lines.length && lines[i - 1].includes("CanvasPosition") && lines[i + 1].includes("CanvasPosition")) continue;
	out.push(lines[i]);
}
console.log(out.join("\n"));
console.log(JSON.stringify(run.safety.changed));
