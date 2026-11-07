--!nocheck
-- Rust engine host (native/src/engine/cli.rs): the story-mounting half of runtime/main.luau,
-- run with the engine's shared globals. Kept in Luau so the story gui is built through the same
-- script-visible property assignments as the Luau runtime's.

local Host = {}

-- MakeStoryGui: the ScreenGui and target frame a story mounts into.
function Host.MakeStoryGui(Margin, TargetWidth, TargetHeight)
	local Gui = Instance.new("ScreenGui")
	Gui.Name = "RURStory"
	Gui.IgnoreGuiInset = true
	Gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
	Gui.ResetOnSpawn = false

	local Target = Instance.new("Frame")
	Target.Name = "StoryTarget"
	Target.Size = if Margin > 0 then UDim2.fromOffset(TargetWidth, TargetHeight) else UDim2.fromScale(1, 1)
	Target.Position = UDim2.fromOffset(Margin, Margin)
	Target.BackgroundTransparency = 1
	Target.BorderSizePixel = 0
	Target.Parent = Gui

	Gui.Parent = game:GetService("CoreGui")
	return Gui, Target
end

-- UI Labs control conversion: every control becomes a vide source holding its default value.
local function ConvertControls(Controls, MakeSource)
	local Result = {}
	if type(Controls) ~= "table" then
		return Result
	end
	for Key, Control in Controls do
		local Value = Control
		if type(Control) == "table" then
			if Control.EntryType == "ControlGroup" then
				Result[Key] = ConvertControls(Control.Controls, MakeSource)
				continue
			elseif Control.EntryType == "Control" then
				Value = Control.ControlValue
				if Control.Type == "Choose" and type(Control.List) == "table" and Value == nil then
					Value = Control.List[1]
				end
			elseif #Control > 0 then
				Value = Control[1]
			end
		end
		Result[Key] = MakeSource(Value)
	end
	return Result
end

-- UI Labs controls (runtime/session.luau): a JSON-friendly description of each control and a
-- setter by path

local function DescribeValue(Value)
	local Kind = typeof(Value)
	if Kind == "boolean" or Kind == "number" or Kind == "string" then
		return Kind, Value
	elseif Kind == "Color3" then
		return "color", { Value.R, Value.G, Value.B }
	elseif Kind == "EnumItem" then
		return "enum", tostring(Value)
	end
	return "other", tostring(Value)
end

local function DescribeControls(Specs, Prefix, Out)
	if type(Specs) ~= "table" then
		return
	end
	local Keys = {}
	for Key in Specs do
		table.insert(Keys, Key)
	end
	table.sort(Keys, function(A, B)
		return tostring(A) < tostring(B)
	end)
	for _, Key in Keys do
		local Spec = Specs[Key]
		local Path = table.clone(Prefix)
		table.insert(Path, tostring(Key))
		if type(Spec) == "table" and Spec.EntryType == "ControlGroup" then
			DescribeControls(Spec.Controls, Path, Out)
			continue
		end
		local Entry = { path = Path }
		if type(Spec) == "table" and Spec.EntryType == "Control" then
			Entry.kind, Entry.value = DescribeValue(Spec.ControlValue)
			Entry.control = Spec.Type
			Entry.min = Spec.Min
			Entry.max = Spec.Max
			Entry.step = Spec.Step
			if type(Spec.List) == "table" then
				Entry.kind = "choose"
				Entry.options = {}
				for _, Option in Spec.List do
					local _, Described = DescribeValue(Option)
					table.insert(Entry.options, Described)
				end
				Entry.value = Entry.value ~= nil and Entry.value or Entry.options[1]
			end
		elseif type(Spec) == "table" and #Spec > 0 then
			Entry.kind = "choose"
			Entry.options = {}
			for _, Option in Spec do
				local _, Described = DescribeValue(Option)
				table.insert(Entry.options, Described)
			end
			Entry.value = Entry.options[1]
		else
			Entry.kind, Entry.value = DescribeValue(Spec)
		end
		table.insert(Out, Entry)
	end
end

local function FindControl(Controls, Specs, Path)
	local Source, Spec = Controls, Specs
	for _, Key in Path do
		if type(Source) ~= "table" then
			return nil, nil
		end
		Source = Source[Key]
		if type(Spec) == "table" then
			local Next = Spec[Key]
			if type(Next) == "table" and Next.EntryType == "ControlGroup" then
				Next = Next.Controls
			end
			Spec = Next
		end
	end
	return Source, Spec
end

-- Converts a JSON value back to the control's Luau type (colours arrive as [r, g, b]; choices by
-- 1-based index or by the option's label)
local function ControlValue(Spec, Value)
	local Current = if type(Spec) == "table" and Spec.EntryType == "Control" then Spec.ControlValue else Spec
	local List = if type(Spec) == "table" and Spec.EntryType == "Control" then Spec.List elseif type(Spec) == "table" and #Spec > 0 then Spec else nil
	if type(List) == "table" and type(Value) == "number" and List[Value] ~= nil then
		return List[Value]
	end
	if type(List) == "table" and type(Value) == "string" then
		for _, Option in List do
			local _, Described = DescribeValue(Option)
			if tostring(Described) == Value then
				return Option
			end
		end
	end
	if typeof(Current) == "Color3" and type(Value) == "table" then
		return Color3.new(Value[1], Value[2], Value[3])
	end
	return Value
end

-- Session.DescribeControls: the ready reply's control list
function Host.DescribeControls(Specs)
	local Out = {}
	DescribeControls(Specs, {}, Out)
	return Out
end

-- A session's control change (the "control" command, and "control" events of a frame):
-- `Strict` raises for a path without a settable source, as the command does; events skip it.
function Host.SetControl(Controls, Specs, Path, Value, Strict)
	local Source, Spec = FindControl(Controls, Specs, Path or {})
	if type(Source) ~= "function" and type(Source) ~= "table" then
		if Strict then
			error(`no control at {table.concat(Path or {}, ".")}`, 0)
		end
		return
	end
	Source(ControlValue(Spec, Value))
end

-- Session.ApplyControls: `--controls` of a one-shot render ({"Path.To.Control": value}, decoded),
-- applied to the story's control sources before it mounts. Unknown paths are an error.
local function ApplyControls(Controls, Specs, Decoded)
	for PathText, Value in Decoded do
		local Path = string.split(PathText, ".")
		local Source, ControlSpec = FindControl(Controls, Specs, Path)
		local Converted = ControlValue(ControlSpec, Value)
		if type(Source) == "function" or (type(Source) == "table" and getmetatable(Source) ~= nil) then
			Source(Converted)
		elseif Source ~= nil or ControlSpec ~= nil then
			-- Plain values (stories without vide sources) are replaced in place
			local Parent = FindControl(Controls, Specs, { table.unpack(Path, 1, #Path - 1) })
			Parent[Path[#Path]] = Converted
		else
			error(`--controls: no control at {PathText}`, 0)
		end
	end
end

-- MountStory: mounts a loaded story (or module) into the target. `State.Controls` (decoded
-- --controls) is applied before the mount; the story's control sources and definitions are
-- kept in State.StoryControls / State.StoryControlSpecs for live sessions.
function Host.MountStory(Story, Target, IsModule, State)
	if IsModule and not Host.IsStoryTable(Story) then
		local Result = if type(Story) == "function" then Story(Target, table.unpack(Host.ModuleArguments(State))) else Story
		if typeof(Result) == "Instance" and Result.Parent == nil then
			Result.Parent = Target
		end
	elseif type(Story) == "table" and Story.story then
		local Vide = Story.vide
		local Controls = ConvertControls(Story.controls, if Vide then Vide.source else function(Value)
			return Value
		end)
		if State and State.Controls ~= nil then
			ApplyControls(Controls, Story.controls, State.Controls)
		end
		local Props = { controls = Controls, target = Target, converted = Story.controls }
		if State then
			State.StoryControls, State.StoryControlSpecs = Controls, Story.controls
		end
		if Vide then
			Vide.mount(function()
				return Story.story(Props)
			end, Target)
		else
			local Result = Story.story(Props)
			if typeof(Result) == "Instance" then
				Result.Parent = Target
			end
		end
	elseif type(Story) == "table" and Story.render then
		Host.MountGeneric(Story, Target, State)
	elseif type(Story) == "function" then
		Story(Target)
	else
		error("unsupported story format (expected UI Labs table with story, or a function)", 0)
	end
end

-- The require + mount of main.luau: a story whose require yields mounts when it finishes.
function Host.LoadAndMount(Require, Module, Target, IsModule, BeginMount, State)
	local Loaded = Require(Module)
	if State.RequireYielded then
		BeginMount()
		Host.MountStory(Loaded, Target, IsModule, State)
	end
	return Loaded
end

-- Engine.ClientGuiMount (main.luau, rur.json clientGui): requires the ModuleScript at `ModulePath`
-- (dot-separated from game) and calls its Mount(), once, when an edit-mode story first reaches the
-- client's PlayerGui (lua/globals.rs missing_child).
function Host.ClientGuiMount(ModulePath, Require, Warn)
	local Current = game
	for Name in string.gmatch(ModulePath, "[^%.]+") do
		Current = Current and Current:FindFirstChild(Name)
	end
	if not Current then
		Warn(`rur.json clientGui: {ModulePath} not found`)
		return
	end
	local Mounted, Error = pcall(function()
		local ClientGui = Require(Current)
		if type(ClientGui) == "table" and type(ClientGui.Mount) == "function" then
			ClientGui.Mount()
		end
	end)
	if not Mounted then
		Warn(`rur.json clientGui {ModulePath}: {Error}`)
	end
end

-- (Helpers below MountStory keep its line numbers, which the golden tracebacks record)

-- A module / fixture returning a UI Labs story table mounts as a story (with --controls)
function Host.IsStoryTable(Story)
	return type(Story) == "table" and (type(Story.story) == "function" or type(Story.render) == "function")
end

-- The arguments after Target of a function(Target, Controls) module: --controls as nested tables
-- ({"Size.Scale": 2} -> { Size = { Scale = 2 } }), or none
function Host.ModuleArguments(State)
	if not State or State.Controls == nil then
		return {}
	end
	local Result = {}
	for PathText, Value in State.Controls do
		local Path = string.split(PathText, ".")
		local Parent = Result
		for Index = 1, #Path - 1 do
			local Next = Parent[Path[Index]]
			if type(Next) ~= "table" then
				Next = {}
				Parent[Path[Index]] = Next
			end
			Parent = Next
		end
		Parent[Path[#Path]] = Value
	end
	return { Result }
end

-- UI Labs generic stories (`render`) by their first values table: the subscribed listeners and
-- the current values once a control changed
Host.Generic = setmetatable({}, { __mode = "k" })

-- MountStory's generic story (`render`): the plain control values (with --controls applied),
-- kept in State.StoryControls / State.StoryControlSpecs like a story's, and props for render
function Host.MountGeneric(Story, Target, State)
	local Controls = ConvertControls(Story.controls, function(Value)
		return Value
	end)
	if State and State.Controls ~= nil then
		ApplyControls(Controls, Story.controls, State.Controls)
	end
	local Converted = Host.ConvertedControls(Story.controls)
	local Generic = { Listeners = {}, Converted = Converted }
	Host.Generic[Controls] = Generic
	if State then
		State.StoryControls, State.StoryControlSpecs = Controls, Story.controls
	end
	Story.render({
		target = Target,
		controls = Controls,
		converted = Converted,
		subscribe = function(Listener)
			if not table.find(Generic.Listeners, Listener) then
				table.insert(Generic.Listeners, Listener)
			end
			return function()
				local Index = table.find(Generic.Listeners, Listener)
				if Index then
					table.remove(Generic.Listeners, Index)
				end
			end
		end,
	})
end

-- The control a literal stands for in the UI Labs package (ConvertControl in ui-labs-utils,
-- src/Controls/ControlConversion.luau lines 20-34, github.com/PepeElToro41/ui-labs-utils at 77fddfb):
-- what its String, Number, Boolean and Color3 constructors make (PrimitiveControls.luau lines 7-35,
-- DatatypeControls.luau lines 5-8), or nil for a value it refuses
local function LiteralControl(Value)
	local Kind = typeof(Value)
	if Kind == "string" then
		return { EntryType = "Control", Type = "String", ControlValue = Value }
	elseif Kind == "number" then
		return { EntryType = "Control", Type = "Number", ControlValue = Value, Dragger = true, Sensibility = Value * 10 }
	elseif Kind == "boolean" then
		return { EntryType = "Control", Type = "Boolean", ControlValue = Value }
	elseif Kind == "Color3" then
		local Clamped = Color3.new(math.clamp(Value.R, 0, 1), math.clamp(Value.G, 0, 1), math.clamp(Value.B, 0, 1))
		return { EntryType = "Control", Type = "Color3", ControlValue = Clamped }
	end
	return nil
end

-- props.converted of a generic story, as the UI Labs plugin builds it (ConvertLiterals and
-- TryConvertControl in src/UI/StoryPreview/PreviewController/Mounters/Utils.tsx lines 20-51,
-- github.com/PepeElToro41/ui-labs at 1e8f45f): a table (a control, or a `{ value, ... }` list) is
-- passed as it is, a group is copied with its controls converted, a literal becomes its control,
-- and a value UI Labs makes no control of is left out with a warning (props.controls keeps it)
function Host.ConvertedControls(Specs, Prefix)
	local Result = {}
	if type(Specs) ~= "table" then
		return Result
	end
	for Key, Spec in Specs do
		local Name = if Prefix then `{Prefix}.{Key}` else tostring(Key)
		if type(Spec) == "table" and Spec.EntryType == "ControlGroup" then
			local Group = table.clone(Spec)
			Group.Controls = Host.ConvertedControls(Spec.Controls, Name)
			Result[Key] = Group
		elseif type(Spec) == "table" then
			Result[Key] = Spec
		else
			local Control = LiteralControl(Spec)
			if Control then
				Result[Key] = Control
			else
				warn(`control {Name}: UI Labs makes no control of a {typeof(Spec)}, so props.converted leaves it out`)
			end
		end
	end
	return Result
end

-- The listener's second argument: { __old, __new } per control of props.converted, groups nested,
-- as the plugin's CreateControlInfos makes it (Mounters/Generic/Utils.ts lines 4-26)
local function ControlInfos(Converted, Values, OldValues)
	local Result = {}
	for Key, Control in Converted do
		if Control.EntryType == "ControlGroup" then
			Result[Key] = ControlInfos(Control.Controls, Values[Key], OldValues[Key])
		else
			Result[Key] = { __new = Values[Key], __old = OldValues[Key] }
		end
	end
	return Result
end

-- The control definition at Path of a generic story's controls, or nil when the path names a group
-- or nothing. Any control can be set, whatever its value (a Choose of tables, an RGBA), as in the
-- plugin's controls panel.
local function GenericControlSpec(Specs, Path)
	local Spec = { EntryType = "ControlGroup", Controls = Specs }
	for _, Key in Path do
		if type(Spec) ~= "table" or Spec.EntryType ~= "ControlGroup" or type(Spec.Controls) ~= "table" then
			return nil
		end
		Spec = Spec.Controls[Key]
	end
	if type(Spec) == "table" and Spec.EntryType == "ControlGroup" then
		return nil
	end
	return Spec
end

-- One listener call, as the plugin makes each (RunListeners in Mounters/Generic/index.tsx lines
-- 45-61, through YCall in src/Utils/MiscUtils.ts lines 21-35): in a thread of its own under xpcall,
-- so a listener that errors or yields is reported with a warning and the next one still gets the
-- change; a yielded listener carries on when its thread resumes
local function CallListener(Listener, Values, Infos)
	local Failure
	local Thread = coroutine.create(function()
		local Ran, Message = xpcall(Listener, function(Error)
			return debug.traceback(tostring(Error), 2)
		end, Values, Infos)
		if not Ran then
			Failure = Message
		end
	end)
	local Resumed, Error = coroutine.resume(Thread)
	if coroutine.status(Thread) ~= "dead" then
		warn("a control listener of the generic story yielded; the listeners after it did not wait for it")
	elseif not Resumed or Failure ~= nil then
		warn(`a control listener of the generic story errored: {Failure or Error}`)
	end
end

-- A live control change of a generic story: a new values table (as UI Labs passes), then every
-- subscribed listener with the values and their infos
function Host.SetGenericControl(Controls, Specs, Path, Value, Strict)
	local Generic = Host.Generic[Controls]
	local OldValues = Generic.Values or Controls
	local Spec = GenericControlSpec(Specs, Path)
	if Spec == nil then
		if Strict then
			error(`no control at {table.concat(Path, ".")}`, 0)
		end
		return
	end
	local Values = table.clone(OldValues)
	local Parent = Values
	for Index = 1, #Path - 1 do
		Parent[Path[Index]] = table.clone(Parent[Path[Index]])
		Parent = Parent[Path[Index]]
	end
	Parent[Path[#Path]] = ControlValue(Spec, Value)
	Generic.Values = Values
	local Infos = ControlInfos(Generic.Converted, Values, OldValues)
	for _, Listener in table.clone(Generic.Listeners) do
		CallListener(Listener, Values, Infos)
	end
end

-- A session's control change of a generic story goes to SetGenericControl, any other to the
-- SetControl above
local SetSourceControl = Host.SetControl
function Host.SetControl(Controls, Specs, Path, Value, Strict)
	if Host.Generic[Controls] then
		Host.SetGenericControl(Controls, Specs, Path or {}, Value, Strict)
		return
	end
	SetSourceControl(Controls, Specs, Path, Value, Strict)
end

return Host
