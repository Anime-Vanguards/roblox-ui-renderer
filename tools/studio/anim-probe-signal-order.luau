-- Order between an instance's Changed and GetPropertyChangedSignal handlers, independent of which
-- was connected first, and between two different properties written in one go.
return function(Target)
	local Log = {}
	local function Add(Name)
		table.insert(Log, Name)
	end

	-- Property signal connected BEFORE the Changed handlers
	local First = Instance.new("NumberValue")
	First:GetPropertyChangedSignal("Value"):Connect(function()
		Add("First:Property")
	end)
	First.Changed:Connect(function()
		Add("First:ChangedA")
	end)
	First.Changed:Connect(function()
		Add("First:ChangedB")
	end)
	First.Value = 1
	task.wait()

	-- Property signal connected AFTER the Changed handlers
	local Second = Instance.new("NumberValue")
	Second.Changed:Connect(function()
		Add("Second:ChangedA")
	end)
	Second:GetPropertyChangedSignal("Value"):Connect(function()
		Add("Second:Property")
	end)
	Second.Changed:Connect(function()
		Add("Second:ChangedB")
	end)
	Second.Value = 1
	task.wait()

	-- A GuiObject: Changed (name argument) vs property signals for two properties
	local Frame = Instance.new("Frame")
	Frame.Changed:Connect(function(Name)
		Add(`Frame:Changed:{Name}`)
	end)
	Frame:GetPropertyChangedSignal("BackgroundTransparency"):Connect(function()
		Add("Frame:Property:BackgroundTransparency")
	end)
	Frame:GetPropertyChangedSignal("Rotation"):Connect(function()
		Add("Frame:Property:Rotation")
	end)
	Frame.BackgroundTransparency = 0.5
	Frame.Rotation = 10
	task.wait()

	First:Destroy()
	Second:Destroy()
	Frame:Destroy()
	return { Log = Log }
end
