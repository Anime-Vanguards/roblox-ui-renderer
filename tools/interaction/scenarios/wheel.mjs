// Mouse wheel through VirtualInput (Studio: over non-Active ScrollingFrames only, a full-screen Catcher
// takes unused wheel so the camera never zooms): frame at the end passing the wheel on, a plain frame
// over the catcher, a child of a ScrollingFrame, a ScrollingEnabled = false frame, direction X.
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
	R.Watch(O, Name, if Class == "ScrollingFrame" then { "CanvasPosition" } else {}, { "InputBegan", "InputEnded", "InputChanged", "MouseWheelForward", "MouseWheelBackward" })
	return O
end
Make("ScrollingFrame", "Catcher", R.Gui, 0, 0, 2060, 1150, { CanvasSize = UDim2.fromOffset(2060, 40000), ZIndex = -10 })
local Outer = Make("ScrollingFrame", "Outer", R.Gui, 50, 50, 400, 400, { CanvasSize = UDim2.fromOffset(400, 4000) })
Make("ScrollingFrame", "Inner", Outer, 20, 20, 200, 200, { CanvasSize = UDim2.fromOffset(200, 400) })
Make("Frame", "Plain", R.Gui, 500, 50, 200, 200)
local S = Make("ScrollingFrame", "S", R.Gui, 750, 50, 300, 300, { CanvasSize = UDim2.fromOffset(300, 3000) })
Make("Frame", "Child", S, 10, 10, 200, 100)
Make("ScrollingFrame", "Off", R.Gui, 1100, 50, 300, 300, { CanvasSize = UDim2.fromOffset(300, 3000), ScrollingEnabled = false })
Make("ScrollingFrame", "DirX", R.Gui, 1450, 50, 300, 300, { CanvasSize = UDim2.fromOffset(300, 3000), ScrollingDirection = Enum.ScrollingDirection.X })
R.V = UIS:CreateVirtualInput()
`;

const wheel = (x, y, n = -1) => `R.V:SendMousePosition(Vector2.new(${x}, ${y})) task.wait(0.05) R.V:SendPointerAction(Vector2.new(${x}, ${y}), { Wheel = ${n} })`;
const steps = [];
let at = 0.3;
for (const [note, code] of [
	["Inner 1", wheel(150, 150)],
	["Inner 2", wheel(150, 150)],
	["Inner 3 (reaches its end)", wheel(150, 150)],
	["Inner 4 (at end -> Outer)", wheel(150, 150)],
	["Plain over Catcher", wheel(600, 150)],
	["S.Child", wheel(800, 100)],
	["S.Child again", wheel(800, 100)],
	["Off (ScrollingEnabled false)", wheel(1200, 150)],
	["DirX", wheel(1550, 150)],
	["Wheel -2 on S", wheel(900, 300, -2)],
	["Wheel +1 on S", wheel(900, 300, 1)],
]) {
	steps.push({ at, note }, { at: at + 0.02, luau: code });
	at += 0.6;
}
steps.push({ at, luau: `R.V:SendMousePosition(Vector2.new(1900, 1100))` });

export default { name: "wheel", viewport: [2060, 1151], setup, duration: at + 0.5, steps, props: ["CanvasPosition"] };
