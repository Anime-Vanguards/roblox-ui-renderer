// UIDragDetector with an Active parent over several frames (is the detector live in edit mode at all?).
// held in between), over an Active full-screen cover.
import fs from "node:fs";
import { withStudioInput, formatLog, act, COVER } from "../studio.mjs";

const setup = `${COVER}
local function Box(Name, X, Y, Props)
	local F = Instance.new("Frame")
	F.Name = Name
	F.Position = UDim2.fromOffset(X, Y)
	F.Size = UDim2.fromOffset(100, 100)
	for Key, Value in Props or {} do
		F[Key] = Value
	end
	F.Parent = R.Gui
	return F
end
local A = Box("A", 100, 100, { Active = true })
R.Watch(A, "A", { "Position" }, { "InputBegan", "InputEnded" })
local D = Instance.new("UIDragDetector")
D.Name = "DA"
D.Parent = A
R.Watch(D, "DA", { "DragUDim2" }, { "DragStart", "DragContinue", "DragEnd" })
local G = Box("G", 800, 100, { Active = true, Draggable = true })
R.Watch(G, "G", { "Position" }, { "DragBegin", "DragStopped", "InputBegan", "InputEnded" })
`;

const run = await withStudioInput("drag3", setup, async api => {
	for (const [name, [x, y]] of [["A detector (Active parent)", [150, 150]]]) {
		await api.luau(`R.Note(${JSON.stringify(name)}) return ""`);
		api.post([
			act.batch(act.move(1900, 1100)), act.sleep(150),
			act.batch(act.move(x, y), act.down(x, y)), act.sleep(120),
			act.batch(act.move(x + 10, y + 5, 1)), act.sleep(120),
			act.batch(act.move(x + 30, y + 15, 1)), act.sleep(120),
			act.batch(act.move(x + 50, y + 30, 1)), act.sleep(120),
			act.batch(act.up(x + 50, y + 30)), act.sleep(300),
		]);
	}
	return "";
});
fs.writeFileSync("out/interaction/drag2.json", JSON.stringify(run.log));
console.log(formatLog(run.log.filter(e => e.p !== "MouseLocation" && e.p !== "KeysPressed" && !(e.e && /^UIS/.test(e.e) && e.a[0].input === "MouseMovement"))));
console.log(JSON.stringify(run.safety.changed));
