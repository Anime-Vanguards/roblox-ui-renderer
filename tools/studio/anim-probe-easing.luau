-- TweenService:GetValue(alpha, style, direction) for every EasingStyle x EasingDirection on a fine
-- alpha grid (-0.25 .. 1.25 in 1/512 steps) plus edge cases, as %.9g strings (the result is a float).
return function(Target)
	local TweenService = game:GetService("TweenService")
	local Alphas = {}
	for Index = -128, 640 do
		table.insert(Alphas, Index / 512)
	end
	for _, Extra in { 1e-7, 1e-4, 0.001, 0.1, 0.3, 1 / 3, 0.7, 0.999, 0.9999, 0.9999999, 1.0000001, 2, -1, 10, -10, 0.49999997, 0.50000006, 0.3636363636, 0.7272727272, 0.9090909090, 0.95454545, 0.1234567, 0.0625, 0.9375 } do
		table.insert(Alphas, Extra)
	end

	local Result = { Alphas = {}, Values = {} }
	for _, Alpha in Alphas do
		table.insert(Result.Alphas, string.format("%.17g", Alpha))
	end
	for _, Style in Enum.EasingStyle:GetEnumItems() do
		for _, Direction in Enum.EasingDirection:GetEnumItems() do
			local Row = {}
			for _, Alpha in Alphas do
				table.insert(Row, string.format("%.9g", TweenService:GetValue(Alpha, Style, Direction)))
			end
			Result.Values[`{Style.Name}.{Direction.Name}`] = table.concat(Row, " ")
		end
	end
	return Result
end
