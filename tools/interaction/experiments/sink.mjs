// Input sinking without a cover: UIS gameProcessedEvent and GUI occlusion per object kind, Active and
// InputSink. The 3D view behind the whole viewport is empty (probes/raycast-area.luau), so an unsunk
// click reaches nothing; Selection / undo are checked before and after.
import { withStudioInput, formatLog, act } from "../studio.mjs";

const setup = `
local function Make(Class, Name, X, Y, Props, Parent)
	local O = Instance.new(Class)
	O.Name = Name
	O.Position = UDim2.fromOffset(X, Y)
	O.Size = UDim2.fromOffset(120, 120)
	for Key, Value in Props or {} do
		O[Key] = Value
	end
	O.Parent = Parent or R.Gui
	R.Watch(O, Name, { "GuiState", "Active", "InputSink" })
	return O
end
Make("Frame", "Plain", 100, 100)
Make("Frame", "ActiveFrame", 300, 100, { Active = true })
Make("TextButton", "Button", 500, 100)
Make("TextButton", "InactiveButton", 700, 100, { Active = false })
Make("TextLabel", "Label", 900, 100)
Make("Frame", "SinkActivate", 1100, 100, { InputSink = Enum.InputSink.Activate })
Make("Frame", "SinkAll", 1300, 100, { InputSink = Enum.InputSink.All })
Make("ScrollingFrame", "Scroll", 1500, 100)
Make("TextButton", "B8", 100, 400)
Make("Frame", "Over8", 100, 400, { InputSink = Enum.InputSink.Activate, ZIndex = 2, Size = UDim2.fromOffset(60, 120) })
Make("TextButton", "B9", 300, 400)
Make("Frame", "Over9", 300, 400, { InputSink = Enum.InputSink.All, ZIndex = 2, Size = UDim2.fromOffset(60, 120) })
Make("TextButton", "B11", 500, 400)
Make("TextButton", "Over11", 500, 400, { Active = false, ZIndex = 2, Size = UDim2.fromOffset(60, 120) })
Make("TextButton", "B12", 700, 400)
Make("TextButton", "Over12", 700, 400, { InputSink = Enum.InputSink.None, ZIndex = 2, Size = UDim2.fromOffset(60, 120) })
Make("TextButton", "B13", 900, 400)
Make("TextBox", "Over13", 900, 400, { ZIndex = 2, Size = UDim2.fromOffset(60, 120), Text = "", ClearTextOnFocus = false })
`;

const P = [["Plain", 160, 160], ["ActiveFrame", 360, 160], ["Button", 560, 160], ["InactiveButton", 760, 160], ["Label", 960, 160], ["SinkActivate", 1160, 160], ["SinkAll", 1360, 160], ["Scroll", 1560, 160],
	["Over8", 130, 460], ["Over9", 330, 460], ["Over11", 530, 460], ["Over12", 730, 460], ["Over13", 930, 460], ["empty", 1000, 900]];

const run = await withStudioInput("sink", setup, async api => {
	const note = text => api.luau(`R.Note(${JSON.stringify(text)}) return ""`);
	for (const [name, x, y] of P) {
		await note(`hover ${name}`);
		api.post([act.move(1900, 1100), act.move(x, y), act.sleep(250)]);
		await note(`click ${name}`);
		api.post([act.move(x, y), act.down(x, y), act.up(x, y), act.sleep(250)]);
		await api.luau(`local F = UIS:GetFocusedTextBox() if F then F:ReleaseFocus() end task.wait(0.1) return ""`);
	}
	api.post([act.move(1900, 1100), act.sleep(250)]);
	return "";
});
console.log(formatLog(run.log.filter(e => !(e.p === "MouseLocation") && !(e.e && /^UIS\.Input(Began|Changed|Ended)$/.test(e.e) && e.a[0].input === "MouseMovement" && e.a[0].pos[0] > 2000))));
console.log(JSON.stringify(run.safety));
