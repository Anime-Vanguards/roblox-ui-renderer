-- Engine probe: creation serials (GetDebugId) of the DataModel's services and mounted roots, so the
-- harness can locate where the Rust engine's project mount diverges from the Luau runtime's.
return function(Target)
	local Lines = {}
	for _, Service in game:GetChildren() do
		table.insert(Lines, `{Service:GetDebugId()} {Service.ClassName} {Service.Name}`)
	end
	print(table.concat(Lines, "\n"))
	print("target", Target:GetDebugId(), script:GetDebugId(), script.Parent:GetDebugId())
end
