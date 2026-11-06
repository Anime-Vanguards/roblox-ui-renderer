// VirtualInput (UserInputService:CreateVirtualInput) on a TextButton with Active = false (the API
// refuses positions over Active CoreGui objects). The 3D view behind the viewport is empty.
import { withStudioInput, formatLog } from "../studio.mjs";

const setup = `
local Button = Instance.new("TextButton")
Button.Name = "Button"
Button.Active = false
Button.Position = UDim2.fromOffset(100, 100)
Button.Size = UDim2.fromOffset(200, 100)
Button.Parent = R.Gui
R.Watch(Button, "Button", { "GuiState", "BackgroundColor3", "Active", "InputSink" })
`;

const steps = `
local Virtual = UIS:CreateVirtualInput()
local function Step(Label, Callback)
	R.Note(Label)
	local Ok, Error = pcall(Callback)
	if not Ok then
		R.Note("ERROR " .. tostring(Error))
	end
	task.wait(0.25)
end
Step("move 900,600", function() Virtual:SendMousePosition(Vector2.new(900, 600)) end)
Step("move 150,150", function() Virtual:SendMousePosition(Vector2.new(150, 150)) end)
Step("move 160,155", function() Virtual:SendMousePosition(Vector2.new(160, 155)) end)
Step("down 160,155", function() Virtual:SendMouseButton(Vector2.new(160, 155), Enum.UserInputType.MouseButton1, true, 1) end)
Step("up 160,155", function() Virtual:SendMouseButton(Vector2.new(160, 155), Enum.UserInputType.MouseButton1, false, 1) end)
Step("move 900,600", function() Virtual:SendMousePosition(Vector2.new(900, 600)) end)
return "done"
`;

const run = await withStudioInput("virtual-button", setup, async api => api.luau(steps));
console.log(run.result);
console.log(formatLog(run.log));
console.log(JSON.stringify(run.safety.changed), run.safety.after.selection);
