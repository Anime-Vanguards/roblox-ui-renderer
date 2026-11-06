local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Probe"
Gui.Parent = CoreGui
local Out = {}
local function Case(Name, Configure)
	local Holder = Instance.new("Frame")
	Holder.Size = UDim2.fromOffset(600, 200)
	Holder.Parent = Gui
	local Child = Instance.new("Frame")
	Child.Size = UDim2.fromOffset(180, 42)
	Child.Parent = Holder
	local Scale = Instance.new("UIScale")
	Scale.Scale = 0.885
	Scale.Parent = Child
	local Other = Instance.new("Frame")
	Other.Size = UDim2.fromOffset(50, 20)
	Other.Parent = Holder
	Configure(Holder, Child)
	task.wait()
	table.insert(Out, { name = Name, child = { Child.AbsolutePosition.X, Child.AbsolutePosition.Y, Child.AbsoluteSize.X, Child.AbsoluteSize.Y }, other = { Other.AbsolutePosition.X - Holder.AbsolutePosition.X, Other.AbsolutePosition.Y - Holder.AbsolutePosition.Y }, holder = { Holder.AbsoluteSize.X, Holder.AbsoluteSize.Y } })
	Holder:Destroy()
end
Case("plain", function() end)
Case("anchor-center", function(_, Child)
	Child.AnchorPoint = Vector2.new(0.5, 0.5)
	Child.Position = UDim2.fromOffset(100, 100)
end)
for _, Wraps in { false, true } do
	for _, VA in { "Top", "Center" } do
		for _, Dir in { "Horizontal", "Vertical" } do
			Case(`list {Dir} wraps={Wraps} va={VA}`, function(Holder)
				local List = Instance.new("UIListLayout")
				List.FillDirection = Enum.FillDirection[Dir]
				List.Wraps = Wraps
				List.VerticalAlignment = Enum.VerticalAlignment[VA]
				List.Padding = UDim.new(0, 12)
				List.Parent = Holder
			end)
		end
	end
end
Case("list horizontal auto-y holder", function(Holder)
	Holder.Size = UDim2.fromOffset(600, 0)
	Holder.AutomaticSize = Enum.AutomaticSize.Y
	local List = Instance.new("UIListLayout")
	List.FillDirection = Enum.FillDirection.Horizontal
	List.Wraps = true
	List.VerticalAlignment = Enum.VerticalAlignment.Center
	List.Padding = UDim.new(0, 12)
	List.Parent = Holder
end)
Gui:Destroy()
return HttpService:JSONEncode(Out)
