-- Read-only: what CoreGui content sits under a few screen points (no GUI created, no input).
local CoreGui = game:GetService("CoreGui")
local HttpService = game:GetService("HttpService")
local Result = { children = {}, at = {} }
for _, Child in CoreGui:GetChildren() do
	local Entry = Child.ClassName .. " " .. Child.Name
	if Child:IsA("LayerCollector") then
		Entry ..= ` enabled={Child.Enabled}`
	end
	if Child:IsA("ScreenGui") then
		Entry ..= ` display={Child.DisplayOrder}`
	end
	table.insert(Result.children, Entry)
end
for _, Point in { { 10, 10 }, { 150, 150 }, { 600, 600 }, { 1000, 500 }, { 2000, 1100 } } do
	local Ok, List = pcall(function()
		return CoreGui:GetGuiObjectsAtPosition(Point[1], Point[2])
	end)
	local Names = {}
	if Ok then
		for _, Object in List do
			table.insert(Names, Object:GetFullName() .. ` ({Object.ClassName} active={Object.Active} vis={Object.Visible})`)
		end
	else
		Names = { "ERROR " .. tostring(List) }
	end
	Result.at[`{Point[1]},{Point[2]}`] = Names
end
return HttpService:JSONEncode(Result)
