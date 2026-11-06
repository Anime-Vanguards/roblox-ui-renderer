-- TextScaled / TextWrapped property coupling on assignment.
return function(Target)
	local Results = {}
	local function State(Label)
		return { if Label.TextScaled then 1 else 0, if Label.TextWrapped then 1 else 0 }
	end
	local Steps = {
		{ "ScaledOn", function(L) L.TextScaled = true end },
		{ "ScaledOn_WrapOff", function(L) L.TextScaled = true L.TextWrapped = false end },
		{ "ScaledOn_ScaledOff", function(L) L.TextScaled = true L.TextScaled = false end },
		{ "WrapOn", function(L) L.TextWrapped = true end },
		{ "WrapOn_ScaledOn_WrapOff", function(L) L.TextWrapped = true L.TextScaled = true L.TextWrapped = false end },
		{ "ScaledOn_WrapOn", function(L) L.TextScaled = true L.TextWrapped = true end },
		{ "WrapOff_Only", function(L) L.TextWrapped = false end },
		{ "ScaledOn_WrapOff_ScaledOn", function(L) L.TextScaled = true L.TextWrapped = false L.TextScaled = true end },
	}
	for _, Step in Steps do
		local Label = Instance.new("TextLabel")
		Label.Parent = Target
		Step[2](Label)
		Results[Step[1]] = State(Label)
	end
	return Results
end
