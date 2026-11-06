-- Events fired from inside deferred handlers (Workspace.SignalBehavior Deferred): a property-changed
-- handler that sets another object's property, and task.defer callbacks that fire a BindableEvent.
-- Nothing is parented to the place.
--   node tools/studio-lock.mjs node tools/studio-mcp.cjs luau tools/sweep/probe-defer-chain.luau
-- Offline: node cli/rur.mjs render --fixture fixtures/stories-defer-chain.luau prints the same counts.

local HttpService = game:GetService("HttpService")

local A = Instance.new("Frame")
local B = Instance.new("Frame")
local Counts = { a = 0, b = 0, event = 0, nested = 0 }

B:GetPropertyChangedSignal("BackgroundColor3"):Connect(function()
	Counts.b += 1
end)
A:GetPropertyChangedSignal("Size"):Connect(function()
	Counts.a += 1
	B.BackgroundColor3 = Color3.new(Counts.a / 20, 0, 0)
end)

local Bindable = Instance.new("BindableEvent")
Bindable.Event:Connect(function()
	Counts.event += 1
	task.defer(function()
		Counts.nested += 1
	end)
end)

for Index = 1, 10 do
	A.Size = UDim2.fromOffset(Index, Index)
	task.wait()
end
for _ = 1, 5 do
	task.defer(function()
		Bindable:Fire()
	end)
	task.wait()
end
task.wait(0.1)

A:Destroy()
B:Destroy()
Bindable:Destroy()
return HttpService:JSONEncode({ counts = Counts })
