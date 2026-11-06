-- Builds snapshot files (`<Service>.rbxm`, the runtime's --snapshot-dir format) from instances Studio
-- serialized one by one (tools/sweep/studio-assets.mjs). Each item names its service, the ancestors
-- between the service and the instance ({ name, className }, outermost first) and its .rbxm file; items
-- that share ancestors share them in the output.
--   lune run tools/sweep/assemble-snapshot.luau <spec.json>
-- spec.json: { "out": "<dir>", "items": [ { "service", "chain": [ { "name", "className" } ], "file" } ] }

local Fs = require("@lune/fs")
local Process = require("@lune/process")
local Roblox = require("@lune/roblox")
local Serde = require("@lune/serde")

local Spec = Serde.decode("json", Fs.readFile(Process.args[1]))

-- Top-level instances per service, in item order
local Services: { [string]: { any } } = {}
local ServiceOrder: { string } = {}

local function FindChild(Siblings: { any }, Name: string, ClassName: string): any?
	for _, Sibling in Siblings do
		if Sibling.Name == Name and Sibling.ClassName == ClassName then
			return Sibling
		end
	end
	return nil
end

for _, Item in Spec.items do
	if not Services[Item.service] then
		Services[Item.service] = {}
		table.insert(ServiceOrder, Item.service)
	end
	local Roots = Services[Item.service]

	local Parent = nil
	for _, Link in Item.chain do
		local Siblings = if Parent then Parent:GetChildren() else Roots
		local Found = FindChild(Siblings, Link.name, Link.className)
		if not Found then
			Found = Roblox.Instance.new(Link.className)
			Found.Name = Link.name
			if Parent then
				Found.Parent = Parent
			else
				table.insert(Roots, Found)
			end
		end
		Parent = Found
	end

	for _, Root in Roblox.deserializeModel(Fs.readFile(Item.file)) do
		if Parent then
			Root.Parent = Parent
		else
			table.insert(Roots, Root)
		end
	end
end

for _, Service in ServiceOrder do
	local File = `{Spec.out}/{Service}.rbxm`
	Fs.writeFile(File, Roblox.serializeModel(Services[Service]))
	print(`{File}: {#Services[Service]} top-level instance(s)`)
end
