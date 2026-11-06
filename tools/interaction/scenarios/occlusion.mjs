// Hover and clicks over layered objects (a button with a child frame, a label, an Active frame, an
// Interactable = false frame, an Active ImageLabel and a button above it), Active cover underneath.
import { act, COVER } from "../studio.mjs";

const setup = `${COVER}
local function Make(Class, Name, Parent, X, Y, W, H, Props)
	local O = Instance.new(Class)
	O.Name = Name
	O.Position = UDim2.fromOffset(X, Y)
	O.Size = UDim2.fromOffset(W, H)
	for Key, Value in Props or {} do
		O[Key] = Value
	end
	O.Parent = Parent
	R.Watch(O, Name, {})
	return O
end
local F = Make("Frame", "F", R.Gui, 100, 100, 1000, 400)
local B = Make("TextButton", "B", F, 50, 50, 900, 200, { Text = "" })
Make("Frame", "C", B, 0, 0, 100, 200)
Make("TextLabel", "L", F, 200, 50, 100, 200, { ZIndex = 2, Text = "L" })
Make("Frame", "A", F, 350, 50, 100, 200, { ZIndex = 2, Active = true })
Make("Frame", "N", F, 500, 50, 100, 200, { ZIndex = 2, Interactable = false })
Make("ImageLabel", "I", F, 650, 50, 100, 200, { ZIndex = 2, Active = true })
Make("TextButton", "T", F, 800, 50, 100, 200, { ZIndex = 2, Text = "T" })
`;

const points = { F: [130, 450], B: [275, 250], C: [180, 250], L: [330, 250], A: [480, 250], N: [630, 250], I: [780, 250], T: [930, 250] };
const steps = [{ at: 0.2, post: [act.move(1900, 1100)] }];
let at = 0.6;
for (const [name, [x, y]] of Object.entries(points)) {
	steps.push({ at, note: `hover ${name}` }, { at: at + 0.05, post: [act.move(x, y)] });
	at += 0.45;
}
steps.push({ at, note: "hover out" }, { at: at + 0.05, post: [act.move(1900, 1100)] });
at += 0.45;
for (const [name, [x, y]] of Object.entries(points)) {
	steps.push({ at, post: [act.move(1900, 1100)] }, { at: at + 0.3, post: [act.move(x, y)] }, { at: at + 0.7, note: `click ${name}` }, { at: at + 0.75, post: [act.down(x, y), act.up(x, y)] });
	at += 1.2;
}

export default { name: "occlusion", viewport: [2060, 1151], setup, duration: at + 0.3, steps };
