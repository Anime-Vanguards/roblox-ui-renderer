-- Dumps Font.fromEnum for every Enum.Font item (family, weight, style) plus TextService family info.

local HttpService = game:GetService("HttpService")

local FromEnum = {}
for _, Item in Enum.Font:GetEnumItems() do
	local Ok, Face = pcall(Font.fromEnum, Item)
	if Ok then
		FromEnum[Item.Name] = { Face.Family, Face.Weight.Name, Face.Style.Name }
	end
end

return HttpService:JSONEncode({ fromEnum = FromEnum })
