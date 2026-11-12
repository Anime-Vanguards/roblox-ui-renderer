-- One string whose fifth glyph's pen is exactly halfway between two quarter pixels (device scale 1.5),
-- left aligned at many device-pixel origins, and right aligned and centred in boxes of every width from
-- 60 to 139: which quarter Studio picks.
-- compare: --viewport 1500x660 --time 0.2 --background 0,0,0
return function(Target)
	local Face = Font.new("rbxasset://fonts/families/SourceSansPro.json")
	local function Add(Name, Index, Row, Alignment, Width, Shift)
		local Label = Instance.new("TextLabel")
		Label.Name = Name
		Label.BackgroundTransparency = 1
		Label.TextColor3 = Color3.new(1, 1, 1)
		Label.FontFace = Face
		Label.TextSize = 20
		Label.Text = "aaaa bbbb"
		Label.TextXAlignment = Alignment
		Label.Size = UDim2.fromOffset(Width, 28)
		Label.Position = UDim2.fromOffset(10 + (Index % 10) * 148 + Shift, 4 + Row * 32)
		Label.Parent = Target
	end
	for Index = 0, 39 do
		Add(`Left{Index}`, Index, Index // 10, Enum.TextXAlignment.Left, 84, Index / 1.5)
	end
	for Index = 0, 79 do
		Add(`Right{60 + Index}`, Index, 4 + Index // 10, Enum.TextXAlignment.Right, 60 + Index, 0)
		Add(`Center{60 + Index}`, Index, 12 + Index // 10, Enum.TextXAlignment.Center, 60 + Index, 0)
	end
end
