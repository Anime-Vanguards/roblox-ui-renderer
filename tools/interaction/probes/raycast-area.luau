-- Read-only: which 3D objects sit behind screen points of the probe area (edit camera)?
local Camera = workspace.CurrentCamera
local HttpService = game:GetService("HttpService")
local Hits = {}
local Count, Empty = 0, 0
for Y = 0, 1150, 25 do
	local Row = {}
	for X = 0, 2050, 25 do
		local Ray = Camera:ViewportPointToRay(X, Y)
		local Result = workspace:Raycast(Ray.Origin, Ray.Direction * 20000)
		Count += 1
		if Result then
			table.insert(Row, "#")
			Hits[Result.Instance:GetFullName()] = (Hits[Result.Instance:GetFullName()] or 0) + 1
		else
			table.insert(Row, ".")
			Empty += 1
		end
	end
	Hits["row " .. string.format("%04d", Y)] = table.concat(Row)
end
return HttpService:JSONEncode({ count = Count, empty = Empty, hits = Hits, camera = tostring(Camera.CFrame), focus = tostring(Camera.Focus) })
