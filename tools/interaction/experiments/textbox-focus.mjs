// TextBox focus without keys (VirtualInput refuses keys while a CoreGui TextBox has focus): CaptureFocus
// / ReleaseFocus, ClearTextOnFocus, CursorPosition / SelectionStart after focus, SetTextFromInput,
// script writes while focused, click focus + caret placement (posted clicks) and click-outside.
import fs from "node:fs";
import { withStudioInput, formatLog, act } from "../studio.mjs";

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
	R.Watch(T, Name, { "Text", "CursorPosition", "SelectionStart", "GuiState" }, { "Focused", "FocusLost", "InputBegan", "InputEnded", "MouseEnter", "MouseLeave" })
	return T
end
Box("T1", 100, { Text = "hello world" })
Box("T2", 200, { Text = "abc", ClearTextOnFocus = true })
Box("T3", 300, { Text = "line one" .. string.char(10) .. "line two", MultiLine = true, TextWrapped = true, Size = UDim2.fromOffset(300, 150) })
Box("T4", 500, { Text = "fixed", TextEditable = false })
Box("T5", 600, { Text = "", PlaceholderText = "type here" })
local F = Instance.new("Frame")
F.Name = "Outside"
F.Position = UDim2.fromOffset(800, 100)
F.Size = UDim2.fromOffset(200, 200)
F.Parent = R.Gui
`;

const luauSteps = `
local W = R.Watched
local Out = {}
local function Check(Box, Label)
	table.insert(Out, string.format("%s: text=%q cursor=%d sel=%d focused=%s", Label, Box.Text, Box.CursorPosition, Box.SelectionStart, tostring(UIS:GetFocusedTextBox() == Box)))
end
Check(W.T1, "T1 initial")
R.Note("T1 CaptureFocus") W.T1:CaptureFocus() task.wait(0.15) Check(W.T1, "T1 focused")
R.Note("T1 SetTextFromInput('abc')") local Ok, Err = pcall(function() W.T1:SetTextFromInput("abc") end) task.wait(0.15) Check(W.T1, "T1 SetTextFromInput " .. tostring(Ok) .. " " .. tostring(Err))
R.Note("T1 script Text = 'scripted text'") W.T1.Text = "scripted text" task.wait(0.15) Check(W.T1, "T1 script text")
R.Note("T1 script CursorPosition = 3") W.T1.CursorPosition = 3 task.wait(0.15) Check(W.T1, "T1 cursor 3")
R.Note("T1 script SelectionStart = 1") W.T1.SelectionStart = 1 task.wait(0.15) Check(W.T1, "T1 sel 1")
R.Note("T1 script CursorPosition = 100") W.T1.CursorPosition = 100 task.wait(0.15) Check(W.T1, "T1 cursor 100")
R.Note("T2 CaptureFocus (steals focus from T1)") W.T2:CaptureFocus() task.wait(0.15) Check(W.T1, "T1 after T2 focus") Check(W.T2, "T2 focused")
R.Note("T2 ReleaseFocus(true)") W.T2:ReleaseFocus(true) task.wait(0.15) Check(W.T2, "T2 released true")
R.Note("T3 CaptureFocus") W.T3:CaptureFocus() task.wait(0.15) Check(W.T3, "T3 focused")
R.Note("T3 ReleaseFocus()") W.T3:ReleaseFocus() task.wait(0.15)
R.Note("T4 CaptureFocus (TextEditable false)") W.T4:CaptureFocus() task.wait(0.15) Check(W.T4, "T4 focused")
R.Note("T4 ReleaseFocus") W.T4:ReleaseFocus() task.wait(0.15)
R.Note("T5 CaptureFocus (placeholder)") W.T5:CaptureFocus() task.wait(0.15) Check(W.T5, "T5 focused")
R.Note("T5 ReleaseFocus") W.T5:ReleaseFocus() task.wait(0.15)
R.Note("T1 Text = 'hello world' unfocused, CursorPosition = 4") W.T1.Text = "hello world" W.T1.CursorPosition = 4 task.wait(0.15) Check(W.T1, "T1 unfocused set")
return table.concat(Out, string.char(10))
`;

const run = await withStudioInput("textbox-focus", setup, async api => {
	const out = [await api.luau(luauSteps)];
	const note = text => api.luau(`R.Note(${JSON.stringify(text)}) return ""`);
	const state = label => api.luau(`local T = UIS:GetFocusedTextBox() local W = R.Watched.T1 return string.format("%s: focused=%s cursor=%d sel=%d text=%q", ${JSON.stringify(label)}, T and T.Name or "none", W.CursorPosition, W.SelectionStart, W.Text)`);
	// click positions along T1's text (x from 100 = left edge); SourceSans 24 "hello world"
	for (const x of [101, 105, 108, 112, 130, 160, 200, 250, 400]) {
		await note(`click T1 at x=${x}`);
		api.post([act.batch(act.move(1900, 1100)), act.sleep(120), act.batch(act.move(x, 125), act.down(x, 125), act.up(x, 125)), act.sleep(250)]);
		out.push(await state(`click x=${x}`));
	}
	await note("click outside (frame)");
	api.post([act.batch(act.move(900, 200), act.down(900, 200), act.up(900, 200)), act.sleep(250)]);
	out.push(await state("after outside click"));
	await note("click T5 (empty, placeholder)");
	api.post([act.batch(act.move(300, 625), act.down(300, 625), act.up(300, 625)), act.sleep(250)]);
	out.push(await api.luau(`local T = UIS:GetFocusedTextBox() return "T5 click: focused=" .. (T and T.Name or "none") .. " cursor=" .. R.Watched.T5.CursorPosition`));
	await note("click T2 (ClearTextOnFocus)");
	api.post([act.batch(act.move(300, 225), act.down(300, 225), act.up(300, 225)), act.sleep(250)]);
	out.push(await api.luau(`local T = UIS:GetFocusedTextBox() return "T2 click: focused=" .. (T and T.Name or "none") .. " text=" .. R.Watched.T2.Text .. " cursor=" .. R.Watched.T2.CursorPosition`));
	await note("drag-select in T1 from x=110 to x=200");
	api.post([act.batch(act.move(110, 125), act.down(110, 125), act.move(150, 125, 1), act.move(200, 125, 1), act.up(200, 125)), act.sleep(250)]);
	out.push(await state("drag select"));
	await note("double click T1 at x=130");
	api.post([act.batch(act.move(130, 125), act.down(130, 125), act.up(130, 125)), act.sleep(60), act.batch(act.down(130, 125), act.up(130, 125)), act.sleep(250)]);
	out.push(await state("double click"));
	api.post([act.batch(act.move(900, 200), act.down(900, 200), act.up(900, 200)), act.sleep(250)]);
	return out;
});
fs.writeFileSync("out/interaction/textbox-focus.json", JSON.stringify(run.log));
console.log(run.result.join("\n"));
console.log("----");
console.log(formatLog(run.log.filter(e => e.p !== "MouseLocation" && e.p !== "KeysPressed" && !(e.a && e.a[0] && e.a[0].input === "MouseMovement"))));
console.log(JSON.stringify(run.safety.changed));
