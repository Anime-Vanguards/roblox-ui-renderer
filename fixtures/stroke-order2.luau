-- UIStroke draw order among strokes of equal ZIndex, part 2 (strokes round 6): does the way the strokes reach the
-- rendered tree decide which one Studio draws on top? Every cell is a 50 x 30 frame with same-footprint Center
-- strokes (8 px), so the band shows only the top stroke's colour. Eight identical cells per row:
--   R0 frame parented first, then red, blue (incremental, like stroke-order.luau)
--   R1 red, blue parented to the frame, then the frame parented (a subtree insert, how Vide builds UI)
--   R2 like R1, the frame inside a holder that is parented last
--   R3 like R1 with blue parented before red
--   R4 like R0, red's Color nudged at t = 0.05 s
--   R5 like R1, red's Transparency nudged at t = 0.05 s
--   R6 red, green, blue, subtree insert
--   R7 red, green, blue, incremental
--   R8 like R1 with the red stroke reparented away and back at t = 0.05 s
-- compare-unstable: Studio draws strokes of equal ZIndex in an order that changes from mount to mount (notes/strokes.md)
-- compare: --viewport 1200x900 --time 0.3 --background 20,20,26
return function(Target)
	local Red, Green, Blue = Color3.fromRGB(230, 40, 40), Color3.fromRGB(40, 200, 90), Color3.fromRGB(40, 90, 230)
	local function Frame(X, Y)
		local F = Instance.new("Frame")
		F.Position = UDim2.fromOffset(X, Y)
		F.Size = UDim2.fromOffset(50, 30)
		F.BackgroundColor3 = Color3.fromRGB(70, 70, 90)
		F.BorderSizePixel = 0
		return F
	end
	local function Stroke(Color)
		local S = Instance.new("UIStroke")
		S.Color = Color
		S.Thickness = 8
		S.BorderStrokePosition = Enum.BorderStrokePosition.Center
		return S
	end
	local Later = {}
	for Col = 0, 7 do
		local X = 30 + Col * 90
		local function Y(Row)
			return 30 + Row * 70
		end
		-- R0
		do
			local F = Frame(X, Y(0))
			F.Parent = Target
			Stroke(Red).Parent = F
			Stroke(Blue).Parent = F
		end
		-- R1
		do
			local F = Frame(X, Y(1))
			Stroke(Red).Parent = F
			Stroke(Blue).Parent = F
			F.Parent = Target
		end
		-- R2
		do
			local H = Instance.new("Frame")
			H.BackgroundTransparency = 1
			H.Size = UDim2.fromOffset(0, 0)
			local F = Frame(X, Y(2))
			Stroke(Red).Parent = F
			Stroke(Blue).Parent = F
			F.Parent = H
			H.Parent = Target
		end
		-- R3
		do
			local F = Frame(X, Y(3))
			local R = Stroke(Red)
			Stroke(Blue).Parent = F
			R.Parent = F
			F.Parent = Target
		end
		-- R4
		do
			local F = Frame(X, Y(4))
			F.Parent = Target
			local R = Stroke(Red)
			R.Parent = F
			Stroke(Blue).Parent = F
			table.insert(Later, function()
				R.Color = Color3.fromRGB(231, 40, 40)
			end)
		end
		-- R5
		do
			local F = Frame(X, Y(5))
			local R = Stroke(Red)
			R.Parent = F
			Stroke(Blue).Parent = F
			F.Parent = Target
			table.insert(Later, function()
				R.Transparency = 0.001
			end)
		end
		-- R6
		do
			local F = Frame(X, Y(6))
			Stroke(Red).Parent = F
			Stroke(Green).Parent = F
			Stroke(Blue).Parent = F
			F.Parent = Target
		end
		-- R7
		do
			local F = Frame(X, Y(7))
			F.Parent = Target
			Stroke(Red).Parent = F
			Stroke(Green).Parent = F
			Stroke(Blue).Parent = F
		end
		-- R8
		do
			local F = Frame(X, Y(8))
			local R = Stroke(Red)
			R.Parent = F
			Stroke(Blue).Parent = F
			F.Parent = Target
			table.insert(Later, function()
				R.Parent = nil
				R.Parent = F
			end)
		end
	end
	task.delay(0.05, function()
		for _, Fn in Later do
			Fn()
		end
	end)
end
