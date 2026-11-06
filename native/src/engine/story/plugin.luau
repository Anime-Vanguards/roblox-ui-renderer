--!nocheck
-- `--plugin`: a stand-in for Studio's `plugin` global (story.rs mounts it before the story), for
-- plugin UIs previewed offline. Run with the engine's shared globals; returns the plugin object
-- and the list of dock widgets it created (exported as layers after the story's own gui).
--
--   GetSetting / SetSetting        in memory, seeded from --plugin <settings JSON> when given
--   CreateDockWidgetPluginGui       a real DockWidgetPluginGui in CoreGui, Enabled, the size of the
--                                   viewport; parent UI into it (or Vide/Roact mount into it)
--   CreateToolbar():CreateButton()  objects whose Click signal never fires
--   CreatePluginAction / Menu       objects with Triggered signals; ShowAsync returns nil
--   GetMouse                        a mouse at (0, 0) with signals that never fire
--   Activate / Deactivate / IsActivated, Unloading / Deactivation signals, GetStudioUserId (0)
--
-- Anything else raises "<Key> is not a valid member of Plugin".

local NewWidget, Settings = ...

local function Signal()
	return Instance.new("BindableEvent")
end

-- Objects with fixed fields; signal fields are created on first use
local function Object(ClassName, Fields, SignalNames, NilFields)
	local Events = {}
	return setmetatable(Fields, {
		__index = function(_, Key)
			if SignalNames[Key] then
				local Event = Events[Key] or Signal()
				Events[Key] = Event
				return Event.Event
			end
			if NilFields and NilFields[Key] then
				return nil
			end
			error(`{tostring(Key)} is not a valid member of {ClassName}`, 2)
		end,
		__tostring = function()
			return Fields.Name or ClassName
		end,
	})
end

local function Action(Id, Text, StatusTip, Icon)
	return Object("PluginAction", {
		ActionId = Id,
		Name = Id,
		Text = Text or "",
		StatusTip = StatusTip or "",
		Icon = Icon or "",
		AllowBinding = true,
		Enabled = true,
		Checked = false,
	}, { Triggered = true })
end

local function Menu(Id, Title, Icon)
	local Menu
	Menu = Object("PluginMenu", {
		Name = Id or "",
		Title = Title or "",
		Icon = Icon or "",
		AddAction = function() end,
		AddNewAction = function(_, ActionId, Text, ActionIcon)
			return Action(ActionId, Text, nil, ActionIcon)
		end,
		AddMenu = function() end,
		AddSeparator = function() end,
		Clear = function() end,
		ShowAsync = function()
			return nil
		end,
		Destroy = function() end,
	}, {})
	return Menu
end

local Widgets = {}
local Active = false
local Plugin = Object("Plugin", {
	Name = "Plugin",
	CollisionEnabled = false,
	GridSize = 1,
	UsesAssetInsertionDrag = false,
	GetSetting = function(_, Key)
		return Settings[Key]
	end,
	SetSetting = function(_, Key, Value)
		Settings[Key] = Value
	end,
	CreateDockWidgetPluginGui = function(_, Id, _Info)
		local Widget = NewWidget(tostring(Id))
		Widget.Title = tostring(Id)
		-- Shown, as if the plugin's toolbar button had opened it (nothing clicks it offline)
		Widget.Enabled = true
		table.insert(Widgets, Widget)
		return Widget
	end,
	CreateToolbar = function(_, Name)
		return Object("PluginToolbar", {
			Name = Name or "",
			CreateButton = function(_, Id, Tooltip, Icon, Text)
				return Object("PluginToolbarButton", {
					Name = Id or "",
					Tooltip = Tooltip or "",
					Icon = Icon or "",
					Text = Text or Id or "",
					Enabled = true,
					ClickableWhenViewportHidden = false,
					SetActive = function() end,
					Destroy = function() end,
				}, { Click = true })
			end,
			Destroy = function() end,
		}, {})
	end,
	CreatePluginAction = function(_, Id, Text, StatusTip, Icon)
		return Action(Id, Text, StatusTip, Icon)
	end,
	CreatePluginMenu = function(_, Id, Title, Icon)
		return Menu(Id, Title, Icon)
	end,
	GetMouse = function()
		return Object("PluginMouse", {
			X = 0,
			Y = 0,
			ViewSizeX = 0,
			ViewSizeY = 0,
			Hit = CFrame.new(),
			Origin = CFrame.new(),
			UnitRay = Ray.new(Vector3.zero, Vector3.new(0, 0, -1)),
			Icon = "",
		}, { Button1Down = true, Button1Up = true, Button2Down = true, Button2Up = true, Move = true, Idle = true, WheelForward = true, WheelBackward = true, DragEnter = true }, { Target = true, TargetFilter = true })
	end,
	Activate = function()
		Active = true
	end,
	Deactivate = function()
		Active = false
	end,
	IsActivated = function()
		return Active
	end,
	GetStudioUserId = function()
		return 0
	end,
	GetSelectedRibbonTool = function()
		return Enum.RibbonTool.None
	end,
	SelectRibbonTool = function() end,
	OpenScript = function() end,
	OpenWikiPage = function() end,
	SaveSelectedToRoblox = function() end,
	PromptSaveSelection = function() end,
	StartDrag = function() end,
}, { Unloading = true, Deactivation = true, Ready = true })

return Plugin, Widgets
