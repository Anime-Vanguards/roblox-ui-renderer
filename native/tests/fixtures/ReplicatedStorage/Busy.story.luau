return function(Target)
    local Frame = Instance.new("Frame")
    Frame.Name = "BeforeBusyCallback"
    Frame.Parent = Target
    game:GetService("RunService").RenderStepped:Connect(function()
        local Value = 0
        while true do Value += 1 end
    end)
end
