// Press / drag-off / release sequences posted as tight batches (all messages queued before Qt handles
// the first): which button events fire, and in which order, when the release happens outside, after
// dragging off and back, or over another button.
import { withStudioInput, formatLog, act, COVER } from "../studio.mjs";

const setup = `${COVER}
local function Make(Name, X)
	local B = Instance.new("TextButton")
	B.Name = Name
	B.Position = UDim2.fromOffset(X, 100)
	B.Size = UDim2.fromOffset(200, 100)
	B.Parent = R.Gui
	R.Watch(B, Name, { "GuiState" })
end
Make("A", 100)
Make("B", 400)
`;

const A = [200, 150], B = [500, 150], OUT = [800, 600];
const seq = {
	"click A": [act.move(...A), act.down(...A), act.up(...A)],
	"down A, up outside": [act.move(...A), act.down(...A), act.move(...OUT, 1), act.up(...OUT)],
	"down A, out, back, up A": [act.move(...A), act.down(...A), act.move(...OUT, 1), act.move(A[0] + 10, A[1], 1), act.up(A[0] + 10, A[1])],
	"down A, up over B": [act.move(...A), act.down(...A), act.move(...B, 1), act.up(...B)],
	"down outside, up A": [act.move(...OUT), act.down(...OUT), act.move(...A, 1), act.up(...A)],
	"double click A": [act.move(...A), act.down(...A), act.up(...A), act.down(...A), act.up(...A)],
	"right click A": [act.move(...A), act.down(A[0], A[1], 2), act.up(A[0], A[1], 2)],
};

const run = await withStudioInput("drag-batch", setup, async api => {
	const out = [];
	for (const [name, items] of Object.entries(seq)) {
		await api.luau(`R.Note(${JSON.stringify(name)}) return ""`);
		out.push(api.post([act.batch(act.move(1900, 1100)), act.sleep(150), act.batch(...items), act.sleep(350)]));
	}
	return out;
});
console.log(formatLog(run.log.filter(e => !(e.p === "MouseLocation") && !(e.a && e.a[0] && e.a[0].input === "MouseMovement" && e.a[0].pos[0] > 1800))));
console.log(JSON.stringify(run.safety.changed), run.result[0]);
