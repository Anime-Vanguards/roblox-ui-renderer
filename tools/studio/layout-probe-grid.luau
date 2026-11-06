-- UIGridLayout: CellSize/CellPadding scale+offset, FillDirection, FillDirectionMaxCells,
-- StartCorner, alignments, overflow, padding, invisible items, AbsoluteContentSize/CellSize/CellCount.
return function(Target)
	local Results = {}
	local Built = {}
	local Count = 0
	local function Case(Name, Props, Items, Holder)
		Count += 1
		local H = Instance.new("Frame")
		H.Name = Name
		H.Position = UDim2.fromOffset(((Count - 1) % 6) * 195, ((Count - 1) // 6) * 150)
		H.Size = Holder or UDim2.fromOffset(185, 140)
		H.BackgroundTransparency = 1
		H.Parent = Target
		local G = Instance.new("UIGridLayout")
		G.SortOrder = Enum.SortOrder.LayoutOrder
		for Key, Value in Props do
			(G :: any)[Key] = Value
		end
		G.Parent = H
		local Frames = {}
		for Index = 1, Items or 5 do
			local F = Instance.new("Frame")
			F.Name = `I{Index}`
			F.LayoutOrder = Index
			F.Parent = H
			table.insert(Frames, F)
		end
		table.insert(Built, { Name = Name, Holder = H, Grid = G, Frames = Frames })
		return H, G, Frames
	end
	local Cell = UDim2.fromOffset(40, 30)
	Case("Default", { CellSize = Cell })
	Case("Padding", { CellSize = Cell, CellPadding = UDim2.fromOffset(7, 3) })
	Case("ScaleCell", { CellSize = UDim2.new(0.25, 2, 0.2, 1), CellPadding = UDim2.new(0.02, 1, 0.03, 0) })
	Case("Vertical", { CellSize = Cell, FillDirection = Enum.FillDirection.Vertical, CellPadding = UDim2.fromOffset(4, 5) }, 7)
	Case("MaxCells2", { CellSize = Cell, FillDirectionMaxCells = 2 })
	Case("MaxCells2V", { CellSize = Cell, FillDirection = Enum.FillDirection.Vertical, FillDirectionMaxCells = 2 })
	for _, Corner in { "TopLeft", "TopRight", "BottomLeft", "BottomRight" } do
		Case(`Corner{Corner}`, { CellSize = Cell, StartCorner = Enum.StartCorner[Corner], CellPadding = UDim2.fromOffset(3, 3) })
		Case(`CornerV{Corner}`, { CellSize = Cell, StartCorner = Enum.StartCorner[Corner], FillDirection = Enum.FillDirection.Vertical, CellPadding = UDim2.fromOffset(3, 3) })
	end
	for _, HA in { "Left", "Center", "Right" } do
		for _, VA in { "Top", "Center", "Bottom" } do
			Case(`Align{HA}{VA}`, { CellSize = Cell, HorizontalAlignment = Enum.HorizontalAlignment[HA], VerticalAlignment = Enum.VerticalAlignment[VA], CellPadding = UDim2.fromOffset(5, 5) })
		end
	end
	Case("AlignCornerCombo", { CellSize = Cell, HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Bottom, StartCorner = Enum.StartCorner.BottomRight, CellPadding = UDim2.fromOffset(2, 2) })
	Case("Overflow", { CellSize = UDim2.fromOffset(70, 50), CellPadding = UDim2.fromOffset(5, 5) }, 9)
	Case("TooWide", { CellSize = UDim2.fromOffset(200, 30) }, 3)
	Case("NegativePad", { CellSize = Cell, CellPadding = UDim2.fromOffset(-5, -4) }, 6)
	do
		local H = Case("Padded", { CellSize = Cell, CellPadding = UDim2.fromOffset(2, 2), HorizontalAlignment = Enum.HorizontalAlignment.Center })
		local P = Instance.new("UIPadding")
		P.PaddingLeft = UDim.new(0, 10)
		P.PaddingRight = UDim.new(0.1, 0)
		P.PaddingTop = UDim.new(0, 6)
		P.Parent = H
	end
	do
		local _, _, Frames = Case("Invisible", { CellSize = Cell })
		Frames[2].Visible = false
	end
	Case("OddSizes", { CellSize = UDim2.fromOffset(33, 21), CellPadding = UDim2.fromOffset(3, 2), HorizontalAlignment = Enum.HorizontalAlignment.Center, VerticalAlignment = Enum.VerticalAlignment.Center }, 7, UDim2.fromOffset(181, 131))

	task.wait()
	for _, Entry in Built do
		local Origin = Entry.Holder.AbsolutePosition
		local Rects = {}
		for _, F in Entry.Frames do
			table.insert(Rects, { F.AbsolutePosition.X - Origin.X, F.AbsolutePosition.Y - Origin.Y, F.AbsoluteSize.X, F.AbsoluteSize.Y })
		end
		local G = Entry.Grid
		Results[Entry.Name] = {
			items = Rects,
			content = { G.AbsoluteContentSize.X, G.AbsoluteContentSize.Y },
			cell = { G.AbsoluteCellSize.X, G.AbsoluteCellSize.Y },
			count = { G.AbsoluteCellCount.X, G.AbsoluteCellCount.Y },
		}
	end
	return Results
end
