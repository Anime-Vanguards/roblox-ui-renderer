return function(Target)
	local Clip = Instance.new("Frame")
	Clip.Position = UDim2.fromOffset(100, 100)
	Clip.Size = UDim2.fromOffset(100, 100)
	Clip.ClipsDescendants = true
	Clip.Parent = Target
	local Child = Instance.new("TextButton")
	Child.Name = "RotatedHitProbe"
	Child.Position = UDim2.fromOffset(-40, 25)
	Child.Size = UDim2.fromOffset(180, 40)
	Child.Rotation = 10
	Child.Parent = Clip
	task.wait(0.1)
	local Result = {}
	for _, X in { 70, 90, 110, 150, 190, 210, 230 } do
		local Found = false
		for _, Hit in game:GetService("CoreGui"):GetGuiObjectsAtPosition(X, 145) do
			if Hit == Child then Found = true end
		end
		Result[tostring(X)] = Found
	end
	return Result
end
