-- Font.fromEnum faces and their TextBounds vs explicit Font.new.
return function(Target)
	local Results = {}
	local Labels = {}
	for _, Name in { "GothamBlack", "Gotham", "GothamBold", "GothamMedium", "Arial", "ArialBold", "SourceSansBold", "Creepster", "Legacy" } do
		local Face = Font.fromEnum((Enum.Font :: any)[Name])
		local Label = Instance.new("TextLabel")
		Label.Size = UDim2.fromOffset(300, 24)
		Label.FontFace = Face
		Label.TextSize = 18
		Label.Text = Name .. " Hamburgefonstiv 0123"
		Label.Parent = Target
		Labels[Name] = Label
		Results[Name .. "_face"] = { Face.Family, Face.Weight.Name, Face.Style.Name }
	end
	task.wait()
	for Name, Label in Labels do
		Results[Name] = { Label.TextBounds.X, Label.TextBounds.Y }
	end
	task.wait(1)
	for Name, Label in Labels do
		Results[Name .. "_later"] = { Label.TextBounds.X, Label.TextBounds.Y }
	end
	return Results
end
