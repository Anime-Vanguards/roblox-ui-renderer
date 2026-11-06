-- TextBox focus / caret / selection. Studio reference: tools/interaction/experiments
-- caret.mjs / caret2.mjs (SourceSans 48 "hello world" box at (100, 100), 600x80).
--   node cli/rur.mjs render --fixture fixtures/interaction-textbox.luau --viewport 900x400 --time 1
--     --background 20,20,26 --actions fixtures/interaction-textbox.actions.json
return function(Target)
	local T = Instance.new("TextBox")
	T.Name = "T"
	T.Position = UDim2.fromOffset(100, 100)
	T.Size = UDim2.fromOffset(600, 80)
	T.Font = Enum.Font.SourceSans
	T.TextSize = 48
	T.TextXAlignment = Enum.TextXAlignment.Left
	T.ClearTextOnFocus = false
	T.Text = "hello world"
	T.TextColor3 = Color3.fromRGB(200, 30, 30)
	T.BackgroundColor3 = Color3.fromRGB(255, 255, 255)
	T.BorderSizePixel = 0
	T.Parent = Target

	local P = Instance.new("TextBox")
	P.Name = "P"
	P.Position = UDim2.fromOffset(100, 250)
	P.Size = UDim2.fromOffset(600, 80)
	P.Font = Enum.Font.SourceSans
	P.TextSize = 48
	P.TextXAlignment = Enum.TextXAlignment.Left
	P.ClearTextOnFocus = false
	P.Text = ""
	P.PlaceholderText = "placeholder"
	P.PlaceholderColor3 = Color3.fromRGB(30, 30, 200)
	P.TextColor3 = Color3.fromRGB(200, 30, 30)
	P.BackgroundColor3 = Color3.fromRGB(255, 255, 255)
	P.BorderSizePixel = 0
	P.Parent = Target

	local Log = Instance.new("TextLabel")
	Log.Name = "Log"
	Log.Position = UDim2.fromOffset(100, 350)
	Log.Size = UDim2.fromOffset(600, 30)
	Log.BackgroundTransparency = 1
	Log.TextColor3 = Color3.new(1, 1, 1)
	Log.TextSize = 18
	Log.Text = ""
	Log.Parent = Target
	local Events = {}
	local function Add(Text)
		table.insert(Events, Text)
		Log.Text = table.concat(Events, " ")
	end
	T.Focused:Connect(function()
		Add("focused")
	end)
	T.FocusLost:Connect(function(Enter)
		Add(`lost({Enter})`)
	end)
	T:GetPropertyChangedSignal("Text"):Connect(function()
		Add(`text={T.Text}`)
	end)
end
