// Hit-testing / sinking: which objects under the cursor get hover and mouse events, and what blocks a
// button? Button B inside frame F; over B: its own child C (Frame), a sibling TextLabel L (higher
// ZIndex, Active false), a sibling Frame A (higher ZIndex, Active true), a sibling Frame N (higher
// ZIndex, Active false, Interactable false), a sibling ImageLabel I (Active true, Visible... true).
// Posted moves one per burst; clicks as [move, down, up] bursts. Full-screen Active cover underneath.
import { withStudioInput, formatLog, act, COVER } from "../studio.mjs";

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
	R.Watch(O, Name, { "GuiState" })
	return O
end
local F = Make("Frame", "F", R.Gui, 100, 100, 1000, 400, { BackgroundColor3 = Color3.fromRGB(40, 40, 60) })
local B = Make("TextButton", "B", F, 50, 50, 900, 200, { Text = "" })
local C = Make("Frame", "C", B, 0, 0, 100, 200, { BackgroundColor3 = Color3.fromRGB(200, 0, 0) })
local L = Make("TextLabel", "L", F, 200, 50, 100, 200, { ZIndex = 2, Text = "L" })
local A = Make("Frame", "A", F, 350, 50, 100, 200, { ZIndex = 2, Active = true, BackgroundColor3 = Color3.fromRGB(0, 200, 0) })
local N = Make("Frame", "N", F, 500, 50, 100, 200, { ZIndex = 2, Interactable = false, BackgroundColor3 = Color3.fromRGB(0, 0, 200) })
local I = Make("ImageLabel", "I", F, 650, 50, 100, 200, { ZIndex = 2, Active = true, BackgroundColor3 = Color3.fromRGB(200, 200, 0) })
local T = Make("TextButton", "T", F, 800, 50, 100, 200, { ZIndex = 2, Text = "T" })
`;

// absolute points: F only, B only, B+C, B+L, B+A, B+N, B+I, B+T
const P = { F: [130, 450], B: [300, 250], C: [180, 250], L: [330, 250], A: [480, 250], N: [630, 250], I: [780, 250], T: [930, 250] };

const run = await withStudioInput("occlusion", setup, async api => {
	const note = text => api.luau(`R.Note(${JSON.stringify(text)}) return ""`);
	api.post([act.move(1900, 1100), act.sleep(300)]);
	for (const [name, [x, y]] of Object.entries(P)) {
		await note(`hover ${name}`);
		api.post([act.move(x, y), act.sleep(300)]);
	}
	await note("hover out");
	api.post([act.move(1900, 1100), act.sleep(300)]);
	for (const [name, [x, y]] of Object.entries(P)) {
		await note(`click ${name}`);
		api.post([act.move(1900, 1100), act.move(x, y), act.down(x, y), act.up(x, y), act.sleep(300)]);
	}
	api.post([act.move(1900, 1100), act.sleep(300)]);
	return "";
});
console.log(formatLog(run.log.filter(e => !(e.p === "MouseLocation"))));
console.log(JSON.stringify(run.safety.changed));
