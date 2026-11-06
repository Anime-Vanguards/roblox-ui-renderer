// TextBox editing through VirtualInput keys / text (focus by CaptureFocus): cursor placement, insert,
// arrows, Home/End, Backspace/Delete, Shift selection, Ctrl+A, Ctrl word moves, Return, MultiLine,
// ClearTextOnFocus, TextEditable, ReleaseFocus. Selection empty -> stray keys can do nothing.
import fs from "node:fs";
import { withStudioInput, formatLog } from "../studio.mjs";

const setup = `
local function Box(Name, Y, Props)
	local T = Instance.new("TextBox")
	T.Name = Name
	T.Position = UDim2.fromOffset(100, Y)
	T.Size = UDim2.fromOffset(400, 50)
	T.Font = Enum.Font.SourceSans
	T.TextSize = 24
	T.TextXAlignment = Enum.TextXAlignment.Left
	T.ClearTextOnFocus = false
	for Key, Value in Props or {} do
		T[Key] = Value
	end
	T.Parent = R.Gui
	R.Watch(T, Name, { "Text", "CursorPosition", "SelectionStart", "GuiState" }, { "Focused", "FocusLost", "InputBegan", "InputEnded" })
	return T
end
Box("T1", 100, { Text = "hello world" })
Box("T2", 200, { Text = "abc", ClearTextOnFocus = true })
Box("T3", 300, { Text = "line one" .. string.char(10) .. "line two", MultiLine = true, TextWrapped = true, Size = UDim2.fromOffset(300, 150) })
Box("T4", 500, { Text = "fixed", TextEditable = false })
Box("T5", 600, { Text = "", PlaceholderText = "type here" })
`;

const steps = `
local K = Enum.KeyCode
local function Note(Text)
	R.Note(Text)
end
local function Tap(Code, Modifier)
	if Modifier then
		R.SendKey(Modifier, true)
		task.wait(0.03)
	end
	R.SendKey(Code, true)
	task.wait(0.05)
	R.SendKey(Code, false)
	if Modifier then
		task.wait(0.03)
		R.SendKey(Modifier, false)
	end
	task.wait(0.08)
end
local function Type(Text)
	R.SendText(Text)
	task.wait(0.1)
end
local function Focused(Box)
	return UIS:GetFocusedTextBox() == Box
end
local W = R.Watched
local Out = {}
local function Check(Box, Label)
	table.insert(Out, string.format("%s: text=%q cursor=%d sel=%d focused=%s", Label, Box.Text, Box.CursorPosition, Box.SelectionStart, tostring(Focused(Box))))
end

Note("T1 CaptureFocus")
W.T1:CaptureFocus()
task.wait(0.15)
Check(W.T1, "after focus")
if Focused(W.T1) then
	Note("type X") Type("X") Check(W.T1, "X")
	Note("Left x2, type Y") Tap(K.Left) Tap(K.Left) Type("Y") Check(W.T1, "Y")
	Note("Home, type Z") Tap(K.Home) Type("Z") Check(W.T1, "Z")
	Note("End, type W") Tap(K.End) Type("W") Check(W.T1, "W")
	Note("Backspace") Tap(K.Backspace) Check(W.T1, "BS")
	Note("Home, Delete") Tap(K.Home) Tap(K.Delete) Check(W.T1, "Del")
	Note("Right x3, Shift+Right x2") Tap(K.Right) Tap(K.Right) Tap(K.Right) Tap(K.Right, K.LeftShift) Tap(K.Right, K.LeftShift) Check(W.T1, "shift sel")
	Note("type Q over selection") Type("Q") Check(W.T1, "Q")
	Note("Shift+Home") Tap(K.Home, K.LeftShift) Check(W.T1, "shift home")
	Note("Right (collapse)") Tap(K.Right) Check(W.T1, "right collapse")
	Note("Ctrl+Left") Tap(K.Left, K.LeftControl) Check(W.T1, "ctrl left")
	Note("Ctrl+Left again") Tap(K.Left, K.LeftControl) Check(W.T1, "ctrl left 2")
	Note("Ctrl+Right") Tap(K.Right, K.LeftControl) Check(W.T1, "ctrl right")
	Note("Ctrl+Backspace") Tap(K.Backspace, K.LeftControl) Check(W.T1, "ctrl bs")
	Note("Ctrl+A") Tap(K.A, K.LeftControl) Check(W.T1, "ctrl a")
	Note("type R over all") Type("R") Check(W.T1, "R")
	Note("type multi 'st u'") Type("st u") Check(W.T1, "st u")
	Note("Return") Tap(K.Return) Check(W.T1, "return")
end
task.wait(0.2)

Note("T2 CaptureFocus (ClearTextOnFocus)")
W.T2:CaptureFocus()
task.wait(0.15)
Check(W.T2, "T2 focus")
if Focused(W.T2) then
	Note("type ok") Type("ok") Check(W.T2, "T2 ok")
	Note("T2 ReleaseFocus()") W.T2:ReleaseFocus() task.wait(0.15) Check(W.T2, "T2 released")
end

Note("T3 CaptureFocus (MultiLine)")
W.T3:CaptureFocus()
task.wait(0.15)
Check(W.T3, "T3 focus")
if Focused(W.T3) then
	Note("Up") Tap(K.Up) Check(W.T3, "T3 up")
	Note("End") Tap(K.End) Check(W.T3, "T3 end")
	Note("Return (newline?)") Tap(K.Return) Check(W.T3, "T3 return")
	Note("type nl") Type("nl") Check(W.T3, "T3 nl")
	Note("Down") Tap(K.Down) Check(W.T3, "T3 down")
	Note("Home") Tap(K.Home) Check(W.T3, "T3 home")
	Note("Shift+Return") Tap(K.Return, K.LeftShift) Check(W.T3, "T3 shift return")
	Note("Escape") Tap(K.Escape) Check(W.T3, "T3 escape")
	if Focused(W.T3) then
		W.T3:ReleaseFocus(true)
		task.wait(0.15)
		Check(W.T3, "T3 ReleaseFocus(true)")
	end
end

Note("T4 CaptureFocus (TextEditable false)")
W.T4:CaptureFocus()
task.wait(0.15)
Check(W.T4, "T4 focus")
if Focused(W.T4) then
	Note("type x") Type("x") Check(W.T4, "T4 x")
	Note("Backspace") Tap(K.Backspace) Check(W.T4, "T4 bs")
	Note("Tab") Tap(K.Tab) Check(W.T4, "T4 tab")
	if Focused(W.T4) then
		W.T4:ReleaseFocus()
		task.wait(0.1)
	end
end

Note("T5 CaptureFocus (placeholder)")
W.T5:CaptureFocus()
task.wait(0.15)
Check(W.T5, "T5 focus")
if Focused(W.T5) then
	Note("Tab") Tap(K.Tab) Check(W.T5, "T5 tab")
	Note("type a") Type("a") Check(W.T5, "T5 a")
	if Focused(W.T5) then
		W.T5:ReleaseFocus()
		task.wait(0.1)
	end
end
Note("script sets Text / CursorPosition on unfocused T1")
W.T1.CursorPosition = 3
task.wait(0.1)
Check(W.T1, "T1 cursor=3 set")
W.T1.Text = "zz"
task.wait(0.1)
Check(W.T1, "T1 text=zz set")
return table.concat(Out, string.char(10))
`;

const run = await withStudioInput("textbox", setup, async api => api.luau(steps));
fs.writeFileSync("out/interaction/textbox.json", JSON.stringify(run.log));
console.log(run.result);
console.log("----");
console.log(formatLog(run.log.filter(e => e.p !== "MouseLocation" && e.p !== "KeysPressed")));
console.log(JSON.stringify(run.safety.changed));
