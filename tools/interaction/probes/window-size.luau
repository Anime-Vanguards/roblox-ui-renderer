-- AbsoluteWindowSize / max CanvasPosition per scroll bar configuration (temporary CoreGui GUI)
local CoreGui = game:GetService("CoreGui")
local HttpService = game:GetService("HttpService")
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Input"
Gui.IgnoreGuiInset = true
local Cases = {
	{ "both12", 200, 200, 200, 400, 12 },
	{ "vertOnly12", 200, 200, 100, 400, 12 },
	{ "horzOnly12", 200, 200, 400, 100, 12 },
	{ "both6", 200, 200, 200, 400, 6 },
	{ "vertOnly6", 200, 200, 100, 400, 6 },
	{ "both20", 200, 200, 200, 400, 20 },
	{ "vertOnly20", 200, 200, 100, 400, 20 },
	{ "both0", 200, 200, 200, 400, 0 },
	{ "vertOnly0", 200, 200, 100, 400, 0 },
	{ "vertOnly12odd", 201, 151, 100, 400, 12 },
	{ "both12big", 300, 300, 3000, 3000, 12 },
	{ "fit12", 200, 200, 188, 400, 12 },
	{ "fit12b", 200, 200, 187, 400, 12 },
	{ "fit12c", 200, 200, 189, 400, 12 },
}
local Frames = {}
for Index, Case in Cases do
	local S = Instance.new("ScrollingFrame")
	S.Name = Case[1]
	S.Position = UDim2.fromOffset(((Index - 1) % 6) * 320, math.floor((Index - 1) / 6) * 320)
	S.Size = UDim2.fromOffset(Case[2], Case[3])
	S.CanvasSize = UDim2.fromOffset(Case[4], Case[5])
	S.ScrollBarThickness = Case[6]
	S.Parent = Gui
	Frames[Case[1]] = S
end
Gui.Parent = CoreGui
task.wait()
for _, S in Frames do
	S.CanvasPosition = Vector2.new(5000, 5000)
end
task.wait(0.1)
local Out = {}
for Name, S in Frames do
	Out[Name] = { window = { S.AbsoluteWindowSize.X, S.AbsoluteWindowSize.Y }, canvas = { S.AbsoluteCanvasSize.X, S.AbsoluteCanvasSize.Y }, max = { S.CanvasPosition.X, S.CanvasPosition.Y } }
end
Gui:Destroy()
return HttpService:JSONEncode(Out)
