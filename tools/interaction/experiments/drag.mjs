// UIDragDetector (and legacy Draggable) with posted drags in single batches (press, moves, release all
// before Qt's leave), over an Active full-screen cover.
import fs from "node:fs";
import { withStudioInput, formatLog, act, COVER } from "../studio.mjs";

const setup = `${COVER}
local function Box(Name, X, Y, Parent, Props)
	local F = Instance.new("Frame")
	F.Name = Name
	F.Position = UDim2.fromOffset(X, Y)
	F.Size = UDim2.fromOffset(100, 100)
	for Key, Value in Props or {} do
		F[Key] = Value
	end
	F.Parent = Parent or R.Gui
	return F
end
local function Detector(Parent, Name, Props)
	local D = Instance.new("UIDragDetector")
	D.Name = Name
	for Key, Value in Props or {} do
		D[Key] = Value
	end
	D.Parent = Parent
	R.Watch(D, Name, { "DragUDim2", "DragRotation" }, { "DragStart", "DragContinue", "DragEnd" })
	return D
end
local A = Box("A", 100, 100)
R.Watch(A, "A", { "Position", "GuiState" }, { "InputBegan", "InputEnded", "InputChanged" })
Detector(A, "DA")
local Holder = Box("Holder", 400, 100, nil, { Size = UDim2.fromOffset(300, 300), BackgroundTransparency = 0.5 })
local Scale = Instance.new("UIScale")
Scale.Scale = 2
Scale.Parent = Holder
local B = Box("B", 10, 10, Holder)
R.Watch(B, "B", { "Position" }, {})
Detector(B, "DB")
local Bound = Box("Bound", 800, 100, nil, { Size = UDim2.fromOffset(300, 300), BackgroundTransparency = 0.5 })
local C = Box("C", 10, 10, Bound)
R.Watch(C, "C", { "Position" }, {})
Detector(C, "DC", { BoundingUI = Bound })
local D = Box("D", 100, 500)
R.Watch(D, "D", { "Position" }, {})
Detector(D, "DD", { ResponseStyle = Enum.UIDragDetectorResponseStyle.Scale })
local E = Box("E", 400, 500)
R.Watch(E, "E", { "Position" }, {})
Detector(E, "DE", { DragStyle = Enum.UIDragDetectorDragStyle.TranslateLine, DragAxis = Vector2.new(1, 0) })
local G = Box("G", 800, 500, nil, { Active = true, Draggable = true })
R.Watch(G, "G", { "Position" }, { "DragBegin", "DragStopped" })
local H = Box("H", 1200, 100)
R.Watch(H, "H", { "Position" }, {})
local HB = Instance.new("TextButton")
HB.Name = "HB"
HB.Size = UDim2.fromOffset(50, 50)
HB.Parent = H
R.Watch(HB, "HB", {}, {})
Detector(H, "DH")
`;

const drags = [
	["A plain +50,+30 (two moves)", [150, 150], [[170, 160], [200, 180]]],
	["B under UIScale 2 +50,+30", [440, 140], [[490, 170]]],
	["C bounded by parent, +400,+0", [850, 150], [[1250, 150]]],
	["D ResponseStyle Scale +50,+30", [150, 550], [[200, 580]]],
	["E TranslateLine X +50,+30", [450, 550], [[500, 580]]],
	["G legacy Draggable +50,+30", [850, 550], [[900, 580]]],
	["H press on child button +50,+30", [1215, 115], [[1265, 145]]],
	["H press on frame part +50,+30", [1280, 180], [[1330, 210]]],
];

const run = await withStudioInput("drag", setup, async api => {
	for (const [name, start, moves] of drags) {
		await api.luau(`R.Note(${JSON.stringify(name)}) return ""`);
		const items = [act.move(...start), act.down(...start)];
		for (const m of moves) items.push(act.move(m[0], m[1], 1));
		const last = moves[moves.length - 1];
		items.push(act.up(last[0], last[1]));
		api.post([act.batch(act.move(1900, 1100)), act.sleep(150), act.batch(...items), act.sleep(400)]);
	}
	return "";
});
fs.writeFileSync("out/interaction/drag.json", JSON.stringify(run.log));
console.log(formatLog(run.log.filter(e => e.p !== "MouseLocation" && !(e.a && e.a[0] && e.a[0].input === "MouseMovement") && !(e.e && /^UIS/.test(e.e)))));
console.log(JSON.stringify(run.safety.changed));
