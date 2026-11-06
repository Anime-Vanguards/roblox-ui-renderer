-- Color3:ToHex / Color3.fromHex in Studio (case, rounding, out-of-range components). No GUI.
--   node tools/studio-lock.mjs node tools/studio-mcp.cjs luau tools/sweep/probe-tohex.luau

local HttpService = game:GetService("HttpService")

local Result = {}
local Cases = {
	{ "rgb 226 224 234", Color3.fromRGB(226, 224, 234) },
	{ "1 0.5 0", Color3.new(1, 0.5, 0) },
	{ "0.1 0.2 0.3", Color3.new(0.1, 0.2, 0.3) },
	{ "0.998 0.002 0.5019", Color3.new(0.998, 0.002, 0.5019) },
	{ "127.5 / 255", Color3.new(127.5 / 255, 127.49 / 255, 127.51 / 255) },
	{ "2 -1 0", Color3.new(2, -1, 0) },
	{ "fromHex ABCDEF", Color3.fromHex("#ABCDEF") },
	{ "fromHex abc", Color3.fromHex("abc") },
}
for _, Case in Cases do
	local Color = Case[2]
	table.insert(Result, {
		name = Case[1],
		hex = Color:ToHex(),
		r = string.format("%.17g", Color.R),
		g = string.format("%.17g", Color.G),
		b = string.format("%.17g", Color.B),
	})
end
return HttpService:JSONEncode(Result)
