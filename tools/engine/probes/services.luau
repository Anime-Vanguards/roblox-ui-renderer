-- Engine probe: service behaviours scripts see (runtime/engine/services.luau): what each call
-- returns or raises, printed for the differential harness (now the golden fixtures, tools/engine/golden.mjs).
return function(Target)
	local function Show(Value)
		if type(Value) == "table" then
			local Keys = {}
			for Key in Value do
				table.insert(Keys, Key)
			end
			table.sort(Keys, function(A, B)
				return tostring(A) < tostring(B)
			end)
			local Parts = {}
			for _, Key in Keys do
				table.insert(Parts, `{tostring(Key)}={Show(Value[Key])}`)
			end
			return "{" .. table.concat(Parts, ",") .. "}"
		end
		return tostring(Value)
	end
	local function Try(Label, Callback, ...)
		local Results = table.pack(pcall(Callback, ...))
		local Parts = {}
		for Index = 1, Results.n do
			table.insert(Parts, Show(Results[Index]))
		end
		print(Label, table.concat(Parts, " | "))
	end

	local Http = game:GetService("HttpService")
	Try("guid", function() return Http:GenerateGUID(), Http:GenerateGUID(false) end)
	Try("json-encode", function() return Http:JSONEncode({ a = 1, b = { 1, 2.5, "x" }, c = true, d = 0.1, e = 1 / 3, f = Vector3.new(1, 2, 3), g = Enum.Font.Arial }) end)
	Try("json-encode-array", function() return Http:JSONEncode({ 1, 2, 3, 1e20, -0.5, 123456789.125 }) end)
	Try("json-decode", function() return Http:JSONDecode('{"x":[1,2,{"y":null}],"z":"s","n":24490.894583499998,"m":0.016666666666666666}') end)
	Try("json-decode-number", function()
		local Decoded = Http:JSONDecode("[24490.894583499998, 1.7976931348623157e308, 5e-324, 123456789012345678901234567890]")
		return string.format("%.17g %.17g %.17g %.17g", Decoded[1], Decoded[2], Decoded[3], Decoded[4])
	end)
	Try("json-decode-bad", function() return Http:JSONDecode("{nope") end)
	Try("url-encode", function() return Http:UrlEncode("a b&c=d/é~._-") end)
	Try("http-get", function() return Http:GetAsync("https://example.com") end)

	local Players = game:GetService("Players")
	local Player = Players.LocalPlayer
	Try("player", function() return Player.Name, Player.DisplayName, Player.UserId, #Players:GetPlayers() end)
	Try("player-by-id", function() return Players:GetPlayerByUserId(Player.UserId) == Player, Players:GetPlayerByUserId(999) end)
	Try("names", function() return Players:GetNameFromUserIdAsync(1), Players:GetNameFromUserIdAsync(156), Players:GetNameFromUserIdAsync(123456) end)
	Try("names-unknown", function() return Players:GetNameFromUserIdAsync(4) end)
	Try("thumbnail", function() return Players:GetUserThumbnailAsync(1, Enum.ThumbnailType.AvatarBust, Enum.ThumbnailSize.Size150x150) end)
	Try("user-infos", function() return game:GetService("UserService"):GetUserInfosByUserIdsAsync({ 1, 4, 88, 777 }) end)
	Try("player-methods", function() return Player:IsInGroup(1), Player:GetRankInGroup(1), Player:GetRoleInGroup(1), Player:GetNetworkPing() end)
	Try("player-children", function()
		local Names = {}
		for _, Child in Player:GetChildren() do
			table.insert(Names, Child.Name)
		end
		return table.concat(Names, ",")
	end)

	local Market = game:GetService("MarketplaceService")
	Try("product-info", function() return Market:GetProductInfo(12345, Enum.InfoType.Product) end)
	Try("owns", function() return Market:UserOwnsGamePassAsync(1, 2), Market:PlayerOwnsAsset(Player, 3) end)
	Try("pages", function()
		local Pages = Market:GetDeveloperProductsAsync()
		return Pages.IsFinished, #Pages:GetCurrentPage()
	end)
	Try("policy", function() return game:GetService("PolicyService"):GetPolicyInfoForPlayerAsync(Player) end)
	Try("locale", function()
		local Localization = game:GetService("LocalizationService")
		local Translator = Localization:GetTranslatorForPlayerAsync(Player)
		return Localization.RobloxLocaleId, Translator:Translate(Target, "Hello"), Translator:FormatByKey("K")
	end)

	local Collection = game:GetService("CollectionService")
	local Tagged = Instance.new("Folder")
	Tagged.Name = "Tagged"
	local Added = {}
	Collection:GetInstanceAddedSignal("Probe"):Connect(function(Instance_)
		table.insert(Added, Instance_.Name)
	end)
	Collection:AddTag(Tagged, "Probe")
	Tagged.Parent = Target
	Collection:AddTag(Tagged, "Probe")
	local Other = Instance.new("Frame")
	Other.Name = "Other"
	Other:AddTag("Probe")
	Other.Parent = Target
	Other:AddTag("Second")
	task.defer(function()
		Try("tags", function()
			local Names = {}
			for _, Instance_ in Collection:GetTagged("Probe") do
				table.insert(Names, Instance_.Name)
			end
			local All = Collection:GetAllTags()
			table.sort(All)
			return table.concat(Names, ","), table.concat(Added, ","), Collection:HasTag(Other, "Second"), #Collection:GetTags(Other), table.concat(All, ",")
		end)
	end)

	local Gui = game:GetService("GuiService")
	Try("gui-inset", function() return Gui:GetGuiInset() end)
	Try("gui-topbar", function() return Gui.TopbarInset, Gui:IsTenFootInterface() end)
	local Input = game:GetService("UserInputService")
	Try("input", function() return Input.TouchEnabled, Input.KeyboardEnabled, Input.MouseEnabled, Input.GamepadEnabled end)
	-- (GetMouseLocation answers runtime/input's live pointer: phase 5)
	Try("input-last", function() return Input:GetLastInputType(), Input:IsKeyDown(Enum.KeyCode.A) end)
	Try("input-keycode", function() return Input:GetStringForKeyCode(Enum.KeyCode.Q), #Input:GetKeysPressed() end)
	Try("starter-gui", function() return game:GetService("StarterGui"):GetCore("ChatActive"), game:GetService("StarterGui"):GetCoreGuiEnabled(Enum.CoreGuiType.All) end)

	local Tweens = game:GetService("TweenService")
	Try("ease", function()
		local Values = {}
		for _, Style in { "Linear", "Sine", "Back", "Quad", "Quart", "Quint", "Bounce", "Elastic", "Exponential", "Circular", "Cubic" } do
			for _, Direction in { "In", "Out", "InOut" } do
				table.insert(Values, string.format("%.9g", Tweens:GetValue(0.37, Enum.EasingStyle[Style], Enum.EasingDirection[Direction])))
			end
		end
		return table.concat(Values, " ")
	end)
	Try("ease-bad", function() return Tweens:GetValue(0.5, "Nope", Enum.EasingDirection.In) end)
	Try("smooth-damp", function()
		local Value, Velocity = Tweens:SmoothDamp(0, 10, 0, 0.3, math.huge, 1 / 60)
		local V2, V2Velocity = Tweens:SmoothDamp(Vector2.new(0, 0), Vector2.new(10, 5), Vector2.zero, 0.2)
		return string.format("%.9g %.9g", Value, Velocity), V2, V2Velocity
	end)
	Try("tween-create-bad", function() return Tweens:Create(Target, TweenInfo.new(1), { Name = "x" }) end)
	Try("tween-create-type", function() return Tweens:Create(Target, TweenInfo.new(1), { Size = 5 }) end)
	Try("tween-create-missing", function() return Tweens:Create(Target, TweenInfo.new(1), { Nope = 5 }) end)

	local Sound = Instance.new("Sound")
	Sound:Play()
	Try("sound", function() return Sound.Playing, Sound.IsLoaded, Sound.TimeLength end)
	Sound:Stop()
	Try("sound-stop", function() return Sound.Playing end)

	local Model = Instance.new("Model")
	local Part = Instance.new("Part")
	Part.Size = Vector3.new(2, 4, 6)
	Part.CFrame = CFrame.new(1, 2, 3)
	Part.Parent = Model
	Try("model", function() return Model:GetExtentsSize(), Model:GetPivot().Position, Model:GetScale() end)
	Model:PivotTo(CFrame.new(5, 5, 5))
	Try("model-pivot", function() return Model:GetPivot().Position end)
	local Humanoid = Instance.new("Humanoid")
	Try("humanoid", function() return Humanoid:GetState(), Humanoid:GetStateEnabled(Enum.HumanoidStateType.Jumping), typeof(Humanoid:LoadAnimation(Instance.new("Animation"))) end)
	local Path = Instance.new("Path2D")
	Try("path2d", function()
		Path:SetControlPoints({ Path2DControlPoint.new(UDim2.fromOffset(1, 2)), Path2DControlPoint.new(UDim2.fromOffset(3, 4)) })
		return #Path:GetControlPoints(), Path:GetControlPoint(2).Position.X.Offset, Path:GetMaxControlPoints()
	end)
	Try("path2d-entry", function() Path:SetControlPoints({ 1, 2, 3 }) end)
	Try("editable", function()
		local Image = game:GetService("AssetService"):CreateEditableImage({ Size = Vector2.new(4, 2) })
		return Image.Size, buffer.len(Image:ReadPixelsBuffer(Vector2.zero, Vector2.new(4, 2)))
	end)

	Try("datamodel", function() return game.PlaceId, game.GameId, game.JobId, game.CreatorType, game.PlaceVersion, game:IsLoaded() end)
	Try("run-service", function()
		local Run = game:GetService("RunService")
		return Run:IsStudio(), Run:IsClient(), Run:IsServer(), Run:IsRunning(), Run:IsEdit(), Run:IsRunMode()
	end)
	Try("workspace", function() return workspace.DistributedGameTime, workspace:GetRealPhysicsFPS(), workspace.CurrentCamera.ViewportSize end)
	Try("bindable-function", function()
		local Function = Instance.new("BindableFunction")
		Function.OnInvoke = function(A, B)
			return A + B, "sum"
		end
		return Function:Invoke(2, 3)
	end)
	Try("remote-event", function() return Instance.new("RemoteEvent"):FireServer(1) end)
	Try("text-family", function() return game:GetService("TextService"):GetFamilyInfoAsync("rbxasset://fonts/families/GothamSSm.json") end)
	Try("text-size", function() return game:GetService("TextService"):GetTextSize("Hello world", 18, Enum.Font.GothamBold, Vector2.new(1000, 1000)) end)

	-- Debris and ContentProvider yield / delay on virtual time
	local Temporary = Instance.new("Frame")
	Temporary.Name = "Temporary"
	Temporary.Parent = Target
	game:GetService("Debris"):AddItem(Temporary, 0.1)
	task.spawn(function()
		local Seen = {}
		game:GetService("ContentProvider"):PreloadAsync({ "rbxassetid://1", Target }, function(Id, Status)
			table.insert(Seen, `{Id}:{Status.Name}`)
		end)
		print("preload", table.concat(Seen, ","), os.clock() - 10000)
	end)
	task.delay(0.2, function()
		print("debris", Temporary.Parent)
	end)
	-- A RemoteFunction never returns offline
	task.spawn(function()
		Instance.new("RemoteFunction"):InvokeServer()
		print("remote returned")
	end)
end
