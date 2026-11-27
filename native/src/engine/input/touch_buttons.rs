//! ContextActionService's touch buttons (`--device touch`): BindAction(name, handler, true, ...) shows a button the
//! way a 0.747 client's ContextActionTouch core script does, and a finger on it calls the action.
//!
//! Measured in 0.747 touch clients running the client's own core script (Studio 0.747.0.7471481, touches injected with
//! VirtualInputManager, each case in two sessions; notes/interaction.md "Touch buttons"):
//!   - The first BindAction that asks for a button makes `PlayerGui.ContextActionGui` (ScreenGui: ResetOnSpawn false,
//!     ZIndexBehavior Global, ScreenInsets CoreUISafeInsets) holding `ContextButtonFrame` (Frame at {0.7, 0, 0.5, 0},
//!     size {0.3, 0, 0.5, 0}, BackgroundTransparency 1); a BindAction without a button makes nothing. Both stay once
//!     every action is unbound.
//!   - Each button is an ImageButton `ContextActionButton` (BackgroundTransparency 1, image 97166444), 45 x 45, or 35 x
//!     35 while the screen below the top bar inset (GuiService:GetScreenResolution().Y) is 320 px or less, at the first
//!     free of seven slots, offsets in the frame (123, 70) (30, 60) (180, 160) (85, -25) (185, -25) (185, 260) (216, 65),
//!     added last to the frame. Its children: `ActionIcon` (ImageLabel {0.175, 0, 0.175, 0}, {0.65, 0, 0.65, 0},
//!     BackgroundTransparency 1, SetImage's image) and `ActionTitle` (TextLabel {1, 0, 1, 0}, BackgroundTransparency 1,
//!     SourceSansBold 18 white, TextStrokeTransparency 0, TextWrapped, SetTitle's text). An eighth gets none and warns.
//!   - All of it happens inside the call: BindAction, SetTitle, SetImage, SetPosition (moves the button; its slot stays
//!     taken), UnbindAction (the button goes and frees its slot), rebinding (the old button goes, a new one takes the
//!     first free slot, untitled), UnbindAllActions (every button goes). SetDescription changes nothing on screen.
//!   - A touch: InputBegan(Touch, Begin) on the button makes that touch the button's (a later finger takes it over),
//!     shows image 97166756 and calls the action with Begin; InputChanged of that touch calls Change; its InputEnded with
//!     state End (lifted on the button) calls End; lifted anywhere else, UserInputService.InputEnded calls Cancel. Both
//!     show image 97166444 again. A finger that leaves the button and comes back calls nothing until it moves on it. The
//!     handlers are connections made when the button is made (signals run the newest connection first).
//!   - Writing GuiService.TouchControlsEnabled or UserInputService.ModalEnabled shows (true / false) or hides the
//!     frame, whatever the other holds: the last change decides.
//! Not emulated: a key press, mouse button or gamepad connection hides the frame in the sim client, and a later touch
//! does not show it again there (its touches fire no LastInputTypeChanged), so what a phone does is unknown.

use std::rc::Rc;

use mlua::prelude::*;

use super::{Input, context_actions};
use super::super::{Engine, InstanceId, lua as engine_lua};

/// The core script's work, in Luau so its connections run among the scripts' in the engine's order.
const MODULE: &str = r#"
local Call, Warn, Small, Shown, Made = ...
local Players = game:GetService("Players")
local UserInputService = game:GetService("UserInputService")
local IMAGE = "https://www.roblox.com/asset/?id=97166444"
local PRESSED = "https://www.roblox.com/asset/?id=97166756"
local SLOTS = { { 123, 70 }, { 30, 60 }, { 180, 160 }, { 85, -25 }, { 185, -25 }, { 185, 260 }, { 216, 65 } }
local Gui, Frame
local Buttons = {}
local Taken = {}
local Module = {}

local function Container()
	if Gui and Gui.Parent and Frame.Parent == Gui then
		return Frame
	end
	local Player = Players.LocalPlayer
	local Ok, PlayerGui = pcall(function()
		return Player and (Player:FindFirstChildOfClass("PlayerGui") or Player.PlayerGui)
	end)
	if not Ok or not PlayerGui then
		return nil
	end
	Gui = Instance.new("ScreenGui")
	Gui.Name = "ContextActionGui"
	Gui.ResetOnSpawn = false
	Gui.ZIndexBehavior = Enum.ZIndexBehavior.Global
	Gui.ScreenInsets = Enum.ScreenInsets.CoreUISafeInsets
	Frame = Instance.new("Frame")
	Frame.Name = "ContextButtonFrame"
	Frame.BackgroundTransparency = 1
	Frame.Position = UDim2.new(0.7, 0, 0.5, 0)
	Frame.Size = UDim2.new(0.3, 0, 0.5, 0)
	Frame.Visible = Shown()
	Frame.Parent = Gui
	Gui.Parent = PlayerGui
	Made(Gui)
	return Frame
end

function Module.Bind(Name)
	local Parent = Container()
	if not Parent then
		return
	end
	local Slot
	for Index = 1, #SLOTS do
		if not Taken[Index] then
			Slot = Index
			break
		end
	end
	if not Slot then
		Warn("[Roblox][CoreGui.RobloxGui.CoreScripts/ContextActionTouch] Maximum of 7 allowed touch buttons, new touch button not created for " .. Name .. ".")
		return
	end
	local Size = if Small() then 35 else 45
	local Button = Instance.new("ImageButton")
	Button.Name = "ContextActionButton"
	Button.BackgroundTransparency = 1
	Button.Size = UDim2.new(0, Size, 0, Size)
	Button.Position = UDim2.new(0, SLOTS[Slot][1], 0, SLOTS[Slot][2])
	Button.Image = IMAGE
	local Icon = Instance.new("ImageLabel")
	Icon.Name = "ActionIcon"
	Icon.Position = UDim2.new(0.175, 0, 0.175, 0)
	Icon.Size = UDim2.new(0.65, 0, 0.65, 0)
	Icon.BackgroundTransparency = 1
	Icon.Parent = Button
	local Title = Instance.new("TextLabel")
	Title.Name = "ActionTitle"
	Title.Size = UDim2.new(1, 0, 1, 0)
	Title.BackgroundTransparency = 1
	Title.Font = Enum.Font.SourceSansBold
	Title.TextSize = 18
	Title.TextColor3 = Color3.new(1, 1, 1)
	Title.TextStrokeTransparency = 0
	Title.TextWrapped = true
	Title.Text = ""
	Title.Parent = Button
	local Record = { Button = Button, Icon = Icon, Title = Title, Slot = Slot }
	local function Release(State, Input)
		Record.Current = nil
		Button.Image = IMAGE
		Call(Name, State, Input)
	end
	Record.Connections = {
		Button.InputBegan:Connect(function(Input)
			if Input.UserInputType == Enum.UserInputType.Touch and Input.UserInputState == Enum.UserInputState.Begin then
				Record.Current = Input
				Button.Image = PRESSED
				Call(Name, "Begin", Input)
			end
		end),
		Button.InputChanged:Connect(function(Input)
			if Input == Record.Current then
				Call(Name, "Change", Input)
			end
		end),
		Button.InputEnded:Connect(function(Input)
			if Input == Record.Current and Input.UserInputState == Enum.UserInputState.End then
				Release("End", Input)
			end
		end),
		UserInputService.InputEnded:Connect(function(Input)
			if Input == Record.Current then
				Release("Cancel", Input)
			end
		end),
	}
	Taken[Slot] = Record
	Buttons[Name] = Record
	Button.Parent = Parent
end

function Module.Unbind(Name)
	local Record = Buttons[Name]
	if not Record then
		return
	end
	Buttons[Name] = nil
	Taken[Record.Slot] = nil
	for _, Connection in Record.Connections do
		Connection:Disconnect()
	end
	Record.Button:Destroy()
end

function Module.Clear()
	for Name in Buttons do
		Module.Unbind(Name)
	end
end

function Module.Title(Name, Text)
	local Record = Buttons[Name]
	if Record then
		Record.Title.Text = Text
	end
end

function Module.Image(Name, Image)
	local Record = Buttons[Name]
	if Record then
		Record.Icon.Image = Image
	end
end

function Module.Position(Name, Position)
	local Record = Buttons[Name]
	if Record then
		Record.Button.Position = Position
	end
end

function Module.Show(On)
	if Frame then
		Frame.Visible = On
	end
end

return Module
"#;

/// Whether this engine's ContextActionService shows touch buttons (a touch device, as a client starts
/// ContextActionTouch only where TouchScreenEnabled)
pub(super) fn enabled(engine: &Engine) -> bool {
    engine.options.profile.device.kind().touch()
}

/// The Luau half, made the first time an action asks for a button
fn module(lua: &Lua, engine: &Rc<Engine>, input: &Input) -> LuaResult<LuaTable> {
    if let Some(module) = input.actions.touch.borrow().as_ref() {
        return Ok(module.clone());
    }
    let weak = Rc::downgrade(engine);
    let call = lua.create_function(move |lua, (name, state, object): (String, String, LuaValue)| {
        let Some(engine) = weak.upgrade() else { return Ok(()) };
        let Some(input) = super::get(&engine) else { return Ok(()) };
        if let Some(handler) = input.actions.handler(&name) {
            context_actions::call(lua, &engine, &name, &handler, &state, object)?;
        }
        Ok(())
    })?;
    let weak = Rc::downgrade(engine);
    let warn = lua.create_function(move |_, message: String| {
        if let Some(engine) = weak.upgrade() {
            engine.warn(message);
        }
        Ok(())
    })?;
    // GuiService:GetScreenResolution().Y: the viewport below the top bar inset
    let weak = Rc::downgrade(engine);
    let small = lua.create_function(move |_, ()| {
        Ok(weak.upgrade().is_some_and(|engine| {
            let device = &engine.options.profile.device;
            device.viewport().1 - device.topbar_inset() <= 320.0
        }))
    })?;
    let weak = Rc::downgrade(engine);
    let shown = lua.create_function(move |_, ()| Ok(weak.upgrade().and_then(|engine| super::get(&engine)).is_none_or(|input| input.touch_controls_shown())))?;
    let weak = Rc::downgrade(engine);
    let made = lua.create_function(move |_, gui: LuaValue| {
        if let (Some(engine), Some(id)) = (weak.upgrade(), engine_lua::instance_id(&gui)) {
            if let Some(input) = super::get(&engine) {
                input.actions.touch_gui.set(Some(id));
            }
        }
        Ok(())
    })?;
    let shared = engine.lua.borrow().shared.clone();
    let mut chunk = lua.load(MODULE).set_name("=rur.contextactiontouch");
    if let Some(shared) = shared {
        chunk = chunk.set_environment(shared);
    }
    let module: LuaTable = chunk.call((call, warn, small, shown, made))?;
    *input.actions.touch.borrow_mut() = Some(module.clone());
    Ok(module)
}

/// The module's `name` function, if the module was made (nothing to update before the first button)
fn existing(input: &Input, name: &str) -> Option<LuaFunction> {
    input.actions.touch.borrow().as_ref().and_then(|module| module.get::<LuaFunction>(name).ok())
}

/// A button for the action just bound
pub(super) fn bind(lua: &Lua, engine: &Rc<Engine>, input: &Input, action: &str) -> LuaResult<()> {
    let bind: LuaFunction = module(lua, engine, input)?.get("Bind")?;
    bind.call(action)
}

/// The action's button goes (unbound, or bound again)
pub(super) fn unbind(input: &Input, action: &str) -> LuaResult<()> {
    match existing(input, "Unbind") {
        Some(unbind) => unbind.call(action),
        None => Ok(()),
    }
}

/// Every button goes (UnbindAllActions)
pub(super) fn clear(input: &Input) -> LuaResult<()> {
    match existing(input, "Clear") {
        Some(clear) => clear.call(()),
        None => Ok(()),
    }
}

/// SetTitle / SetImage / SetPosition on a bound action: its button follows
pub(super) fn update(input: &Input, which: &str, action: &str, value: LuaValue) -> LuaResult<()> {
    match existing(input, which) {
        Some(update) => update.call((action, value)),
        None => Ok(()),
    }
}

/// GuiService.TouchControlsEnabled or UserInputService.ModalEnabled changed: the frame is shown or hidden
pub(super) fn show(input: &Input) -> LuaResult<()> {
    match existing(input, "Show") {
        Some(show) => show.call(input.touch_controls_shown()),
        None => Ok(()),
    }
}

/// The ContextActionGui the buttons sit in, while it is in the data model (export.rs draws it with the story)
pub fn gui(engine: &Engine) -> Option<InstanceId> {
    let input = super::get(engine)?;
    let id = input.actions.touch_gui.get()?;
    let state = engine.state.borrow();
    state.dom.get(id)?;
    state.dom.at(id).parent?;
    Some(id)
}
