return function(Target)
    local Scroll = Instance.new("ScrollingFrame")
    Scroll.Name = "NativeWorkerScroll"
    Scroll.Size = UDim2.fromOffset(240, 180)
    Scroll.CanvasSize = UDim2.fromOffset(240, 900)
    Scroll.Parent = Target
    for Index = 1, 8 do
        local Row = Instance.new("Frame")
        Row.Name = `Row{Index}`
        Row.Size = UDim2.fromOffset(180, 50)
        Row.Position = UDim2.fromOffset(10, (Index - 1) * 75)
        Row.BackgroundColor3 = Color3.fromRGB(Index * 25, 90, 180)
        Row.Parent = Scroll
    end
end
