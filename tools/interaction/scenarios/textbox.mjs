// TextBox focus, caret placement and selection without keys: CaptureFocus / ReleaseFocus / focus
// stealing, script writes, clicks at several x, double and triple clicks, drag selection and a click
// outside. Active cover underneath (clicks outside land on it).
import { act, COVER } from "../studio.mjs";

const setup = `${COVER}
local function Box(Name, Y, Props)
	local T = Instance.new("TextBox")
	T.Name = Name
	T.Position = UDim2.fromOffset(100, Y)
	T.Size = UDim2.fromOffset(600, 80)
	T.Font = Enum.Font.SourceSans
	T.TextSize = 48
	T.TextXAlignment = Enum.TextXAlignment.Left
	T.ClearTextOnFocus = false
	for Key, Value in Props or {} do
		T[Key] = Value
	end
	T.Parent = R.Gui
	R.Watch(T, Name, { "Text", "CursorPosition", "SelectionStart" }, { "Focused", "FocusLost", "InputBegan", "InputEnded" })
	return T
end
Box("T1", 100, { Text = "hello world" })
Box("T2", 250, { Text = "abc", ClearTextOnFocus = true })
Box("T3", 400, { Text = "centred", TextXAlignment = Enum.TextXAlignment.Center })
`;

const steps = [];
let at = 0.3;
const add = (step, gap = 0.4) => {
	steps.push({ ...step, at });
	at += gap;
};
add({ note: "T1 CaptureFocus" }, 0.02);
add({ luau: "R.Watched.T1:CaptureFocus()" });
add({ note: "script writes" }, 0.02);
add({ luau: `local T = R.Watched.T1 T.Text = "scripted text" task.wait(0.05) T.CursorPosition = 3 task.wait(0.05) T.SelectionStart = 1 task.wait(0.05) T.CursorPosition = 100` });
add({ note: "T2 CaptureFocus steals" }, 0.02);
add({ luau: "R.Watched.T2:CaptureFocus()" });
add({ note: "T2 ReleaseFocus(true)" }, 0.02);
add({ luau: "R.Watched.T2:ReleaseFocus(true)" });
add({ note: "reset T1" }, 0.02);
add({ luau: `R.Watched.T1.Text = "hello world"` });
for (const x of [102, 112, 130, 160, 200, 250]) {
	add({ post: [act.move(1900, 1100)] }, 0.3);
	add({ post: [act.move(x, 140)] }, 0.3);
	add({ note: `click T1 at ${x}` }, 0.02);
	add({ post: [act.down(x, 140), act.up(x, 140)] }, 0.6);
}
const hoverThenClick = (note, x, y) => {
	add({ post: [act.move(x, y)] }, 0.35);
	add({ note }, 0.02);
	add({ post: [act.down(x, y), act.up(x, y)] }, 0.6);
};
hoverThenClick("click centred T3 at 400", 400, 440);
add({ post: [act.move(150, 140)] }, 0.35);
add({ note: "focus click + double click T1 at 150 (80 ms)" }, 0.02);
add({ post: [[act.down(150, 140), act.up(150, 140)], 80, [act.down(150, 140), act.up(150, 140)]] }, 0.8);
add({ note: "double click T1 at 150 (focused)" }, 0.02);
add({ post: [[act.down(150, 140), act.up(150, 140)], 80, [act.down(150, 140), act.up(150, 140)]] }, 0.8);
add({ post: [act.move(180, 140)] }, 0.35);
add({ note: "triple click T1 at 180" }, 0.02);
add({ post: [[act.down(180, 140), act.up(180, 140)], 80, [act.down(180, 140), act.up(180, 140)], 80, [act.down(180, 140), act.up(180, 140)]] }, 0.8);
add({ post: [act.move(110, 140)] }, 0.35);
add({ note: "drag select 110 -> 200" }, 0.02);
add({ post: [act.down(110, 140), act.move(150, 140, 1), act.move(200, 140, 1), act.up(200, 140)] }, 0.6);
hoverThenClick("click outside", 900, 700);
hoverThenClick("T2 click (ClearTextOnFocus)", 300, 290);
add({ note: "end" }, 0.02);
add({ luau: "local F = UIS:GetFocusedTextBox() if F then F:ReleaseFocus() end" }, 0.3);

export default { name: "textbox", viewport: [2060, 1151], setup, duration: at + 0.3, steps, props: ["Text", "CursorPosition", "SelectionStart"] };
