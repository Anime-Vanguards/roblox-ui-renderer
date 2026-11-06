-- How TweenService interpolates each property type. Every target is tweened with the same TweenInfo
-- as a reference NumberValue (0 -> 1), whose value at each render frame is the eased alpha that
-- frame used. Types whose tween cannot be created are reported with the error.
return function(Target)
	local RunService = game:GetService("RunService")
	local TweenService = game:GetService("TweenService")

	local Info = TweenInfo.new(0.6, Enum.EasingStyle.Linear)

	local function Fmt(Value)
		local Kind = typeof(Value)
		if Kind == "number" then
			return string.format("%.17g", Value)
		elseif Kind == "boolean" or Kind == "string" then
			return tostring(Value)
		elseif Kind == "Color3" then
			return string.format("%.17g %.17g %.17g", Value.R, Value.G, Value.B)
		elseif Kind == "Vector2" then
			return string.format("%.17g %.17g", Value.X, Value.Y)
		elseif Kind == "Vector3" then
			return string.format("%.17g %.17g %.17g", Value.X, Value.Y, Value.Z)
		elseif Kind == "UDim" then
			return string.format("%.17g %d", Value.Scale, Value.Offset)
		elseif Kind == "UDim2" then
			return string.format("%.17g %d %.17g %d", Value.X.Scale, Value.X.Offset, Value.Y.Scale, Value.Y.Offset)
		elseif Kind == "Rect" then
			return string.format("%.17g %.17g %.17g %.17g", Value.Min.X, Value.Min.Y, Value.Max.X, Value.Max.Y)
		elseif Kind == "CFrame" then
			local Parts = { Value:GetComponents() }
			for Index, Part in Parts do
				Parts[Index] = string.format("%.17g", Part)
			end
			return table.concat(Parts, " ")
		elseif Kind == "NumberRange" then
			return string.format("%.17g %.17g", Value.Min, Value.Max)
		elseif Kind == "Vector2int16" or Kind == "Vector3int16" then
			return tostring(Value)
		end
		return `{Kind}:{tostring(Value)}`
	end

	local Cases = {}
	local Errors = {}
	local Tweens = {}

	local function Case(Name, Instance_, Property, From, To)
		local Ok, Error = pcall(function()
			Instance_[Property] = From
			local Tween = TweenService:Create(Instance_, Info, { [Property] = To })
			table.insert(Tweens, Tween)
		end)
		if not Ok then
			Errors[Name] = tostring(Error)
			return
		end
		table.insert(Cases, { Name = Name, Instance = Instance_, Property = Property, From = Fmt(From), To = Fmt(To) })
	end

	local function New(ClassName, Parent)
		local Object = Instance.new(ClassName)
		Object.Parent = Parent
		return Object
	end

	local Frame = New("Frame", Target)
	local Image = New("ImageLabel", Target)
	local Text = New("TextLabel", Target)
	local Scroll = New("ScrollingFrame", Target)
	local Corner = New("UICorner", Frame)
	local Stroke = New("UIStroke", Frame)
	local Gradient = New("UIGradient", Frame)
	local List = New("UIListLayout", Scroll)
	local Aspect = New("UIAspectRatioConstraint", Image)
	local SizeConstraint = New("UISizeConstraint", Text)

	Case("Number", New("NumberValue"), "Value", 0, 1)
	Case("NumberBig", New("NumberValue"), "Value", -1234.5678, 98765.4321)
	Case("Int", New("IntValue"), "Value", 0, 7)
	Case("IntNeg", New("IntValue"), "Value", 3, -4)
	Case("Bool", New("BoolValue"), "Value", false, true)
	Case("BoolDown", New("BoolValue"), "Value", true, false)
	Case("Color3Value", New("Color3Value"), "Value", Color3.new(0.1, 0.8, 0.3), Color3.new(0.9, 0.2, 0.7))
	Case("Vector3Value", New("Vector3Value"), "Value", Vector3.new(1.5, -2.25, 3), Vector3.new(-40.125, 17, 0.001))
	Case("CFrameValue", New("CFrameValue"), "Value", CFrame.new(1, 2, 3) * CFrame.Angles(0.3, -0.2, 0.5), CFrame.new(-4, 5, 10) * CFrame.Angles(-1.2, 2.0, 0.4))
	Case("CFrameFlip", New("CFrameValue"), "Value", CFrame.new(), CFrame.Angles(0, math.pi * 0.999, 0))
	Case("BrickColorValue", New("BrickColorValue"), "Value", BrickColor.new("Bright red"), BrickColor.new("Bright blue"))
	Case("StringValue", New("StringValue"), "Value", "a", "b")

	Case("Transparency", Frame, "BackgroundTransparency", 0.1, 0.9)
	Case("Rotation", Frame, "Rotation", -30, 395)
	Case("BackgroundColor3", Frame, "BackgroundColor3", Color3.fromRGB(10, 200, 60), Color3.fromRGB(250, 20, 160))
	Case("Size", Frame, "Size", UDim2.new(0.1, 10, 0.2, -5), UDim2.new(0.7, 301, 0.9, 77))
	Case("Position", Frame, "Position", UDim2.new(0, -3, 1, 4), UDim2.new(0.5, 4, 0, -5))
	Case("AnchorPoint", Frame, "AnchorPoint", Vector2.new(0, 0.25), Vector2.new(1, 0.75))
	Case("ZIndex", Frame, "ZIndex", 1, 10)
	Case("LayoutOrder", Frame, "LayoutOrder", 5, -5)
	Case("BorderSizePixel", Frame, "BorderSizePixel", 0, 9)
	Case("Visible", Frame, "Visible", true, false)
	Case("SizeConstraintEnum", Frame, "SizeConstraint", Enum.SizeConstraint.RelativeXY, Enum.SizeConstraint.RelativeYY)
	Case("CornerRadius", Corner, "CornerRadius", UDim.new(0, 2), UDim.new(0.5, 13))
	Case("StrokeThickness", Stroke, "Thickness", 1, 6.5)
	Case("StrokeColor", Stroke, "Color", Color3.new(0, 0, 0), Color3.new(1, 0.5, 0.25))
	Case("GradientOffset", Gradient, "Offset", Vector2.new(-1, 0), Vector2.new(1, 0.5))
	Case("GradientRotation", Gradient, "Rotation", 0, 270)
	Case("GradientColor", Gradient, "Color", ColorSequence.new(Color3.new(1, 0, 0)), ColorSequence.new(Color3.new(0, 0, 1)))
	Case("GradientTransparency", Gradient, "Transparency", NumberSequence.new(0), NumberSequence.new(1))
	Case("ImageColor3", Image, "ImageColor3", Color3.new(1, 1, 1), Color3.new(0, 0.5, 1))
	Case("ImageRectOffset", Image, "ImageRectOffset", Vector2.new(0, 0), Vector2.new(128.5, 64))
	Case("SliceCenter", Image, "SliceCenter", Rect.new(0, 0, 10, 10), Rect.new(5, 6, 50, 70))
	Case("SliceScale", Image, "SliceScale", 1, 3)
	Case("AspectRatio", Aspect, "AspectRatio", 1, 2)
	Case("TextSize", Text, "TextSize", 8, 40)
	Case("TextColor3", Text, "TextColor3", Color3.new(0, 0, 0), Color3.new(1, 1, 1))
	Case("TextStrokeTransparency", Text, "TextStrokeTransparency", 1, 0)
	Case("MaxSize", SizeConstraint, "MaxSize", Vector2.new(10, 10), Vector2.new(400, 300))
	Case("CanvasPosition", Scroll, "CanvasPosition", Vector2.new(0, 0), Vector2.new(0, 500))
	Case("CanvasSize", Scroll, "CanvasSize", UDim2.new(0, 0, 0, 0), UDim2.new(0, 0, 3, 17))
	Case("ScrollBarThickness", Scroll, "ScrollBarThickness", 2, 20)
	Case("ListPadding", List, "Padding", UDim.new(0, 0), UDim.new(0.1, 25))
	Case("NumberRange", New("ParticleEmitter"), "Lifetime", NumberRange.new(1, 2), NumberRange.new(3, 10))

	-- Reference: the eased alpha of each frame (same TweenInfo, 0 -> 1)
	local Reference = New("NumberValue")
	table.insert(Tweens, 1, TweenService:Create(Reference, Info, { Value = 1 }))

	local Rows = {}
	local Recording = true
	local Connection = RunService.PreRender:Connect(function(Dt)
		if not Recording then
			return
		end
		local Row = { string.format("%.17g", Dt), string.format("%.17g", Reference.Value) }
		for _, Entry in Cases do
			table.insert(Row, Fmt(Entry.Instance[Entry.Property]))
		end
		table.insert(Rows, Row)
	end)

	for _, Tween in Tweens do
		Tween:Play()
	end
	task.wait(0.8)
	Recording = false
	Connection:Disconnect()
	for _, Tween in Tweens do
		Tween:Cancel()
	end

	local Names = {}
	local Meta = {}
	for _, Entry in Cases do
		table.insert(Names, Entry.Name)
		Meta[Entry.Name] = { From = Entry.From, To = Entry.To }
	end
	return { Names = Names, Meta = Meta, Errors = Errors, Rows = Rows }
end
