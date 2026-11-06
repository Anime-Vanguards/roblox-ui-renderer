-- Engine probe: error messages scripts see (pcall results), member access and typeof.
return function(Target)
	local function Try(Label, Callback, ...)
		local Results = table.pack(pcall(Callback, ...))
		local Parts = {}
		for Index = 1, Results.n do
			table.insert(Parts, tostring(Results[Index]))
		end
		print(Label, table.concat(Parts, " | "))
	end
	local F = Instance.new("Frame")
	Try("new-bad", Instance.new, "NotAClass")
	Try("new-bad-wrapped", function() return Instance.new("NotAClass") end)
	Try("index-missing", function() return F.Nope end)
	Try("newindex-missing", function() F.Nope = 1 end)
	Try("newindex-type", function() F.Size = 5 end)
	Try("newindex-enum", function() F.BorderMode = "Nope" end)
	Try("newindex-enum-wrong", function() F.BorderMode = Enum.Font.Arial end)
	Try("newindex-readonly", function() F.AbsoluteSize = Vector2.new() end)
	Try("newindex-number", function() F.BackgroundTransparency = "x" end)
	Try("newindex-bool", function() F.Visible = 5 end)
	Try("parent-bad", function() F.Parent = 5 end)
	Try("circular", function() F.Parent = F end)
	Try("callback-get", function() return Instance.new("BindableFunction").OnInvoke end)
	Try("propsignal-bad", function() return F:GetPropertyChangedSignal("Nope") end)
	Try("attr-bad", function() F:SetAttribute("A", {}) end)
	Try("connect-bad", function() F.Changed:Connect(5) end)
	Try("signal-member", function() return F.Changed.Nope end)
	Try("enum-missing", function() return Enum.Font.Nope end)
	Try("enum-type-missing", function() return Enum.Nope end)
	Try("getservice-bad", function() return game:GetService("Nope") end)
	Try("require-bad", function() return require(5) end)
	Try("udim2-bad", function() return UDim2.new(UDim.new(), 5) end)
	Try("index-number", function() return F[1] end)
	print("typeof", typeof(F), typeof(F.Size), typeof(Enum.Font), typeof(Enum.Font.Arial), typeof(Enum), typeof(F.Changed), typeof(vector.create(1,2,3)), typeof(Font.new("x")))
	print("tostring", tostring(F), tostring(Enum.Font.Arial), tostring(Enum.Font), tostring(Enum), tostring(F.Changed), tostring(F.Size), tostring(Color3.new(1, 0.5, 0)))
	print("meta", getmetatable(F), getmetatable(Enum.Font.Arial), getmetatable(F.Size))
	print("float", F.BackgroundTransparency, (function() F.BackgroundTransparency = 0.1 return F.BackgroundTransparency end)())
	print("defaults", F.Name, F.ClassName, F.Size, F.BackgroundColor3, F.BorderMode, F.Visible, F.ZIndex, F.LayoutOrder, F.Parent)
	print("fullname", F:GetFullName(), Target:GetFullName(), game:GetFullName(), workspace:GetFullName())
	local C = Instance.new("TextLabel")
	C.TextSize = 10.7
	print("textsize", C.TextSize, (function() C.TextSize = 150 return C.TextSize end)())
	C.Name = 12
	print("name-number", C.Name, typeof(C.Name))
	C.Parent = F
	F.Parent = Target
	print("find", F:FindFirstChild("12"), F:FindFirstChildOfClass("TextLabel"), F:FindFirstChildWhichIsA("GuiObject"), C:FindFirstAncestorOfClass("Frame"), C:IsDescendantOf(Target), C:IsA("GuiObject"), C:IsA("Frame"))
	print("children", #F:GetChildren(), #Target:GetDescendants())
end
