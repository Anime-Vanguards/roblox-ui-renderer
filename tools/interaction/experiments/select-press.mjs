// Gamepad/keyboard selection press: GuiService.SelectedObject on a CoreGui button + VirtualInput keys.
import { withStudioInput, formatLog } from "../studio.mjs";

const setup = `
local B = Instance.new("TextButton")
B.Name = "Button"
B.Text = ""
B.Position = UDim2.fromOffset(100, 100)
B.Size = UDim2.fromOffset(200, 100)
B.BackgroundColor3 = Color3.new(1, 1, 1)
B.Parent = R.Gui
R.Watch(B, "Button", { "GuiState", "Selected" })
`;

const run = await withStudioInput("select-press", setup, async api => api.luau(`
local GuiService = game:GetService("GuiService")
local B = R.Watched.Button
local Out = {}
local Ok, Error = pcall(function()
	GuiService.SelectedObject = B
end)
table.insert(Out, "set SelectedObject: " .. tostring(Ok) .. " " .. tostring(Error))
task.wait(0.1)
table.insert(Out, "SelectedObject = " .. tostring(GuiService.SelectedObject))
local Ok2, Error2 = pcall(function()
	GuiService.SelectedCoreObject = B
end)
table.insert(Out, "set SelectedCoreObject: " .. tostring(Ok2) .. " " .. tostring(Error2))
task.wait(0.1)
local Ok3, Value3 = pcall(function()
	return GuiService.SelectedCoreObject
end)
table.insert(Out, "SelectedCoreObject = " .. tostring(Ok3) .. " " .. tostring(Value3))
if B ~= nil and GuiService.SelectedObject == B then
	local Virtual = UIS:CreateVirtualInput()
	R.Note("key down Return")
	Virtual:SendKey(true, Enum.KeyCode.Return, false)
	local Ok4, Error4 = pcall(function()
		task.wait(0.3)
		table.insert(Out, "during: " .. tostring(B.GuiState))
	end)
	R.Note("key up Return")
	Virtual:SendKey(false, Enum.KeyCode.Return, false)
	task.wait(0.3)
	if not Ok4 then
		table.insert(Out, "error: " .. tostring(Error4))
	end
end
pcall(function()
	GuiService.SelectedObject = nil
end)
pcall(function()
	GuiService.SelectedCoreObject = nil
end)
return table.concat(Out, " | ")
`));
console.log(run.result);
console.log(formatLog(run.log));
console.log(JSON.stringify(run.safety.changed));
