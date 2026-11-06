// Mouse wheel on ScrollingFrames: CanvasPosition per notch (WHEEL_DELTA = 120), frame-size dependence,
// horizontal-only canvases, nested frames, wheel events and gameProcessed. Active cover underneath.
import { withStudioInput, formatLog, act, COVER } from "../studio.mjs";

const setup = `${COVER}
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
Scroller("S2", 400, 50, 600, 150, 600, 3000)
Scroller("S3", 1050, 50, 300, 300, 3000, 300)
Scroller("S5", 1400, 50, 300, 600, 300, 20000)
local Outer = Scroller("Outer", 50, 450, 400, 400, 400, 4000)
Scroller("Inner", 20, 20, 200, 200, 200, 400, nil, Outer)
Scroller("S6", 500, 450, 300, 300, 3000, 3000)
Scroller("S7", 850, 450, 300, 300, 300, 3000, { ScrollingEnabled = false })
`;

const run = await withStudioInput("wheel", setup, async api => {
	const note = text => api.luau(`R.Note(${JSON.stringify(text)}) return ""`);
	const wheel = async (label, x, y, delta, count = 1, horizontal = false) => {
		await note(label);
		for (let i = 0; i < count; i++) api.post([act.batch(act.move(x, y), act.wheel(x, y, delta, horizontal)), act.sleep(250)]);
	};
	await wheel("S1 down x1", 200, 200, -120);
	await wheel("S1 down x2", 200, 200, -120, 2);
	await wheel("S1 up x1", 200, 200, 120);
	await wheel("S2 down x1", 700, 125, -120);
	await wheel("S3 down x1 (horizontal canvas)", 1200, 200, -120);
	await wheel("S3 hwheel right x1", 1200, 200, 120, 1, true);
	await wheel("S5 down x1 (tall frame)", 1550, 300, -120);
	await wheel("S1 down half notch (60)", 200, 200, -60);
	await wheel("Inner down x3", 150, 550, -120, 3);
	await wheel("Inner down x2 more (at end)", 150, 550, -120, 2);
	await wheel("S6 down x1 (XY canvas)", 650, 600, -120);
	await wheel("S7 down x1 (ScrollingEnabled false)", 1000, 600, -120);
	await note("done");
	return "";
});
console.log(formatLog(run.log.filter(e => !(e.p === "MouseLocation" || e.e === "InputChanged" && e.a[0].input === "MouseMovement" || e.e === "UIS.InputChanged" && e.a[0].input === "MouseMovement" || e.e === "UIS.InputBegan" && e.a[0].input === "MouseMovement" || e.e === "UIS.InputEnded" && e.a[0].input === "MouseMovement"))));
console.log(JSON.stringify(run.safety));
