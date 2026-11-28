//! Roblox's own GUI on a playing client's screen: the top bar's menu button and Unibar, the player list, the health bar
//! and the flash of a hit, drawn over the game's GUI as a 0.747 client's core scripts draw them (roblox-sim clients
//! running `StarterScript`: desktop at device scales 1, 1.5, 2 and 3, touch at 1 and 3, gamepad at 1, each case in two
//! sessions; notes/interaction.md "Roblox's own GUI", docs/render.md "Roblox's own GUI").
//!
//! A play run of a client with a top-bar inset (the default in play mode: `--topbar-inset` 58) shows it; edit mode
//! (Studio's edit viewport), a server and `--topbar-inset 0` show none. It is two ScreenGuis outside the DataModel (a
//! client's game scripts cannot reach CoreGui either), named as the client names them: `TopBarApp` (DisplayOrder 6) and
//! `PlayerList` (DisplayOrder 1). They are laid out like any GUI, at no price to the game's scripts, and exported after
//! the run's own layers, in the CoreGui pass: over every PlayerGui ScreenGui whatever its DisplayOrder. They take no
//! input (in a client they do: the game's GUI under the menu button gets no hover or click, and a click opens Roblox's
//! menu; neither is emulated). Their icons and names are text in the BuilderIcons and BuilderSans fonts and the health
//! bar and the flash are the install's textures: without a Roblox install the shapes are drawn and those are not.
//!
//! What a client draws (GUI pixels, the same at every device scale):
//!   - The menu button (not on a gamepad): a disc of (18, 18, 21) at BackgroundTransparency 0.08, 44 x 44 at (16, 12),
//!     the 24 px BuilderIcons glyph "tilt" in (247, 247, 248) in its middle.
//!   - The Unibar (not on a gamepad): a pill of the same colour at (68, 12), 44 tall, 96 wide holding the
//!     "three-bars-horizontal" glyph (a 36 x 36 box at x 8) and chat's "speech-bubble-align-center" (at x 52: bold while
//!     chat's window shows, regular where it starts closed, a touch screen under 700 x 500); with
//!     chat off (SetCoreGuiEnabled Chat or All false) 52 wide with the first alone. Its right edge steps to the new width
//!     over the frames after the call ([`OFF_STEPS`], [`ON_STEPS`]); chat's glyph goes or comes the frame after it.
//!   - The player list (desktop only: a touch device or a gamepad shows none): at the screen's right edge less 170, 62
//!     down, 165 wide, boxes of (18, 18, 21) at BackgroundTransparency 0.3: an 18 px top with 7 px corners holding an "x"
//!     glyph, a 40 px row with the local player's DisplayName (BuilderSans Medium 15) at x 40, a 7 px bottom with 7 px
//!     corners. With leaderstats (an instance named `leaderstats` in the local player holding IntValues, NumberValues,
//!     StringValues or BoolValues) 77 px wider a stat, four at most, with a 20 px title row ("People" and the stats'
//!     names in BuilderSans Medium 12 of (188, 190, 200)) and each value centred in a 64 px column ([`stat_text`],
//!     [`stat_order`]). SetCoreGuiEnabled PlayerList (or All) false and SetCore("TopbarEnabled", false) hide it at once;
//!     shown again its first frame holds its top 24 px alone (two frames after TopbarEnabled), and SetCoreGuiEnabled
//!     slides it in from 28 px right ([`SLIDE`]); TopbarEnabled shows it in place.
//!   - The health bar (every device): while the local character's Humanoid has less than its MaxHealth (whatever its
//!     HealthDisplayType), rbxasset://textures/ui/TopBar/HealthBarBaseTV.png 125 x 20 at the right edge less
//!     141, 24 down, under HealthBarTV.png filled to the health's share in the health's colour ([`health_colour`]), both
//!     sliced (8, 8, 9, 9). SetCoreGuiEnabled Health (or All) false hides it.
//!   - The flash of a hit: a drop of the local character's Health of at least [`HURT_DROP`] of its MaxHealth shows
//!     rbxasset://textures/ui/TopBar/HurtOverlayAsset.png over the whole screen in (187, 0, 4), over the top bar
//!     ([`hurt_transparency`]); with Health off there is none.
//!   - The menu button and the Unibar stay whatever a script turns off (All included); SetCore("TopbarEnabled", false)
//!     hides the player list alone.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use rbx_dom_weak::types as dom;
use rustc_hash::FxHashMap;

use super::{
    ClassId, Engine, GuiEngine, InstanceId, NameId, Options, State,
    client_profile::DeviceKind,
    instance as core,
    lua::{Args, Flow, Ret, enum_item_value},
    services::Method,
    value::{FontValue, Str, Value},
};

/// The drawn pill's right edge, GUI px, the frames after a call turning chat off (the call's own frame first): measured
/// on rendered captures in two sessions, alike once aligned on the call
pub const OFF_STEPS: &[f64] = &[164.0, 138.0, 135.0, 128.0, 125.0, 123.0, 122.0, 121.0, 120.0];
/// ... and turning it on (from the call's frame; 164 from the 23rd frame on)
pub const ON_STEPS: &[f64] = &[
    120.0, 140.0, 140.0, 155.0, 158.0, 160.0, 161.0, 162.0, 163.0, 163.0, 163.0, 163.0, 163.0, 163.0, 163.0, 163.0, 163.0, 163.0, 163.0, 163.0,
    163.0, 163.0, 164.0,
];
/// The player list's offset to the right, GUI px, the frames after it was shown again (hidden in the call's frame)
pub const SLIDE: &[f64] = &[28.0, 19.0, 12.0, 7.0, 4.0, 2.0, 1.0, 1.0, 0.0];
/// The least drop of Health, a share of MaxHealth, that flashes (5 of 100 and 10 of 200 do, 4 of 100 and 9 of 200 do
/// not; two sessions alike)
pub const HURT_DROP: f64 = 0.05;

const BAR: (f32, f32, f32) = (18.0 / 255.0, 18.0 / 255.0, 21.0 / 255.0);
const GLYPH: (f32, f32, f32) = (247.0 / 255.0, 247.0 / 255.0, 248.0 / 255.0);
const HEADER: (f32, f32, f32) = (188.0 / 255.0, 190.0 / 255.0, 200.0 / 255.0);
const HURT: (f32, f32, f32) = (187.0 / 255.0, 0.0, 4.0 / 255.0);
const ICONS: &str = "rbxasset://LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons/BuilderIcons.json";
const SANS: &str = "rbxasset://fonts/families/BuilderSans.json";
const HEALTH_BASE: &str = "rbxasset://textures/ui/TopBar/HealthBarBaseTV.png";
const HEALTH_FILL: &str = "rbxasset://textures/ui/TopBar/HealthBarTV.png";
const HURT_IMAGE: &str = "rbxasset://textures/ui/TopBar/HurtOverlayAsset.png";

/// A core GUI switch as the game left it (StarterGui:SetCoreGuiEnabled, SetCore("TopbarEnabled")), with the frame of its
/// last change (None: before the first frame, or never changed)
#[derive(Clone, Copy)]
struct Switch {
    on: bool,
    since: Option<u64>,
}

impl Default for Switch {
    fn default() -> Self {
        Switch { on: true, since: None }
    }
}

impl Switch {
    fn set(&mut self, on: bool, frame: u64) {
        if self.on != on {
            self.on = on;
            self.since = (frame > 0).then_some(frame);
        }
    }

    /// Frames since the last change (None: settled from the start)
    fn age(&self, frame: u64) -> Option<u64> {
        self.since.map(|since| frame.saturating_sub(since))
    }
}

/// A hit's flash: the frame of the drop and the overlay's transparency then (1, or what an earlier flash left)
#[derive(Clone, Copy)]
struct Hit {
    frame: u64,
    from: f64,
}

/// The GUI's state in an engine (Gui::chrome)
#[derive(Default)]
pub struct Chrome {
    chat: Cell<Switch>,
    player_list: Cell<Switch>,
    health: Cell<Switch>,
    topbar: Cell<Switch>,
    /// Each Humanoid's Health at its last change (a drop is measured from it) and its last hit
    humanoids: RefCell<FxHashMap<InstanceId, (f64, Option<Hit>)>>,
    /// The last leaderstats names and the order the list shows them in ([`stat_order`])
    order: RefCell<(Vec<String>, Vec<usize>)>,
    tree: RefCell<Option<Tree>>,
}

/// The instances, made once ([`Tree::build`]); every export sets what the moment shows ([`update`])
struct Tree {
    bar: InstanceId,
    list: InstanceId,
    menu: InstanceId,
    unibar: InstanceId,
    chat_icon: InstanceId,
    health: InstanceId,
    fill: InstanceId,
    hurt: InstanceId,
    holder: InstanceId,
    title: InstanceId,
    people: InstanceId,
    body: InstanceId,
    bottom: InstanceId,
    name: InstanceId,
    headers: Vec<InstanceId>,
    cells: Vec<InstanceId>,
}

/// Whether this run shows Roblox's GUI: a playing client with a top-bar inset
pub fn shown(engine: &Engine) -> bool {
    shows(&engine.options)
}

fn shows(options: &Options) -> bool {
    !options.edit_mode() && !options.server() && options.profile.device.topbar_inset() > 0.0
}

/// Where the top bar's band right of Roblox's buttons starts (GuiService.TopbarInset's Min.X, where a TopbarSafeInsets
/// ScreenGui is laid out): 164, 120 with chat off, 0 on a gamepad (no buttons); None without the bar. A client moves it
/// over a few frames after chat changes (ree's TopbarInset steps); the layout takes the new place at once.
pub fn band_start(options: &Options, chat: bool) -> Option<f64> {
    if !shows(options) {
        None
    } else if options.profile.device.kind() == DeviceKind::Gamepad {
        Some(0.0)
    } else {
        Some(if chat { 164.0 } else { 120.0 })
    }
}

/// Mirrors StarterGui's switches (what the engine's own methods keep, asked after each call) into [`Chrome`], and notes
/// the local character's hits.
pub fn install(engine: &Rc<Engine>) {
    let reflection = engine.reflection.clone();
    let (Some(starter), Some(set_enabled), Some(get_enabled), Some(set_core)) = (
        reflection.class_id("StarterGui"),
        reflection.name_id("SetCoreGuiEnabled"),
        reflection.name_id("GetCoreGuiEnabled"),
        reflection.name_id("SetCore"),
    ) else {
        return;
    };
    {
        let mut behaviors = engine.behaviors.borrow_mut();
        let previous_set = behaviors.find_method(&reflection, starter, set_enabled).map(|(_, m)| m.clone());
        let previous_get = behaviors.find_method(&reflection, starter, get_enabled).map(|(_, m)| m.clone());
        let previous_core = behaviors.find_method(&reflection, starter, set_core).map(|(_, m)| m.clone());
        if let (Some(previous_set), Some(previous_get)) = (previous_set, previous_get) {
            behaviors.add_method(starter, set_enabled, Rc::new(move |lua, engine, id, args| {
                let result = call(&previous_set, lua, engine, id, args)?;
                if !engine.options.server() {
                    let frame = engine.state.borrow().sched.frame;
                    let chrome = &engine.gui().chrome;
                    for (name, switch) in [("Chat", &chrome.chat), ("PlayerList", &chrome.player_list), ("Health", &chrome.health)] {
                        let Some(item) = engine.reflection.enum_item("CoreGuiType", name) else { continue };
                        let this = args.0.front().cloned().unwrap_or(mlua::Value::Nil);
                        let asked = Args::new(mlua::MultiValue::from_vec(vec![this, enum_item_value(lua, engine, item)?]));
                        let on = !matches!(call(&previous_get, lua, engine, id, &asked)?.front(), Some(mlua::Value::Boolean(false)));
                        let mut value = switch.get();
                        value.set(on, frame);
                        switch.set(value);
                    }
                    // (the band a TopbarSafeInsets ScreenGui is laid out in follows chat)
                    let start = band_start(&engine.options, chrome.chat.get().on);
                    if let Ok(mut guard) = engine.layout().try_borrow_mut() {
                        if let Some(layout) = guard.as_mut().filter(|layout| layout.topbar_start != start) {
                            layout.topbar_start = start;
                            layout.invalidate();
                        }
                    }
                }
                Ok(result)
            }));
        }
        if let Some(previous_core) = previous_core {
            behaviors.add_method(starter, set_core, Rc::new(move |lua, engine, id, args| {
                let result = call(&previous_core, lua, engine, id, args)?;
                let name = args.0.get(1).and_then(|value| value.as_string()).map(|text| text.to_string_lossy());
                if !engine.options.server() && name.as_deref() == Some("TopbarEnabled") {
                    let on = !matches!(args.0.get(2), Some(mlua::Value::Boolean(false)) | Some(mlua::Value::Nil) | None);
                    let frame = engine.state.borrow().sched.frame;
                    let mut value = engine.gui().chrome.topbar.get();
                    value.set(on, frame);
                    engine.gui().chrome.topbar.set(value);
                }
                Ok(result)
            }));
        }
    }
    let (Some(health), Some(max_health), Some(humanoid)) = (reflection.name_id("Health"), reflection.name_id("MaxHealth"), reflection.class_id("Humanoid")) else {
        return;
    };
    let weak: Weak<Engine> = Rc::downgrade(engine);
    engine.hooks.borrow_mut().property_changed.push(Box::new(move |state: &mut State, id, name| {
        if name != health || state.dom.at(id).class != humanoid {
            return;
        }
        let Some(engine) = weak.upgrade() else { return };
        if !shown(&engine) {
            return;
        }
        let number = |property| state.dom.raw_get(id, property).and_then(Value::as_number);
        let (now, max) = (number(health).unwrap_or(100.0), number(max_health).unwrap_or(100.0));
        let Ok(mut humanoids) = engine.gui().chrome.humanoids.try_borrow_mut() else { return };
        let (before, hit) = humanoids.get(&id).copied().unwrap_or((max, None));
        let frame = state.sched.frame;
        let hit = if before - now >= HURT_DROP * max - 1e-9 && before > now {
            let from = hit.map_or(1.0, |hit| hurt_transparency(hit, frame, state.sched.frame_duration).unwrap_or(1.0));
            Some(Hit { frame, from })
        } else {
            hit
        };
        humanoids.insert(id, (now, hit));
    }));
}

fn call(method: &Method, lua: &mlua::Lua, engine: &Rc<Engine>, id: InstanceId, args: &Args) -> Ret {
    match method {
        Method::Rust(f) => f(lua, engine, id, args),
        Method::Lua(f) => f.call::<mlua::MultiValue>(args.0.clone()).map_err(Flow::from),
    }
}

/// Whether `id` is one of the ScreenGuis of Roblox's GUI
pub fn is_root(engine: &Engine, id: InstanceId) -> bool {
    engine.gui().chrome.tree.try_borrow().is_ok_and(|tree| tree.as_ref().is_some_and(|tree| tree.bar == id || tree.list == id))
}

/// The ScreenGuis to draw after the run's own: none unless [`shown`]; made the first time, and set to what the moment
/// shows (laid out here, at no price to the game's scripts)
pub fn roots(engine: &Engine) -> Vec<InstanceId> {
    if !shown(engine) || engine.reflection.class_id("ScreenGui").is_none() {
        return Vec::new();
    }
    let chrome = &engine.gui().chrome;
    let Ok(mut tree) = chrome.tree.try_borrow_mut() else { return Vec::new() };
    if tree.is_none() {
        *tree = Tree::build(engine);
    }
    let Some(tree) = tree.as_ref() else { return Vec::new() };
    update(engine, chrome, tree);
    for root in [tree.list, tree.bar] {
        super::layout::ensure_uncharged(engine, root);
    }
    vec![tree.list, tree.bar]
}

/// Whether the GUI moves this frame on its own (the Unibar or the player list moving, a flash showing; their last frame
/// included): a session exports such a frame though nothing of the game's changed
pub fn animating(engine: &Engine) -> bool {
    if !shown(engine) {
        return false;
    }
    let chrome = &engine.gui().chrome;
    let (frame, frame_duration) = {
        let state = engine.state.borrow();
        (state.sched.frame, state.sched.frame_duration)
    };
    let within = |switch: Switch, frames: usize| switch.age(frame).is_some_and(|age| (age as usize) <= frames);
    if within(chrome.chat.get(), ON_STEPS.len().max(OFF_STEPS.len())) || within(chrome.player_list.get(), SLIDE.len() + 1) || within(chrome.topbar.get(), SLIDE.len() + 1) {
        return true;
    }
    let Ok(humanoids) = chrome.humanoids.try_borrow() else { return true };
    humanoids.values().any(|&(_, hit)| hit.is_some_and(|hit| hurt_transparency(hit, frame.saturating_sub(1), frame_duration).is_some()))
}

// ---------------------------------------------------------------------------------------------------------------
// What the moment shows

/// A stat the player list shows: its name and its value's text
#[derive(Clone, Debug, PartialEq)]
pub struct Stat {
    pub name: String,
    pub text: String,
}

fn update(engine: &Engine, chrome: &Chrome, tree: &Tree) {
    let (frame, frame_duration) = {
        let state = engine.state.borrow();
        (state.sched.frame, state.sched.frame_duration)
    };
    let w = Writer { engine };
    let kind = engine.options.profile.device.kind();
    let (touch, gamepad) = (kind.touch(), kind == DeviceKind::Gamepad);
    // The menu button and the Unibar: chat's glyph and the pill's width
    w.set(tree.menu, "Visible", Value::Bool(!gamepad));
    w.set(tree.unibar, "Visible", Value::Bool(!gamepad));
    let chat = chrome.chat.get();
    let steps = if chat.on { ON_STEPS } else { OFF_STEPS };
    let edge = match chat.age(frame) {
        Some(age) => steps[(age as usize).min(steps.len() - 1)],
        None => steps[steps.len() - 1],
    };
    let glyph = match chat.age(frame) {
        Some(0) => !chat.on,
        _ => chat.on,
    };
    w.set(tree.unibar, "Size", udim2(0.0, edge - 68.0, 0.0, 44.0));
    w.set(tree.chat_icon, "Visible", Value::Bool(glyph));
    // (chat's glyph is bold while its window shows: closed from the start on a touch screen under 700 x 500)
    let (width, height) = engine.options.profile.device.viewport();
    let closed = touch && (width < 700.0 || height < 500.0);
    w.set(tree.chat_icon, "FontFace", Builder { engine }.font(ICONS, if closed { "Regular" } else { "Bold" }));
    // The player list
    let (list, topbar) = (chrome.player_list.get(), chrome.topbar.get());
    // (shown the frame after a switch turned it on, its first frame only its top 24 px (TopbarEnabled: two frames);
    // SetCoreGuiEnabled slides it in, TopbarEnabled shows it in place)
    let slide = if touch || gamepad || !list.on || !topbar.on {
        None
    } else {
        let offset = |age: Option<u64>| match age {
            Some(0) => None,
            Some(age) => Some((SLIDE[(age as usize - 1).min(SLIDE.len() - 1)], age == 1)),
            None => Some((0.0, false)),
        };
        match (offset(list.age(frame)), topbar.age(frame)) {
            (None, _) | (_, Some(0)) => None,
            (Some((offset, _)), Some(1 | 2)) => Some((offset, true)),
            (shown, _) => shown,
        }
    };
    w.set(tree.list, "Enabled", Value::Bool(slide.is_some()));
    if let Some((offset, first)) = slide {
        w.set(tree.holder, "ClipsDescendants", Value::Bool(true));
        let stats = leaderstats(engine, chrome);
        let n = stats.len() as f64;
        let width = 165.0 + 77.0 * n;
        w.set(tree.holder, "Position", udim2(1.0, -4.0 - 166.0 - 77.0 * n + offset, 0.0, 62.0));
        w.set(tree.holder, "Size", udim2(0.0, width, 0.0, if first { 24.0 } else { 200.0 }));
        w.set(tree.title, "Visible", Value::Bool(!stats.is_empty()));
        let body = if stats.is_empty() { 18.0 } else { 38.0 };
        w.set(tree.body, "Position", udim2(0.0, 0.0, 0.0, body));
        w.set(tree.bottom, "Position", udim2(0.0, 0.0, 0.0, body + 40.0));
        let header = 150.0 + 11.0 * n;
        w.set(tree.people, "Size", udim2(0.0, header, 1.0, 0.0));
        w.set(tree.name, "Size", udim2(0.0, header - 46.0, 1.0, 0.0));
        w.set(tree.name, "Text", Value::String(Str::from(display_name(engine).as_str())));
        for index in 0..tree.headers.len() {
            let stat = stats.get(index);
            let x = header + 64.0 * index as f64;
            // (the title row's texts sit 2 px up in it)
            for (id, y, text) in [(tree.headers[index], -2.0, stat.map(|s| s.name.as_str())), (tree.cells[index], 0.0, stat.map(|s| s.text.as_str()))] {
                w.set(id, "Visible", Value::Bool(stat.is_some()));
                w.set(id, "Position", udim2(0.0, x, 0.0, y));
                w.set(id, "Text", Value::String(Str::from(text.unwrap_or(""))));
            }
        }
    }
    // The health bar and the flash of a hit
    let health = chrome.health.get();
    let humanoid = local_humanoid(engine);
    let share = if health.on { humanoid.and_then(|humanoid| hurt_share(engine, humanoid)) } else { None };
    w.set(tree.health, "Visible", Value::Bool(share.is_some()));
    if let Some(share) = share {
        w.set(tree.fill, "Size", Value::UDim2(dom::UDim2::new(dom::UDim::new(share, 0), dom::UDim::new(1.0, 0))));
        let (r, g, b) = health_colour(share);
        w.set(tree.fill, "ImageColor3", Value::Color3(dom::Color3::new(r, g, b)));
    }
    let hit = humanoid.filter(|_| health.on).and_then(|humanoid| chrome.humanoids.borrow().get(&humanoid).and_then(|&(_, hit)| hit));
    let flash = hit.and_then(|hit| hurt_transparency(hit, frame, frame_duration));
    w.set(tree.hurt, "Visible", Value::Bool(flash.is_some()));
    if let Some(transparency) = flash {
        w.set(tree.hurt, "ImageTransparency", Value::Number(transparency));
    }
}

/// The flash's ImageTransparency `frame` frames into a hit (None once it is over): from where it was to 0 over 0.1 s,
/// held two frames more (to 2/15 s), then back to 1 over 0.5 s, linearly (rendered captures frame by frame: two
/// sessions identical, and a second hit during the first's fade starting from where that left it)
fn hurt_transparency(hit: Hit, frame: u64, frame_duration: f64) -> Option<f64> {
    const HELD: f64 = 2.0 / 15.0;
    let t = frame.saturating_sub(hit.frame) as f64 * if frame_duration > 0.0 { frame_duration } else { 1.0 / 60.0 };
    if t < 0.1 {
        Some(hit.from * (1.0 - t / 0.1))
    } else if t < HELD {
        Some(0.0)
    } else if t < HELD + 0.5 - 1e-9 {
        Some((t - HELD) / 0.5)
    } else {
        None
    }
}

/// The local player's DisplayName (its Name when it has none)
fn display_name(engine: &Engine) -> String {
    let Some(player) = engine.world().local_player else { return String::new() };
    let state = engine.state.borrow();
    let read = |name: &str| {
        engine.reflection.name_id(name).and_then(|id| state.dom.raw_get(player, id)).and_then(|value| match value {
            Value::String(text) => Some(text.to_str_lossy().into_owned()),
            _ => None,
        })
    };
    read("DisplayName").filter(|text| !text.is_empty()).or_else(|| read("Name")).unwrap_or_default()
}

/// The Humanoid of the local player's Character
fn local_humanoid(engine: &Engine) -> Option<InstanceId> {
    let player = engine.world().local_player?;
    let state = engine.state.borrow();
    let reflection = &engine.reflection;
    let character = match state.dom.raw_get(player, reflection.name_id("Character")?) {
        Some(Value::Instance(id)) if state.dom.get(*id).is_some() => *id,
        _ => return None,
    };
    state.dom.find_child_of_class(character, reflection.class_id("Humanoid")?)
}

/// The health share of the local character's Humanoid while the bar shows: below MaxHealth, whatever its
/// HealthDisplayType (AlwaysOn at full health shows none, AlwaysOff below shows it), dead too (empty)
fn hurt_share(engine: &Engine, humanoid: InstanceId) -> Option<f32> {
    let state = engine.state.borrow();
    let reflection = &engine.reflection;
    let number = |name: &str| reflection.name_id(name).and_then(|id| state.dom.raw_get(humanoid, id)).and_then(Value::as_number);
    let (health, max) = (number("Health").unwrap_or(100.0), number("MaxHealth").unwrap_or(100.0));
    (health < max && max > 0.0).then(|| (health / max).clamp(0.0, 1.0) as f32)
}

/// The fill's colour at a health share: green (27, 252, 107) from 80 %, red (255, 28, 0) to 10 %, between them each
/// channel weighed by the inverse square of the share's distance from red at 10 %, yellow (250, 235, 0) at 50 % and
/// green at 80 % (Shepard's interpolation; measured at 2 % steps and nine more shares, each channel to the level)
pub fn health_colour(share: f32) -> (f32, f32, f32) {
    const POINTS: [(f32, [f32; 3]); 3] = [(0.1, [255.0, 28.0, 0.0]), (0.5, [250.0, 235.0, 0.0]), (0.8, [27.0, 252.0, 107.0])];
    let rgb = if share >= 0.8 {
        POINTS[2].1
    } else if share <= 0.1 {
        POINTS[0].1
    } else if let Some(&(_, at)) = POINTS.iter().find(|(point, _)| (share - point).abs() < 1e-6) {
        at
    } else {
        let mut sum = [0.0f32; 3];
        let mut weights = 0.0;
        for (point, colour) in POINTS {
            let weight = 1.0 / ((share - point) * (share - point));
            weights += weight;
            for channel in 0..3 {
                sum[channel] += weight * colour[channel];
            }
        }
        sum.map(|value| value / weights)
    };
    (rgb[0] / 255.0, rgb[1] / 255.0, rgb[2] / 255.0)
}

/// The local player's leaderstats as the list shows them: its IntValues, NumberValues, StringValues and BoolValues
/// ([`stat_text`]), in [`stat_order`], four at most
fn leaderstats(engine: &Engine, chrome: &Chrome) -> Vec<Stat> {
    let Some(player) = engine.world().local_player else { return Vec::new() };
    let mut stats = Vec::new();
    {
        let state = engine.state.borrow();
        let reflection = &engine.reflection;
        let Some(folder) = state.dom.find_child(player, b"leaderstats") else { return Vec::new() };
        let value_name = reflection.name_id("Value");
        for &child in state.dom.at(folder).children.iter() {
            let value = value_name.and_then(|id| state.dom.raw_get(child, id));
            let text = match (reflection.class(state.dom.at(child).class).name.as_ref(), value) {
                ("IntValue" | "NumberValue", Some(Value::Number(n))) => stat_text(*n),
                ("IntValue" | "NumberValue", None) => stat_text(0.0),
                ("StringValue", Some(Value::String(text))) => text.to_str_lossy().into_owned(),
                ("StringValue", None) => String::new(),
                ("BoolValue", Some(Value::Bool(true))) => "1".into(),
                ("BoolValue", _) => "0".into(),
                _ => continue,
            };
            stats.push(Stat { name: state.dom.at(child).name.to_str_lossy().into_owned(), text });
        }
    }
    let names: Vec<String> = stats.iter().map(|stat| stat.name.clone()).collect();
    let order = {
        let mut cached = chrome.order.borrow_mut();
        if cached.0 != names {
            *cached = (names.clone(), stat_order(&names));
        }
        cached.1.clone()
    };
    let mut ordered: Vec<Stat> = order.into_iter().filter_map(|index| stats.get(index).cloned()).collect();
    ordered.truncate(4);
    ordered
}

/// The columns' order: the order Luau's `pairs` meets the names in a table keyed by them, filled in the leaderstats'
/// child order. A client's order depends on the names alone, not on the order they came in; this rule gives its order
/// for 13 of 20 measured sets (every set of one stat; of two and three, all but two where two names share a hash slot),
/// and not for sets of four or more.
pub fn stat_order(names: &[String]) -> Vec<usize> {
    let fallback = || (0..names.len()).collect();
    let lua = mlua::Lua::new();
    let Ok(table) = lua.create_table() else { return fallback() };
    for (index, name) in names.iter().enumerate() {
        if table.raw_set(name.as_str(), index).is_err() {
            return fallback();
        }
    }
    let mut order = Vec::with_capacity(names.len());
    for pair in table.pairs::<mlua::Value, usize>() {
        match pair {
            Ok((_, index)) => order.push(index),
            Err(_) => return fallback(),
        }
    }
    // (a repeated name keeps its last child)
    if order.len() < names.len() {
        order.sort_unstable();
    }
    order
}

/// A number stat's text (IntValues and NumberValues alike):
///   - a whole number below a million with thousands separated by commas (-12,345);
///   - a whole number from a million in M, from a billion in B ([`short`]: 1.2M, 12.3M, 999M, 1,000B);
///   - another number (or a whole one past 64 bits) as Luau writes it when that has at most five digits (1.005, 99.999,
///     1,234.5, 0.0001, 1e-07, 1e+21), else
///     short: in K from a thousand, M from a million, B from a billion, and below a thousand bare (12.3K, 1.2, 123, 0.1);
///   - nan and inf as Luau writes them.
pub fn stat_text(value: f64) -> String {
    if !value.is_finite() {
        return luau_number(value);
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let magnitude = value.abs();
    let abbreviated = |magnitude: f64| {
        if magnitude >= 1e9 {
            format!("{}B", short(magnitude / 1e9))
        } else if magnitude >= 1e6 {
            format!("{}M", short(magnitude / 1e6))
        } else if magnitude >= 1e3 {
            format!("{}K", short(magnitude / 1e3))
        } else {
            short(magnitude)
        }
    };
    // (whole numbers past what a 64-bit integer holds are written as Luau writes them: 1e+21)
    if value.fract() == 0.0 && magnitude < 9.2e18 {
        if magnitude < 1e6 {
            return format!("{sign}{}", grouped(magnitude as u64));
        }
        return format!("{sign}{}", abbreviated(magnitude));
    }
    let written = luau_number(value);
    if written.chars().filter(char::is_ascii_digit).count() <= 5 {
        // (thousands grouped in the whole part of a plain number)
        if written.contains('e') {
            return written;
        }
        let (whole, rest) = written.trim_start_matches('-').split_once('.').unwrap_or((written.trim_start_matches('-'), ""));
        let whole = whole.parse::<u64>().map_or_else(|_| whole.to_string(), grouped);
        return if rest.is_empty() { format!("{sign}{whole}") } else { format!("{sign}{whole}.{rest}") };
    }
    format!("{sign}{}", abbreviated(magnitude))
}

/// One decimal, cut (not rounded), dropped when it is 0 or the whole part has three digits or more; the whole part
/// grouped
fn short(value: f64) -> String {
    let tenths = (value * 10.0 + 1e-9).floor() as u64;
    let (whole, tenth) = (tenths / 10, tenths % 10);
    if tenth == 0 || whole >= 100 { grouped(whole) } else { format!("{}.{tenth}", grouped(whole)) }
}

/// Thousands separated by commas
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A number as Luau's tostring writes it: the shortest digits that read back the same, in plain notation from 1e-5 up
/// to 1e15 and as `de±XX` beyond (1e-07, 1e+21); nan, inf, -inf
pub fn luau_number(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    if (-5..15).contains(&exponent) {
        let plain = format!("{value}");
        return plain;
    }
    format!("{mantissa}e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
}

// ---------------------------------------------------------------------------------------------------------------
// The tree

struct Writer<'a> {
    engine: &'a Engine,
}

impl Writer<'_> {
    /// A property write with the engine's notifications (layout), when the value changes
    fn set(&self, id: InstanceId, property: &str, value: Value) {
        let Some(name) = self.engine.reflection.name_id(property) else { return };
        if self.engine.state.borrow().dom.raw_get(id, name).is_some_and(|old| old.same(&value)) {
            return;
        }
        super::lua::instance::set_property(self.engine, id, name, value);
    }
}

fn udim2(sx: f64, ox: f64, sy: f64, oy: f64) -> Value {
    Value::UDim2(dom::UDim2::new(dom::UDim::new(sx as f32, ox.round() as i32), dom::UDim::new(sy as f32, oy.round() as i32)))
}

fn colour((r, g, b): (f32, f32, f32)) -> Value {
    Value::Color3(dom::Color3::new(r, g, b))
}

struct Builder<'a> {
    engine: &'a Engine,
}

impl Builder<'_> {
    fn item(&self, enum_name: &str, name: &str) -> Value {
        self.engine.reflection.enum_item(enum_name, name).map_or(Value::Nil, Value::EnumItem)
    }

    fn font(&self, family: &str, weight: &str) -> Value {
        match (self.engine.reflection.enum_item("FontWeight", weight), self.engine.reflection.enum_item("FontStyle", "Normal")) {
            (Some(weight), Some(style)) => Value::Font(Rc::new(FontValue { family: Str::from(family), weight, style })),
            _ => Value::Nil,
        }
    }

    /// An instance of `class` named `name` with `properties`, in `parent` (the engine's notifications run for the move);
    /// one without a parent is pinned (kept while the engine holds it outside the tree)
    fn make(&self, class: &str, name: &str, parent: Option<InstanceId>, properties: Vec<(&str, Value)>) -> Option<InstanceId> {
        let engine = self.engine;
        let class: ClassId = engine.reflection.class_id(class)?;
        let id = {
            let mut state = engine.state.borrow_mut();
            let id = core::create_named(&mut state, &engine.reflection, class, name);
            for (property, value) in properties {
                if value.is_nil() {
                    continue;
                }
                let Some(property): Option<NameId> = engine.reflection.name_id(property) else { continue };
                core::raw_set_silent(&mut state, &engine.reflection, id, property, value);
            }
            id
        };
        match parent {
            Some(parent) => {
                let hooks = engine.hooks.borrow();
                let mut state = engine.state.borrow_mut();
                core::set_parent(&mut state, &engine.reflection, &hooks, id, Some(parent)).ok()?;
            }
            None => engine.state.borrow_mut().dom.pin(id),
        }
        Some(id)
    }

    /// A UICorner of `scale` / `offset` at every corner (the four radii a script's CornerRadius writes)
    fn corner(&self, parent: InstanceId, scale: f64, offset: f64) -> Option<InstanceId> {
        let radius = || Value::UDim(dom::UDim::new(scale as f32, offset as i32));
        self.make("UICorner", "UICorner", Some(parent), vec![
            ("TopLeftRadius", radius()),
            ("TopRightRadius", radius()),
            ("BottomRightRadius", radius()),
            ("BottomLeftRadius", radius()),
        ])
    }

    /// A frame of the bar's colour at `transparency`
    fn panel(&self, name: &str, parent: InstanceId, position: Value, size: Value, transparency: f64) -> Option<InstanceId> {
        self.make("Frame", name, Some(parent), vec![
            ("Position", position),
            ("Size", size),
            ("BackgroundColor3", colour(BAR)),
            ("BackgroundTransparency", Value::Number(transparency)),
            ("BorderSizePixel", Value::Number(0.0)),
        ])
    }

    /// A text of `font` at `size` in `colour3`, centred in its box unless `left`
    #[allow(clippy::too_many_arguments)]
    fn label(&self, name: &str, parent: InstanceId, position: Value, size: Value, text: &str, font: Value, text_size: f64, colour3: (f32, f32, f32), left: bool) -> Option<InstanceId> {
        self.make("TextLabel", name, Some(parent), vec![
            ("Position", position),
            ("Size", size),
            ("BackgroundTransparency", Value::Number(1.0)),
            ("BorderSizePixel", Value::Number(0.0)),
            ("Text", Value::String(Str::from(text))),
            ("FontFace", font),
            ("TextSize", Value::Number(text_size)),
            ("TextColor3", colour(colour3)),
            ("TextXAlignment", if left { self.item("TextXAlignment", "Left") } else { Value::Nil }),
        ])
    }

    /// An image over its box, sliced (8, 8, 9, 9) when `sliced`
    fn image(&self, name: &str, parent: InstanceId, position: Value, size: Value, image: &str, sliced: bool) -> Option<InstanceId> {
        self.make("ImageLabel", name, Some(parent), vec![
            ("Position", position),
            ("Size", size),
            ("BackgroundTransparency", Value::Number(1.0)),
            ("BorderSizePixel", Value::Number(0.0)),
            ("Image", Value::String(Str::from(image))),
            ("ScaleType", if sliced { self.item("ScaleType", "Slice") } else { Value::Nil }),
            ("SliceCenter", if sliced { Value::Rect(dom::Rect::new(dom::Vector2::new(8.0, 8.0), dom::Vector2::new(9.0, 9.0))) } else { Value::Nil }),
        ])
    }

    fn screen(&self, name: &str, order: f64) -> Option<InstanceId> {
        self.make("ScreenGui", name, None, vec![
            ("DisplayOrder", Value::Number(order)),
            ("ScreenInsets", self.item("ScreenInsets", "DeviceSafeInsets")),
            ("ZIndexBehavior", self.item("ZIndexBehavior", "Sibling")),
            ("ResetOnSpawn", Value::Bool(false)),
        ])
    }
}

impl Tree {
    fn build(engine: &Engine) -> Option<Tree> {
        let b = Builder { engine };
        let icons = |weight| b.font(ICONS, weight);
        let sans = || b.font(SANS, "Medium");
        // The top bar, the health bar, the flash (last: over the rest)
        let bar = b.screen("TopBarApp", 6.0)?;
        let menu = b.panel("MenuIcon", bar, udim2(0.0, 16.0, 0.0, 12.0), udim2(0.0, 44.0, 0.0, 44.0), 0.08)?;
        b.corner(menu, 1.0, 0.0)?;
        b.label("Icon", menu, udim2(0.0, 10.0, 0.0, 10.0), udim2(0.0, 24.0, 0.0, 24.0), "tilt", icons("Regular"), 24.0, GLYPH, false)?;
        let unibar = b.panel("Unibar", bar, udim2(0.0, 68.0, 0.0, 12.0), udim2(0.0, 96.0, 0.0, 44.0), 0.08)?;
        b.corner(unibar, 1.0, 0.0)?;
        b.label("Menu", unibar, udim2(0.0, 8.0, 0.0, 4.0), udim2(0.0, 36.0, 0.0, 36.0), "three-bars-horizontal", icons("Regular"), 24.0, GLYPH, false)?;
        let chat_icon = b.label("Chat", unibar, udim2(0.0, 52.0, 0.0, 4.0), udim2(0.0, 36.0, 0.0, 36.0), "speech-bubble-align-center", icons("Bold"), 24.0, GLYPH, false)?;
        let health = b.image("HealthBar", bar, udim2(1.0, -141.0, 0.0, 24.0), udim2(0.0, 125.0, 0.0, 20.0), HEALTH_BASE, true)?;
        let fill = b.image("Fill", health, udim2(0.0, 0.0, 0.0, 0.0), udim2(1.0, 0.0, 1.0, 0.0), HEALTH_FILL, true)?;
        let hurt = b.image("HurtOverlay", bar, udim2(0.0, 0.0, 0.0, 0.0), udim2(1.0, 0.0, 1.0, 0.0), HURT_IMAGE, false)?;
        b.set_now(hurt, "ImageColor3", colour(HURT));
        b.set_now(hurt, "Visible", Value::Bool(false));
        // The player list: a top of 18 px cut from a 36 px box with rounded corners, the title row, the player's row, a
        // bottom of 7 px cut from a 14 px box
        let list = b.screen("PlayerList", 1.0)?;
        let holder = b.make("Frame", "Children", Some(list), vec![
            ("Position", udim2(1.0, -170.0, 0.0, 62.0)),
            ("Size", udim2(0.0, 165.0, 0.0, 200.0)),
            ("BackgroundTransparency", Value::Number(1.0)),
        ])?;
        let clip = |name: &str, y: f64, height: f64| {
            b.make("Frame", name, Some(holder), vec![
                ("Position", udim2(0.0, 0.0, 0.0, y)),
                ("Size", udim2(1.0, 0.0, 0.0, height)),
                ("BackgroundTransparency", Value::Number(1.0)),
                ("ClipsDescendants", Value::Bool(true)),
            ])
        };
        let top = clip("TopRoundedRect", 0.0, 18.0)?;
        let top_box = b.panel("DismissIconFrame", top, udim2(0.0, 0.0, 0.0, 0.0), udim2(1.0, 0.0, 0.0, 36.0), 0.3)?;
        b.corner(top_box, 0.0, 7.0)?;
        b.label("DismissButton", top_box, udim2(0.0, 3.0, 0.0, 3.0), udim2(0.0, 16.0, 0.0, 16.0), "x", icons("Regular"), 16.0, GLYPH, false)?;
        let title = b.panel("TitleBar", holder, udim2(0.0, 0.0, 0.0, 18.0), udim2(1.0, 0.0, 0.0, 20.0), 0.3)?;
        let people = b.label("playersHeader", title, udim2(0.0, 0.0, 0.0, -2.0), udim2(0.0, 161.0, 1.0, 0.0), "People", sans(), 12.0, HEADER, true)?;
        b.make("UIPadding", "Padding", Some(people), vec![("PaddingLeft", Value::UDim(dom::UDim::new(0.0, 15)))])?;
        let body = b.panel("Body", holder, udim2(0.0, 0.0, 0.0, 18.0), udim2(1.0, 0.0, 0.0, 40.0), 0.3)?;
        let name = b.label("PlayerName", body, udim2(0.0, 40.0, 0.0, 0.0), udim2(0.0, 104.0, 1.0, 0.0), "", sans(), 15.0, GLYPH, true)?;
        b.set_now(name, "TextTruncate", b.item("TextTruncate", "AtEnd"));
        let (mut headers, mut cells) = (Vec::new(), Vec::new());
        for index in 0..4 {
            let header = b.label(&format!("stat_{index}"), title, udim2(0.0, 0.0, 0.0, -2.0), udim2(0.0, 64.0, 1.0, 0.0), "", sans(), 12.0, HEADER, false)?;
            let cell = b.label(&format!("PlayerStatDisplay_{index}"), body, udim2(0.0, 0.0, 0.0, 0.0), udim2(0.0, 64.0, 1.0, 0.0), "", sans(), 15.0, GLYPH, false)?;
            for id in [header, cell] {
                b.set_now(id, "TextTruncate", b.item("TextTruncate", "AtEnd"));
            }
            headers.push(header);
            cells.push(cell);
        }
        let bottom = clip("BottomRoundedRect", 58.0, 7.0)?;
        let bottom_box = b.panel("1", bottom, udim2(0.0, 0.0, 0.0, -8.0), udim2(1.0, 0.0, 0.0, 14.0), 0.3)?;
        b.corner(bottom_box, 0.0, 7.0)?;
        Some(Tree { bar, list, menu, unibar, chat_icon, health, fill, hurt, holder, title, people, body, bottom, name, headers, cells })
    }
}

impl Builder<'_> {
    fn set_now(&self, id: InstanceId, property: &str, value: Value) {
        if let Some(name) = self.engine.reflection.name_id(property) {
            core::raw_set_silent(&mut self.engine.state.borrow_mut(), &self.engine.reflection, id, name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The texts a 0.747 client's player list writes for stats (two sessions alike)
    #[test]
    fn stat_texts_are_the_clients() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"), (7.0, "7"), (999.0, "999"), (1000.0, "1,000"), (1234.0, "1,234"), (9999.0, "9,999"), (10000.0, "10,000"),
            (12345.0, "12,345"), (99999.0, "99,999"), (100000.0, "100,000"), (123456.0, "123,456"), (999999.0, "999,999"),
            (1000000.0, "1M"), (1234567.0, "1.2M"), (9999999.0, "9.9M"), (12345678.0, "12.3M"), (100000000.0, "100M"),
            (999999999.0, "999M"), (1000000000.0, "1B"), (1234567890.0, "1.2B"), (1e12, "1,000B"), (1.5e15, "1,500,000B"),
            (-1.0, "-1"), (-999.0, "-999"), (-1000.0, "-1,000"), (-1234.0, "-1,234"), (-12345.0, "-12,345"), (-1234567.0, "-1.2M"),
            (1050000.0, "1M"), (1099999.0, "1M"), (1100000.0, "1.1M"), (1999999.0, "1.9M"), (2000000.0, "2M"), (10500000.0, "10.5M"),
            (99999999.0, "99.9M"), (123456789.0, "123M"), (1100000000.0, "1.1B"), (9999999999.0, "9.9B"), (12345678901.0, "12.3B"),
            (999999999999.0, "999B"), (-1000000.0, "-1M"), (-1999999.0, "-1.9M"), (-1234567890.0, "-1.2B"),
            (0.5, "0.5"), (0.123456, "0.1"), (1.25, "1.25"), (12.5, "12.5"), (99.99, "99.99"), (1234.5, "1,234.5"), (12345.678, "12.3K"),
            (1e6 + 0.5, "1M"), (-0.5, "-0.5"), (1e-7, "1e-07"), (1.0 / 3.0, "0.3"), (1.234567, "1.2"), (12.3456, "12.3"), (123.456, "123"),
            (2.5, "2.5"), (1.005, "1.005"), (0.05, "0.05"), (0.001, "0.001"), (0.0001, "0.0001"), (5e-5, "0"), (9.99, "9.99"), (9.999, "9.999"),
            (99.999, "99.999"), (999.9, "999.9"), (999.99, "999.99"), (1000.5, "1,000.5"), (9999.9, "9,999.9"), (10000.5, "10K"),
            (99999.9, "99.9K"), (123456.7, "123K"), (1.25e6, "1.2M"), (1e9, "1B"), (3.75, "3.75"), (0.25, "0.25"), (0.75, "0.75"),
            (0.125, "0.125"), (-1.25, "-1.25"), (-0.123, "-0.123"), (-12345.6, "-12.3K"), (1e15, "1,000,000B"), (1e21, "1e+21"),
            (f64::NAN, "nan"), (f64::INFINITY, "inf"),
        ];
        let wrong: Vec<String> = cases.iter().filter(|(value, text)| stat_text(*value) != *text).map(|(value, text)| format!("{value}: {} (client {text})", stat_text(*value))).collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// The health bar's colour at 2 % steps as a client fills it (a few of the 60 measured shares)
    #[test]
    fn health_colours_are_the_clients() {
        let level = |share: f32| {
            let (r, g, b) = health_colour(share);
            [(r * 255.0).round() as i32, (g * 255.0).round() as i32, (b * 255.0).round() as i32]
        };
        assert_eq!(level(1.0), [27, 252, 107]);
        assert_eq!(level(0.8), [27, 252, 107]);
        assert_eq!(level(0.5), [250, 235, 0]);
        assert_eq!(level(0.4), [239, 216, 6]);
        assert_eq!(level(0.2), [249, 54, 3]);
        assert_eq!(level(0.05), [255, 28, 0]);
        assert_eq!(level(0.0), [255, 28, 0]);
    }

    /// The columns' order: every measured set of one to three stats but two (whose names share a hash slot)
    #[test]
    fn stat_order_follows_the_measured_sets() {
        let order = |names: &str| {
            let names: Vec<String> = names.split(',').map(str::to_string).collect();
            stat_order(&names).into_iter().map(|index| names[index].clone()).collect::<Vec<_>>().join(",")
        };
        for (made, shown) in [
            ("Coins,Gems", "Gems,Coins"), ("Kills,Deaths", "Deaths,Kills"), ("Level,XP,Gold", "Level,XP,Gold"),
            ("Wins,Coins,Rebirths", "Wins,Rebirths,Coins"), ("Z,Y", "Y,Z"), ("Cash,Level", "Level,Cash"), ("Stage", "Stage"),
            ("A,B,C", "A,C,B"), ("C,B,A", "A,C,B"), ("Coins,Rank,Speed", "Speed,Rank,Coins"), ("B,A", "A,B"), ("Coins,Rank", "Rank,Coins"),
        ] {
            assert_eq!(order(made), shown, "{made}");
        }
    }

    /// The flash: in over 0.1 s, held to 2/15 s, out over 0.5 s (the client's captures frame by frame); a second hit starts
    /// from where the first left it
    #[test]
    fn hurt_flash_fades_in_and_out() {
        let hit = Hit { frame: 10, from: 1.0 };
        let at = |frame| hurt_transparency(hit, frame, 1.0 / 60.0);
        assert_eq!(at(10), Some(1.0));
        assert!((at(13).unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(at(16), Some(0.0));
        assert_eq!(at(18), Some(0.0));
        assert!((at(19).unwrap() - 1.0 / 30.0).abs() < 1e-9);
        assert!((at(47).unwrap() - 29.0 / 30.0).abs() < 1e-9);
        assert_eq!(at(48), None);
        let again = Hit { frame: 20, from: 1.0 / 30.0 };
        assert!((hurt_transparency(again, 23, 1.0 / 60.0).unwrap() - 1.0 / 60.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod run_tests {
    use serde_json::Value as Json;

    /// The scene of `fixture` (fixtures/) at 900 x 500 with `extra` options
    fn scene(fixture: &str, extra: &[&str]) -> Json {
        let repo = crate::engine::default_repo();
        let mut list: Vec<String> = vec![
            "--repo".into(), repo.to_string_lossy().into_owned(),
            "--roots".into(), repo.join("tools/tests/oracle-root").to_string_lossy().into_owned(),
            "--fixture".into(), repo.join("fixtures").join(fixture).to_string_lossy().into_owned(),
            "--viewport".into(), "900x500".into(), "--time".into(), "1.5".into(), "--no-snapshots".into(),
        ];
        list.extend(extra.iter().map(|arg| arg.to_string()));
        crate::engine::cli::run_once(&crate::engine::cli::parse(&list).unwrap()).unwrap().scene
    }

    /// The node at `path` (its rect and whether it shows)
    fn node<'a>(scene: &'a Json, path: &str) -> Option<&'a Json> {
        scene["nodes"].as_array().unwrap().iter().find(|node| node["path"] == path)
    }

    fn rect(scene: &Json, path: &str) -> Vec<f64> {
        node(scene, path).unwrap_or_else(|| panic!("no {path}"))["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
    }

    /// Whether the node at `path` shows (a hidden one may not be exported at all)
    fn visible(scene: &Json, path: &str) -> bool {
        node(scene, path).is_some_and(|node| node["visible"] != false)
    }

    fn text(scene: &Json, path: &str) -> String {
        node(scene, path).unwrap_or_else(|| panic!("no {path}"))["props"]["text"].as_str().unwrap_or("").to_string()
    }

    /// A play run draws Roblox's top bar, player list (with the leaderstats) and health bar over the game's GUI, last, in
    /// the CoreGui pass; edit mode and a play run without a top-bar inset draw none
    #[test]
    fn a_play_run_draws_roblox_gui() {
        let play = scene("chrome-topbar.luau", &["--mode", "play"]);
        assert_eq!(rect(&play, "TopBarApp.MenuIcon"), [16.0, 12.0, 44.0, 44.0]);
        assert_eq!(rect(&play, "TopBarApp.Unibar"), [68.0, 12.0, 96.0, 44.0]);
        assert!(visible(&play, "TopBarApp.Unibar.Chat"));
        // (two stats: 165 + 2 x 77 wide at the right edge less 170 + 2 x 77)
        assert_eq!(rect(&play, "PlayerList.Children")[..3], [576.0, 62.0, 319.0]);
        assert_eq!(text(&play, "PlayerList.Children.Body.PlayerName"), "Player");
        let columns: Vec<(String, String)> = (0..2)
            .map(|index| (text(&play, &format!("PlayerList.Children.TitleBar.stat_{index}")), text(&play, &format!("PlayerList.Children.Body.PlayerStatDisplay_{index}"))))
            .collect();
        assert_eq!(columns, [("Rank".to_string(), "Gold".to_string()), ("Coins".to_string(), "12,345".to_string())]);
        assert_eq!(rect(&play, "TopBarApp.HealthBar"), [759.0, 24.0, 125.0, 20.0]);
        assert_eq!(rect(&play, "TopBarApp.HealthBar.Fill")[2], 37.5);
        let layers = play["layers"].as_array().unwrap();
        let last: Vec<(f64, bool)> = layers[layers.len() - 2..].iter().map(|layer| (layer["displayOrder"].as_f64().unwrap(), layer["playerGui"] == true)).collect();
        assert_eq!(last, [(1.0, false), (6.0, false)], "the player list and the top bar last, in the CoreGui pass");
        for (name, extra) in [("edit", &["--mode", "edit"][..]), ("no inset", &["--mode", "play", "--topbar-inset", "0"][..])] {
            let none = scene("chrome-topbar.luau", extra);
            assert!(node(&none, "TopBarApp").is_none() && node(&none, "PlayerList").is_none(), "{name}");
        }
    }

    /// SetCoreGuiEnabled Chat and PlayerList false: one button in the Unibar, no list; GuiService's inset stays 58
    #[test]
    fn the_game_turns_pieces_off() {
        let off = scene("chrome-off.luau", &["--mode", "play"]);
        assert_eq!(rect(&off, "TopBarApp.Unibar"), [68.0, 12.0, 52.0, 44.0]);
        assert!(!visible(&off, "TopBarApp.Unibar.Chat"));
        assert!(!visible(&off, "PlayerList"));
        assert!(!visible(&off, "TopBarApp.HealthBar"));
        let story = off["nodes"].as_array().unwrap().iter().find(|node| node["name"] == "Inset").unwrap();
        assert_eq!(story["props"]["text"], "inset 58");
    }

    /// A touch device shows no player list (and closes chat's window on a small screen: chat's glyph regular); a gamepad
    /// neither the menu button nor the Unibar
    #[test]
    fn devices_show_their_pieces() {
        let touch = scene("chrome-topbar.luau", &["--mode", "play", "--device", "touch"]);
        assert!(!visible(&touch, "PlayerList"));
        assert!(visible(&touch, "TopBarApp.MenuIcon") && visible(&touch, "TopBarApp.HealthBar"));
        let pad = scene("chrome-topbar.luau", &["--mode", "play", "--device", "gamepad"]);
        assert!(!visible(&pad, "TopBarApp.MenuIcon") && !visible(&pad, "TopBarApp.Unibar") && !visible(&pad, "PlayerList"));
        assert!(visible(&pad, "TopBarApp.HealthBar"));
    }
}
