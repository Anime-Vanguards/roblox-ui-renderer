// First click test: posted WM_LBUTTONDOWN/UP over an Active TextButton, with a full-screen Active
// cover frame; the 3D view behind the whole viewport is empty (probes/raycast-area.luau).
import { withStudioInput, formatLog, act, COVER } from "../studio.mjs";

const setup = `${COVER}
R.Watch(Cover, "Cover", {}, { "InputBegan", "InputEnded" })
local Button = Instance.new("TextButton")
Button.Name = "Button"
Button.Position = UDim2.fromOffset(100, 100)
Button.Size = UDim2.fromOffset(200, 100)
Button.Parent = R.Gui
R.Watch(Button, "Button", { "GuiState", "BackgroundColor3" })
`;

const run = await withStudioInput("click-post", setup, async api => {
	const out = [];
	out.push(api.post([act.move(900, 600), act.sleep(200), act.move(150, 150), act.sleep(300), act.down(150, 150), act.sleep(300), act.up(150, 150), act.sleep(300)]));
	out.push(await api.luau(`R.Note("-- after click") return HttpService:JSONEncode(R.Safety())`));
	return out;
});
console.log(run.result.join("\n"));
console.log(formatLog(run.log));
console.log(JSON.stringify(run.safety));
