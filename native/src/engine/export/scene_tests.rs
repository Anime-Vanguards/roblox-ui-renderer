//! Export tests: ports of runtime/tests/export_cache.luau, export_order.luau and
//! scene_presentation.luau, and tools/tests/rotation_clipping.luau. The scripts run in the Rust
//! engine with test hooks as globals (the Luau tests' direct Scene / Layout / Hit calls):
//!   Check(root, name)   Layout.UpdateAll, a build of the test's retained cache (Scene.Build with
//!                       the cache), then the comparison with a one-shot build (Scene.Build without
//!                       one) the Luau tests make: same node count, every node the same (without
//!                       `order`; in presentation mode without clipOwners / rotationCull /
//!                       scroll.presentation), same draw lists. Returns { nodes = by id, layers,
//!                       rebuilt = ids, count }.
//!   Export(root)        the same build without the comparison
//!   Presentation(on)    Cache.Presentation (Retained::set_presentation)
//!   Fresh(root)         Scene.Build without a cache (the scene table)
//!   FreshCacheDraw(root) Scene.Build with a new presentation cache: layers[1].draw
//!   Id(object)          the node id (creation serial)
//!   Contains(object, x, y) / GroupCulled(object)   Hit.Contains / Entry.Groups[1].Culled
//!
//! The Rust retained export returns what the client does not hold yet (runtime/session.luau
//! ReplyDelta: rebuilt nodes as whole nodes or patches, removed ids, changed draw lists), so the
//! hooks keep the client's view (patches merged, removed ids dropped) and compare that. Where the
//! Luau tests inspect cache internals (Cache.Branches / Cache.Nodes[..].Node identity) the ports
//! assert the observable equivalent: the node was not rebuilt (absent from the build's nodes).
//!
//! scene_presentation's `presentation` header checks run through the in-process session (the Rust
//! header is written by `Session::delta`, not by the export).
//!
//! Not portable (Luau cache design the Rust port does not have):
//! - export_cache's Structured cache (native scene without nodeJson), SerializeTime, per-entry
//!   Json / StaticJson, the branch reference bound and the transport switch: the Rust cache keeps no
//!   JSON strings or branches; nodes are serde_json values the session serializes. The "stale
//!   serialized node" checks reduce to value equality with the one-shot build.
//! - rotation_clipping's mode transitions on one cache (Layout.ClipsDescendantsSupportsRotation
//!   toggled at runtime): the Rust option is fixed per engine (`Options`), so each mode runs on
//!   its own engine and cache.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use mlua::prelude::*;
use serde_json::{Map, Value as Json};

use super::{Records, build, retained::{Retained, Update}};
use crate::engine::{
    Engine, InstanceId, Options,
    input::{self, hit},
    layout,
    lua::{self as engine_lua, globals::CallOutcome},
};

fn start(options: Options) -> (Lua, Rc<Engine>) {
    let engine = crate::engine::new_engine(options).expect("engine");
    let lua = Lua::new();
    engine_lua::start(&lua, &engine).expect("start");
    engine.state.borrow_mut().sched.set_main_thread(&lua.current_thread());
    (lua, engine)
}

fn to_lua(lua: &Lua, value: &Json) -> LuaResult<LuaValue> {
    let options = LuaSerializeOptions::new().serialize_none_to_null(false).serialize_unit_to_null(false).set_array_metatable(false);
    lua.to_value_with(value, options)
}

fn id_of(value: &LuaValue) -> LuaResult<InstanceId> {
    engine_lua::instance_id(value).ok_or_else(|| LuaError::runtime("instance expected"))
}

/// What the client holds after applying every build's update.
#[derive(Default)]
struct Client {
    nodes: BTreeMap<u64, Map<String, Json>>,
    layers: Json,
}

impl Client {
    /// Applies `update` (runtime/session.luau's client merge); returns the rebuilt ids.
    fn apply(&mut self, update: Update) -> Result<Vec<u64>, String> {
        let mut rebuilt = Vec::new();
        for node in update.nodes {
            let Json::Object(mut node) = node else { return Err("node is not an object".into()) };
            let id = node.get("id").and_then(Json::as_u64).ok_or("node without id")?;
            rebuilt.push(id);
            if node.remove("patch") == Some(Json::Bool(true)) {
                let unset = node.remove("unset");
                let held = self.nodes.get_mut(&id).ok_or_else(|| format!("patch for node {id} the client does not hold"))?;
                held.extend(node);
                for key in unset.as_ref().and_then(Json::as_array).into_iter().flatten() {
                    held.remove(key.as_str().unwrap_or_default());
                }
            } else {
                self.nodes.insert(id, node);
            }
        }
        for id in update.removed {
            self.nodes.remove(&(id as u64));
        }
        if let Some(layers) = update.layers {
            self.layers = layers;
        }
        Ok(rebuilt)
    }

    /// The Luau tests' comparison with a one-shot export (scene_presentation strips the
    /// presentation-only fields first).
    fn compare(&self, fresh: &Json, presentation: bool) -> Result<(), String> {
        let nodes = fresh["nodes"].as_array().ok_or("one-shot export without nodes")?;
        if nodes.len() != self.nodes.len() {
            return Err(format!("unexpected culling: retained {} nodes, one-shot {}", self.nodes.len(), nodes.len()));
        }
        for node in nodes {
            let mut expected = node.as_object().cloned().unwrap_or_default();
            expected.remove("order");
            let id = expected.get("id").and_then(Json::as_u64).unwrap_or_default();
            let mut held = self.nodes.get(&id).cloned().ok_or_else(|| format!("retained export omitted node {id}"))?;
            if presentation {
                held.remove("clipOwners");
                held.remove("rotationCull");
                if let Some(Json::Object(scroll)) = held.get_mut("scroll") {
                    scroll.remove("presentation");
                }
            }
            if held != expected {
                return Err(format!(
                    "stale node {}:\n retained {}\n one-shot {}",
                    expected.get("name").cloned().unwrap_or(Json::Null),
                    Json::Object(held),
                    Json::Object(expected)
                ));
            }
        }
        if self.layers != fresh["layers"] {
            return Err(format!("stale draw order:\n retained {}\n one-shot {}", self.layers, fresh["layers"]));
        }
        Ok(())
    }
}

/// The test hooks as globals of a script environment (one retained cache per script, created
/// before the script's instances as the Luau tests install their hooks first).
fn hooks(lua: &Lua, engine: &Rc<Engine>, env: &LuaTable) -> LuaResult<()> {
    let cache = Rc::new(RefCell::new(Retained::new(engine)));
    let client = Rc::new(RefCell::new(Client::default()));
    let presentation = Rc::new(Cell::new(false));

    for (name, compare) in [("Check", true), ("Export", false)] {
        let (engine, cache, client, presentation) = (engine.clone(), cache.clone(), client.clone(), presentation.clone());
        let f = lua.create_function(move |lua, (root, label): (LuaValue, Option<String>)| {
            let root = id_of(&root)?;
            let label = label.unwrap_or_default();
            let fail = |message: String| LuaError::runtime(format!("{label}: {message}"));
            layout::update_all(&engine);
            let update = cache.borrow_mut().build(&engine, &[root]);
            let mut client = client.borrow_mut();
            let rebuilt = client.apply(update).map_err(fail)?;
            if compare {
                let fresh = build(&engine, &[root], &Records::default(), Map::new());
                client.compare(&fresh, presentation.get()).map_err(fail)?;
            }
            let result = lua.create_table()?;
            let nodes = lua.create_table()?;
            for (id, node) in &client.nodes {
                nodes.raw_set(*id, to_lua(lua, &Json::Object(node.clone()))?)?;
            }
            result.raw_set("nodes", nodes)?;
            result.raw_set("layers", to_lua(lua, &client.layers)?)?;
            result.raw_set("rebuilt", rebuilt)?;
            result.raw_set("count", client.nodes.len())?;
            Ok(result)
        })?;
        env.raw_set(name, f)?;
    }
    {
        let (cache, presentation) = (cache.clone(), presentation.clone());
        let f = lua.create_function(move |_, on: bool| {
            presentation.set(on);
            cache.borrow_mut().set_presentation(on);
            Ok(())
        })?;
        env.raw_set("Presentation", f)?;
    }
    {
        let engine = engine.clone();
        let f = lua.create_function(move |lua, root: LuaValue| {
            layout::update_all(&engine);
            to_lua(lua, &build(&engine, &[id_of(&root)?], &Records::default(), Map::new()))
        })?;
        env.raw_set("Fresh", f)?;
    }
    {
        let engine = engine.clone();
        let f = lua.create_function(move |lua, root: LuaValue| {
            let mut cache = Retained::new(&engine);
            cache.set_presentation(true);
            layout::update_all(&engine);
            let layers = cache.build(&engine, &[id_of(&root)?]).layers.ok_or_else(|| LuaError::runtime("first build without layers"))?;
            to_lua(lua, &layers[0]["draw"])
        })?;
        env.raw_set("FreshCacheDraw", f)?;
    }
    {
        let engine = engine.clone();
        let f = lua.create_function(move |_, object: LuaValue| Ok(engine.state.borrow().dom.at(id_of(&object)?).serial))?;
        env.raw_set("Id", f)?;
    }
    {
        let engine = engine.clone();
        let f = lua.create_function(move |_, (object, x, y): (LuaValue, f64, f64)| {
            let id = id_of(&object)?;
            let order = input::get(&engine).expect("input").draw_order(&engine);
            Ok(order.iter().find(|e| e.id == id).is_some_and(|e| hit::contains(e, x, y)))
        })?;
        env.raw_set("Contains", f)?;
    }
    {
        let engine = engine.clone();
        let f = lua.create_function(move |_, object: LuaValue| {
            let id = id_of(&object)?;
            let order = input::get(&engine).expect("input").draw_order(&engine);
            Ok(order.iter().find(|e| e.id == id).and_then(|e| e.groups.as_ref().and_then(|g| g.first().map(|g| g.culled))))
        })?;
        env.raw_set("GroupCulled", f)?;
    }
    Ok(())
}

/// Runs `source` with the test hooks (and `globals`); panics with the script's error.
fn run(options: Options, source: &str, globals: &[(&str, bool)]) {
    let (lua, engine) = start(options);
    let script = {
        let mut state = engine.state.borrow_mut();
        let class = engine.reflection.class_id("ModuleScript").unwrap();
        crate::engine::instance::create_named(&mut state, &engine.reflection, class, "Test")
    };
    let env = engine_lua::modules::environment(&lua, &engine, script).expect("environment");
    hooks(&lua, &engine, &env).expect("hooks");
    for (name, value) in globals {
        env.raw_set(*name, *value).unwrap();
    }
    let function = lua.load(format!("{PRELUDE}\n{source}")).set_name("=Test").set_environment(env).into_function().expect("compile");
    let call = engine_lua::globals::call(&lua, &engine, function, LuaMultiValue::new()).expect("call");
    if let CallOutcome::Failed(message, _) = call.outcome().unwrap() {
        panic!("test script failed: {message}");
    }
    let output = engine.state.borrow().output.entries.iter().map(|e| format!("{}: {}", e.kind, e.message)).filter(|l| l.starts_with("error")).collect::<Vec<_>>();
    assert!(output.is_empty(), "{output:?}");
}

const PRELUDE: &str = r#"
local function Same(A, B): boolean
	if type(A) ~= type(B) then return false end
	if type(A) ~= "table" then return A == B end
	for Key, Value in A do if not Same(Value, B[Key]) then return false end end
	for Key in B do if A[Key] == nil then return false end end
	return true
end
"#;

#[test]
fn viewport_models_and_external_cameras_update() {
    run(crate::engine::options(), r#"
local root = Instance.new("ScreenGui")
local viewport = Instance.new("ViewportFrame", root)
local eye = Vector3.new(6, 4, 9)
local look = CFrame.lookAt(eye, Vector3.zero)
assert(look.LookVector:Dot(-eye.Unit) > 0.999, "lookAt must face the target")
assert((look:PointToObjectSpace(Vector3.zero) - Vector3.new(0, 0, -eye.Magnitude)).Magnitude < 0.01)
viewport.Size = UDim2.fromOffset(320, 240)
local camera = Instance.new("Camera")
camera.CFrame = CFrame.new(0, 0, 10)
viewport.CurrentCamera = camera
local world = Instance.new("WorldModel", viewport)
local model = Instance.new("Model", world)
local part = Instance.new("Part", model)
part.Color = Color3.new(1, 0, 0)
part.Size = Vector3.new(2, 2, 2)
model:PivotTo(CFrame.new(3, 4, 5))
assert((part.CFrame.Position - Vector3.new(3, 4, 5)).Magnitude < 0.01)
model:ScaleTo(2)
assert((part.Size - Vector3.new(4, 4, 4)).Magnitude < 0.01)
model:ScaleTo(1)
model:PivotTo(CFrame.new())
local function image()
    local result = Check(root, "viewport")
    return result.nodes[Id(viewport)].image.uri
end
local initial = image()
assert(string.sub(initial, 1, 15) == "rur-viewport://")
assert(image() == initial)
part.Color = Color3.new(0, 1, 0)
local recolored = image()
assert(recolored ~= initial)
camera.CFrame = CFrame.new(2, 0, 10)
local moved = image()
assert(moved ~= recolored)
camera.FieldOfView = 40
assert(image() ~= moved)
local beforeResize = image()
viewport.Size = UDim2.fromOffset(600, 300)
assert(image() ~= beforeResize)
local beforeRemoval = image()
part.Parent = nil
assert(image() ~= beforeRemoval)
part.Parent = model
image()
world:Destroy()
image()
local beforeCameraRemoval = image()
viewport.CurrentCamera = nil
camera:Destroy()
assert(image() ~= beforeCameraRemoval)
"#, &[]);
}

fn options(width: f64, height: f64, pixel_scale: f64) -> Options {
    Options { viewport_width: width, viewport_height: height, pixel_scale, ..crate::engine::options() }
}

/// runtime/tests/export_cache.luau: after every change the retained export (the client's view of
/// its updates) equals a one-shot export, and unchanged / unrelated nodes are not rebuilt.
#[test]
fn export_cache_geometry_decorations_text_ordering() {
    run(
        options(800.0, 600.0, 1.5),
        r##"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
Root.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
local Holder = Instance.new("Frame", Root)
Holder.Position = UDim2.fromOffset(50, 40)
Holder.Size = UDim2.fromOffset(400, 250)
Holder.ClipsDescendants = true
local A, B = Instance.new("TextLabel", Holder), Instance.new("Frame", Holder)
A.Size = UDim2.fromOffset(180, 70)
A.Text = "Cached geometry"
B.Size = UDim2.fromOffset(70, 60)
local Corner = Instance.new("UICorner", A)
local Stroke = Instance.new("UIStroke", A)
local Gradient = Instance.new("UIGradient", Stroke)

local function Run(Name) return Check(Root, Name) end
Run("initial")
Run("retain stable branch")
-- Cache.Branches[Holder] unchanged: nothing was rebuilt
assert(#Run("unchanged retained branch").rebuilt == 0, "unchanged branch was rebuilt")
Holder.Position = UDim2.fromOffset(65.25, 62.75)
Run("translate cached appearance")
A.Size = UDim2.fromOffset(190, 90)
Corner.CornerRadius = UDim.new(0.3, 2)
Run("resize decorations")
Holder.Rotation = 12
Run("rotate ancestor")
Gradient.Color = ColorSequence.new(Color3.fromRGB(255, 0, 50), Color3.fromRGB(0, 80, 255))
Run("stroke gradient grandchild")
A.BackgroundColor3 = Color3.fromRGB(20, 30, 40)
A.Text = "Changed content"
Run("appearance and text")
A.RichText = true
A.TextWrapped = true
A.Text = '<font color="#ff8800">Rich text</font> with <u>wrapped runs</u> and "quoted" text'
Run("rich multiline content")
Holder.Position = UDim2.fromOffset(72.5, 65.125)
Run("translate cached rich lines")
A.TextTransparency = 0.35
A.MaxVisibleGraphemes = 12
Run("cached text appearance invalidation")
A.TextSize = 19
Run("cached glyph size invalidation")
A.Size = UDim2.fromOffset(120, 130)
Run("cached rich wrapping invalidation")
A.LineHeight = 1.4
Run("cached rich line height invalidation")
A.TextScaled = true
Run("cached scaled text invalidation")
Holder.Position = UDim2.fromOffset(80, 70)
Run("translate cached scaled text")
A.TextScaled = false
for _, Value in { 0.1, 1 / 3, math.pi, 2 ^ -24, -1e-7 } do
	A.Position = UDim2.new(Value / 100, 0, Value / 100, 0)
	A.Rotation = Value
	Run("serialized numeric precision")
end
A.ZIndex, B.ZIndex = 3, 1
Run("sibling order")
Root.ZIndexBehavior = Enum.ZIndexBehavior.Global
Run("global order")
A.Text = "Changed while globally ordered"
Run("global appearance invalidates retained sibling branch")
Root.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
Run("restore sibling order")
Holder.Visible = false
Run("hide retained branch")
A.Text = "Changed while hidden"
Run("hidden appearance change")
Holder.Visible = true
Run("show updated branch")
B.Name = "Renamed"
B.Parent = Root
Run("rename and reparent")
B:Destroy()
Run("destroy")

local Unrelated = Instance.new("Frame", Root)
Unrelated.Size = UDim2.fromOffset(20, 20)
local Folder = Instance.new("Folder", Holder)
local NestedFolder = Instance.new("Folder", Folder)
local Nested = Instance.new("Frame", NestedFolder)
Nested.Size = UDim2.fromOffset(25, 25)
local NestedLabel = Instance.new("TextLabel", Nested)
NestedLabel.Size = UDim2.fromOffset(20, 20)
Run("independent hierarchy branches")
-- Cache.Nodes[Unrelated].Node identity from here on: Unrelated is never rebuilt
local function Unchanged(Name, Message)
	assert(not table.find(Run(Name).rebuilt, Id(Unrelated)), Message or `{Name}: rebuilt unrelated branch`)
end
Folder.Name = "RenamedFolder"
Unchanged("ancestor folder rename updates descendant paths", "rename invalidated unrelated branch")
Folder.Parent = Root
Unchanged("folder subtree reparent updates paths and GUI parent ids", "reparent invalidated unrelated branch")
Corner.Parent = Nested
Unchanged("decoration reparent invalidates both owners")
Gradient.Parent = Nested
Unchanged("nested stroke gradient reparent invalidates both owners")
Gradient.Parent = Stroke
Unchanged("stroke gradient reattachment")
Corner:Destroy()
Unchanged("decoration destruction invalidates owner")
Folder:Destroy()
Unchanged("subtree destruction", "destroy invalidated unrelated branch")
local Empty = Instance.new("Frame", Root)
local Removed = Instance.new("Frame", Empty)
Run("branch before final child removed")
Removed:Destroy()
Run("empty branch after final child removed")
Run("empty branch remains empty")
local Scale = Instance.new("UIScale", Holder)
Scale.Scale = 0.8
Run("inherited scale invalidates descendant records")
Scale.Scale = 1
Run("restore inherited scale")
"##,
        &[],
    );
}

/// runtime/tests/export_order.luau: draw lists (Sibling / Global, nested CanvasGroups, scroll bar
/// entries, disabled roots, automatic-size suppression) with and without a presentation cache.
#[test]
fn export_order_sibling_global_groups_and_automatic_size() {
    run(
        options(800.0, 600.0, 1.0),
        r#"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local Names = {}
local function Make(Class, Name, Parent, Z)
	local Object = Instance.new(Class, Parent)
	Object.Name = Name
	Object.Size = UDim2.fromOffset(100, 100)
	Object.ZIndex = Z
	Names[Id(Object)] = Name
	return Object
end
local A = Make("Frame", "A", Root, 2)
Make("Frame", "AChild", A, 0)
Make("Frame", "B", Root, 1)
local Group = Make("CanvasGroup", "Group", Root, 3)
Make("Frame", "GChild", Group, 0)
local Scroll = Make("ScrollingFrame", "Scroll", Group, 1)
Make("Frame", "ScrollChild", Scroll, 2)
local Nested = Make("CanvasGroup", "Nested", Group, 4)
Make("Frame", "NestedChild", Nested, 1)
local Hidden = Make("Frame", "Hidden", Root, 0)
Hidden.Visible = false
Make("Frame", "HiddenChild", Hidden, 0)
local function Describe(Draw)
	local Result = {}
	for _, Entry in Draw do
		if type(Entry) == "table" then
			table.insert(Result, { group = Names[Entry.group], draw = Describe(Entry.draw) })
		else
			table.insert(Result, if Entry < 0 then Names[-Entry] .. ":bars" else Names[Entry])
		end
	end
	return Result
end
local GroupDraw = { group = "Group", draw = { "Group", "GChild", "Scroll", "Scroll:bars", "ScrollChild",
	{ group = "Nested", draw = { "Nested", "NestedChild" } } } }
local function Check(Mode, Expected)
	Root.ZIndexBehavior = Enum.ZIndexBehavior[Mode]
	assert(Same(Describe(Fresh(Root).layers[1].draw), Expected), `{Mode}: draw topology changed`)
	assert(Same(Describe(FreshCacheDraw(Root)), Expected), `{Mode}: draw topology changed (presentation cache)`)
end
Check("Sibling", { "B", "A", "AChild", GroupDraw })
Check("Global", { "AChild", "B", "A", GroupDraw })
Root.Enabled = false
Check("Sibling", {})
Check("Global", {})
Root.Enabled = true
local Clip = Make("Frame", "Clip", Root, 5)
Clip.Size = UDim2.fromOffset(20, 20)
Clip.ClipsDescendants = true
local Auto = Make("Frame", "Auto", Clip, 1)
Auto.Position = UDim2.fromOffset(100, 100)
Auto.AutomaticSize = Enum.AutomaticSize.XY
Make("Frame", "AutoChild", Auto, 1)
local AutoGroup = Make("CanvasGroup", "AutoGroup", Clip, 2)
AutoGroup.Position = UDim2.fromOffset(100, 100)
AutoGroup.AutomaticSize = Enum.AutomaticSize.XY
Make("Frame", "AutoGroupChild", AutoGroup, 1)
Check("Sibling", { "B", "A", "AChild", GroupDraw, "Clip" })
Check("Global", { "AChild", "B", "A", GroupDraw, "Clip" })
Instance.new("UIListLayout", Auto)
Check("Sibling", { "B", "A", "AChild", GroupDraw, "Clip", "AutoChild" })
Check("Global", { "AChild", "B", "AutoChild", "A", GroupDraw, "Clip" })
"#,
        &[],
    );
}

/// runtime/tests/scene_presentation.luau: presentation mode retains culled nodes with their clip
/// provenance and scroll metadata (range, direction, zeroSensitive / enabled), without changing
/// the canonical node fields or draw lists; normal mode carries none of it.
#[test]
fn scene_presentation_clip_owners_and_scroll_metadata() {
    run(
        options(320.0, 240.0, 1.5),
        r#"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local Frame = Instance.new("Frame", Root)
Frame.Size = UDim2.fromOffset(220, 180)
Frame.ClipsDescendants = true
local Scroll = Instance.new("ScrollingFrame", Frame)
Scroll.Size = UDim2.fromOffset(180, 120)
Scroll.CanvasSize = UDim2.fromOffset(180, 900)
Scroll.ScrollingDirection = Enum.ScrollingDirection.Y
local Nested = Instance.new("Frame", Scroll)
Nested.Size = UDim2.fromOffset(150, 400)
Nested.ClipsDescendants = true
local Label = Instance.new("TextLabel", Nested)
Label.Position = UDim2.fromOffset(10, 300)
Label.Size = UDim2.fromOffset(80, 40)
Label.Text = "retained row"
local Group = Instance.new("CanvasGroup", Scroll)
Group.Position = UDim2.fromOffset(0, 600)
Group.Size = UDim2.fromOffset(150, 80)
local GroupChild = Instance.new("Frame", Group)
GroupChild.Size = UDim2.fromOffset(30, 30)
local Before = Export(Root)
local function Run()
	-- Check compares with the one-shot export (count, canonical fields, draw lists)
	local Retained = Check(Root, "presentation")
	for _, Node in Retained.nodes do
		if Node.parent then
			assert(Retained.nodes[Node.parent], "missing retained parent")
		end
		for _, Owner in Node.clipOwners do
			assert(Retained.nodes[Owner], "missing clip owner")
		end
	end
	return Retained.nodes, Retained
end
Presentation(true)
local Nodes, Retained = Run()
assert(Retained.count > Before.count, "fixture must exercise exporter culling")
assert(Same(Nodes[Id(Label)].clipOwners, { Id(Frame), Id(Scroll), Id(Nested) }), "nested clip provenance lost")
assert(Same(Nodes[Id(GroupChild)].clipOwners, { Id(Frame), Id(Scroll), Id(Group) }), "offscreen group subtree lost")
assert(Nodes[Id(Scroll)].scroll.presentation.direction == "Y")
assert(Nodes[Id(Scroll)].scroll.presentation.range[2] > 0)
Scroll.CanvasPosition = Vector2.new(0, 320)
Run()
Nested.Rotation = 360
Nodes = Run()
assert(Same(Nodes[Id(Label)].clipOwners, { Id(Frame), Id(Scroll) }), "modern rotated descendants retain ancestor clips; rotated owners add none")
Nested.Rotation = 0
Nested.ClipsDescendants = false
Nodes = Run()
assert(Same(Nodes[Id(Label)].clipOwners, { Id(Frame), Id(Scroll) }), "clip owner toggle was cached")

local AutoRow = Instance.new("Frame", Scroll)
AutoRow.Position = UDim2.fromOffset(0, 300)
AutoRow.Size = UDim2.fromOffset(80, 0)
AutoRow.AutomaticSize = Enum.AutomaticSize.Y
local AutoChild = Instance.new("Frame", AutoRow)
AutoChild.Size = UDim2.fromOffset(80, 40)
Scroll.CanvasPosition = Vector2.zero
Nodes, Retained = Run()
assert(Nodes[Id(AutoRow)].culled and Nodes[Id(AutoChild)].culled, "fixture must exercise zero-pose automatic culling")
assert(Nodes[Id(Scroll)].scroll.presentation.zeroSensitive, "automatic descendant did not invalidate scroll metadata")
assert(Nodes[Id(Scroll)].scroll.presentation.enabled == false, "zero-pose prediction must wait for authoritative draw lists")
local function DrawContains(Draw, Target)
	for _, Entry in Draw do
		if Entry == Target then
			return true
		end
		if type(Entry) == "table" and (Entry.group == Target or DrawContains(Entry.draw, Target)) then
			return true
		end
	end
	return false
end
assert(not DrawContains(Retained.layers[1].draw, Id(AutoChild)), "zero-pose suppressed descendant unexpectedly drawn")
Scroll.CanvasPosition = Vector2.new(0, 280)
Nodes, Retained = Run()
assert(not Nodes[Id(AutoRow)].culled and not Nodes[Id(AutoChild)].culled, "authoritative scroll did not reveal automatic row")
assert(DrawContains(Retained.layers[1].draw, Id(AutoChild)), "revealed automatic row missing from draw lists")
assert(Nodes[Id(Scroll)].scroll.presentation.enabled ~= false, "nonzero pose must restore prediction eligibility")
AutoRow.AutomaticSize = Enum.AutomaticSize.None
Nodes = Run()
assert(not Nodes[Id(Scroll)].scroll.presentation.zeroSensitive, "automatic-size property removal left stale eligibility")
AutoRow.AutomaticSize = Enum.AutomaticSize.Y
Nodes = Run()
assert(Nodes[Id(Scroll)].scroll.presentation.zeroSensitive, "automatic-size property restoration missed eligibility")
AutoRow.Parent = Root
Nodes = Run()
assert(not Nodes[Id(Scroll)].scroll.presentation.zeroSensitive, "reparented automatic row left stale eligibility")
AutoRow.Parent = Scroll
Nodes = Run()
assert(Nodes[Id(Scroll)].scroll.presentation.zeroSensitive, "inserted automatic row missed eligibility")
AutoRow:Destroy()
Nodes = Run()
assert(not Nodes[Id(Scroll)].scroll.presentation.zeroSensitive, "destroyed automatic row left stale eligibility")
Presentation(false)
local Normal = Export(Root)
for _, Node in Normal.nodes do
	assert(Node.clipOwners == nil and (not Node.scroll or not Node.scroll.presentation), "presentation metadata leaked into normal export")
end
"#,
        &[],
    );
}

/// scene_presentation.luau's header checks (`presentation.version == 1 and retained` while
/// enabled, no header and no presentation fields in normal mode): in Rust the header is the
/// session's (Session::delta), so this runs a story in the in-process session.
#[test]
fn scene_presentation_header_follows_the_session_mode() {
    use crate::engine::{session::Session, story};
    let fixture = std::env::temp_dir().join(format!("rur-presentation-header-test-{}.luau", std::process::id()));
    std::fs::write(
        &fixture,
        r#"
        return function(Target)
            local Frame = Instance.new("Frame")
            Frame.Size = UDim2.fromOffset(220, 180)
            Frame.ClipsDescendants = true
            Frame.Parent = Target
            local Scroll = Instance.new("ScrollingFrame")
            Scroll.Size = UDim2.fromOffset(180, 120)
            Scroll.CanvasSize = UDim2.fromOffset(180, 900)
            Scroll.Parent = Frame
            local Row = Instance.new("Frame")
            Row.Position = UDim2.fromOffset(0, 600)
            Row.Size = UDim2.fromOffset(150, 40)
            Row.Parent = Scroll
        end
        "#,
    )
    .unwrap();
    let engine = crate::engine::new_engine(crate::engine::options()).expect("engine");
    let lua = Lua::new();
    let project = story::mount_project(&lua, &engine, &story::ProjectSpec::default(), std::time::Instant::now()).unwrap();
    let source = story::Source { kind: story::SourceKind::Fixture, path: fixture.to_string_lossy().into_owned() };
    let mounted = story::mount(&lua, &engine, &project, Some(&source), 0.0, (800.0, 600.0), None).unwrap();
    let mut session = Session::new(&engine, project, mounted, [0.2, 0.2, 0.2], true);
    let ready = session.ready(&lua).unwrap();
    assert!(ready.pointer("/delta/header/presentation").is_none(), "{ready}");

    let enabled = session.frame_with(&lua, 1.0 / 60.0, None, &[], Some(&serde_json::json!({ "version": 1 })));
    let header = enabled.pointer("/delta/header/presentation").unwrap_or_else(|| panic!("no presentation header: {enabled}"));
    assert_eq!(header["version"], 1, "{header}");
    assert_eq!(header["retained"], true, "{header}");
    let nodes = enabled.pointer("/delta/nodes").and_then(Json::as_array).expect("nodes");
    assert!(nodes.iter().any(|n| n.get("clipOwners").is_some()), "presentation fields missing: {enabled}");

    let normal = session.frame_with(&lua, 1.0 / 60.0, None, &[], Some(&serde_json::json!({ "version": 0 })));
    let header = normal.pointer("/delta/header").unwrap_or_else(|| panic!("mode switch did not resend the header: {normal}"));
    assert!(header.get("presentation").is_none(), "{header}");
    for node in normal.pointer("/delta/nodes").and_then(Json::as_array).expect("nodes") {
        assert!(
            node.get("clipOwners").is_none() && node.pointer("/scroll/presentation").is_none(),
            "presentation metadata leaked into normal export: {node}"
        );
    }
    drop(session);
    drop(lua);
    let _ = std::fs::remove_file(fixture);
}

const ROTATION_CLIPPING: &str = r#"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local function Box(Parent, Name, X, Y, W, H, Angle, Class)
	local F = Instance.new(Class or "Frame", Parent)
	F.Name = Name
	F.Position = UDim2.fromOffset(X, Y)
	F.Size = UDim2.fromOffset(W, H)
	F.BorderSizePixel = 0
	F.Rotation = Angle or 0
	return F
end
local Clip = Box(Root, "Clip", 100, 100, 100, 100)
Clip.ClipsDescendants = true
local Partial = Box(Clip, "Partial", -40, 25, 180, 40, 10)
local Outside = Box(Clip, "Outside", 20, -45, 180, 8, 45)
local Arm = Box(Clip, "Arm", 20, 40, 70, 40, 20)
Arm.ClipsDescendants = true
local Cancel = Box(Arm, "Cancel", -40, 0, 180, 8, -20)
local NoClip = Box(Root, "NoClip", 300, 100, 100, 100, 0, "ScrollingFrame")
NoClip.CanvasSize = UDim2.fromOffset(100, 300)
NoClip.ScrollBarThickness = 0
NoClip.ClipsDescendants = false
local Overflow = Box(NoClip, "Overflow", 110, 0, 50, 50)
local Group = Box(Root, "Group", 100, 300, 100, 100, 0, "CanvasGroup")
local GroupChild = Box(Group, "GroupChild", -40, 25, 180, 40, 10)
local Disjoint = Box(Clip, "Disjoint", 150, 0, 100, 100)
Disjoint.ClipsDescendants = true
local EmptyChild = Box(Disjoint, "EmptyChild", -200, 30, 300, 20, 10)
local OutsideGroup = Box(Clip, "OutsideGroup", 150, 0, 80, 30, 10, "CanvasGroup")
local OutsideGroupChild = Box(OutsideGroup, "OutsideGroupChild", -150, 10, 200, 10, 10)
Presentation(true)
local function Nodes(Result)
	local ByName = {}
	for _, Node in Result.nodes do ByName[Node.name] = Node end
	return ByName
end
-- The Luau test alternates the mode on one cache (true, false, true); here the mode is the
-- engine's option, so the same checks run twice on this engine's cache.
for _ = 1, 2 do
	local Result = Export(Root)
	local N = Nodes(Result)
	assert((N.Partial.clip ~= nil) == Modern)
	assert(N.Outside.culled == true, "wholly outside unrotated layout bounds are rejected in both modes")
	assert(N.Outside.rotationCull.baseCulled == false)
	assert(N.Outside.rotationCull.owners[1] == N.Clip.id)
	assert(N.Cancel.clip ~= nil, "net-zero rotation restores ancestor clip")
	assert(N.Cancel.clip[1] == 100 and N.Cancel.clip[3] == 100, "rotated owner does not add its own clip")
	assert(N.Overflow.clip == nil, "ClipsDescendants=false applies to scrolling windows")
	assert(Contains(Partial, 70, 145) == not Modern, "hit clipping follows the selected rollout")
	assert(Contains(Partial, 150, 145))
	assert(Contains(Overflow, 420, 120), "non-clipping scroller overflow remains interactive")
	assert(not Contains(GroupChild, 70, 345), "CanvasGroup texture still clips legacy rotated hit targets")
	assert(not Contains(EmptyChild, 250, 145), "empty intersections hide spanning rotated descendants")
	if not Modern then
		assert(#N.Outside.clipOwners == 0)
		assert(table.find(Result.layers[1].draw, N.Outside.id), "retained coarse-culled nodes keep draw membership")
		assert(N.EmptyChild.culled, "zero-area clips reject rotated visuals")
		assert(N.OutsideGroup.culled and not N.OutsideGroupChild.culled, "retained culled groups keep releasable descendant state")
		assert(GroupCulled(OutsideGroupChild), "coarse-culled group rejects descendant hit targets")
		assert(not Contains(OutsideGroupChild, 250, 120))
		assert(not Nodes(Fresh(Root)).OutsideGroupChild, "canonical culling prunes the hidden CanvasGroup texture instead of flattening its children")
	else
		assert(N.Outside.clipOwners[1] == N.Clip.id)
	end
	-- Nodes(Again).Partial == N.Partial: the unchanged export rebuilds nothing
	assert(#Export(Root).rebuilt == 0, "unchanged export reuses the cached node")
end
"#;

/// tools/tests/rotation_clipping.luau with ClipsDescendantsSupportsRotation (modern)
#[test]
fn rotation_clipping_modern() {
    run(Options { clips_descendants_supports_rotation: true, ..options(600.0, 500.0, 1.0) }, ROTATION_CLIPPING, &[("Modern", true)]);
}

/// tools/tests/rotation_clipping.luau without ClipsDescendantsSupportsRotation (legacy)
#[test]
fn rotation_clipping_legacy() {
    run(Options { clips_descendants_supports_rotation: false, ..options(600.0, 500.0, 1.0) }, ROTATION_CLIPPING, &[("Modern", false)]);
}

/// A Path2D is a node of its own under its parent's: its segments on the screen, its stroke, the box the stroke
/// reaches. It draws after its parent and before the parent's GuiObjects of its ZIndex wherever it sits among them,
/// follows its parent when the parent moves, and takes the clip its parent's children get.
const PATH2D: &str = r#"
local Root = Instance.new("ScreenGui", game:GetService("CoreGui"))
Root.IgnoreGuiInset = true
local Names = {}
local function Box(Parent, Name, X, Y, W, H)
	local F = Instance.new("Frame", Parent)
	F.Name = Name
	F.Position = UDim2.fromOffset(X, Y)
	F.Size = UDim2.fromOffset(W, H)
	Names[Id(F)] = Name
	return F
end
local function Path(Parent, Name, Points, Thickness)
	local P = Instance.new("Path2D")
	P.Name = Name
	P.Thickness = Thickness
	P.Color3 = Color3.new(1, 0.5, 0)
	P:SetControlPoints(Points)
	P.Parent = Parent
	Names[Id(P)] = Name
	return P
end
local Point, Offset = Path2DControlPoint.new, UDim2.fromOffset
local A = Box(Root, "A", 100, 50, 200, 100)
local Child = Box(A, "Child", 10, 10, 20, 20)
-- (made after Child: it still draws before it)
local Line = Path(A, "Line", { Point(Offset(0, 0)), Point(UDim2.new(1, 0, 0.5, 10)) }, 4)
local function Nodes(Result)
	local ByName = {}
	for _, Node in Result.nodes do ByName[Node.name] = Node end
	return ByName
end
local function Draw(Result)
	local Out = {}
	for _, Entry in Result.layers[1].draw do table.insert(Out, Names[Entry]) end
	return table.concat(Out, " ")
end

local Result = Check(Root, "first")
local N = Nodes(Result)
assert(N.Line.class == "Path2D" and N.Line.parent == N.A.id and N.Line.matrix == nil and N.Line.rotation == 0)
assert(Same(N.Line.path2d.segments, { { 100, 50, 300, 110 } }), "a line's two points on the screen")
assert(N.Line.path2d.thickness == 4 and N.Line.path2d.closed == nil)
assert(math.abs(N.Line.path2d.color[2] - 0.5) < 0.01 and N.Line.path2d.transparency == nil)
-- the curve's bounds grown by (Thickness + 1) / 2: what GetBoundingRect answers
local Rect = Line:GetBoundingRect()
assert(Same(N.Line.rect, { 97.5, 47.5, 205, 65 }), "the box the stroke reaches")
assert(Rect.Min == Vector2.new(97.5, 47.5) and Rect.Width == 205 and Rect.Height == 65)
assert(Same(N.Line.path2d.origin, { 97.5, 47.5 }))
assert(Draw(Result) == "A Line Child", Draw(Result))

-- ZIndex among the parent's children; equal paths in child order; a Global root sorts it among every descendant
Line.ZIndex = 2
assert(Draw(Check(Root, "above")) == "A Child Line")
Line.ZIndex = 1
local Curve = Path(A, "Curve", { Point(Offset(0, 100), Offset(0, 0), Offset(50, -50)), Point(Offset(100, 100), Offset(-50, -50), Offset(0, 0)), Point(Offset(200, 100)) }, 2)
Result = Check(Root, "two paths")
assert(Draw(Result) == "A Line Curve Child", Draw(Result))
assert(#Nodes(Result).Curve.path2d.segments == 2 and #Nodes(Result).Curve.path2d.segments[1] == 8 and #Nodes(Result).Curve.path2d.segments[2] == 4, "a cubic, then a line")
Curve.Closed = true
assert(#Nodes(Check(Root, "closed")).Curve.path2d.segments == 3 and Nodes(Check(Root, "closed")).Curve.path2d.closed == true)
Curve.Closed = false
Root.ZIndexBehavior = Enum.ZIndexBehavior.Global
Child.ZIndex = 0
Curve.ZIndex = 3
assert(Draw(Check(Root, "global")) == "Child A Line Curve", Draw(Check(Root, "global")))
Root.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
Child.ZIndex = 1
Curve.ZIndex = 1

-- unchanged: nothing is rebuilt; the parent moves: the path's node is, with its segments where the parent is
Check(Root, "settled")
assert(#Check(Root, "idle").rebuilt == 0, "an unchanged path is not rebuilt")
A.Position = UDim2.fromOffset(120, 50)
Result = Check(Root, "moved")
assert(table.find(Result.rebuilt, Id(Line)) and table.find(Result.rebuilt, Id(Curve)), "a path follows its parent")
assert(Same(Nodes(Result).Line.path2d.segments, { { 120, 50, 320, 110 } }))
-- its own points and properties
Line:UpdateControlPoint(2, Point(UDim2.new(1, 0, 1, 0)))
Result = Check(Root, "points")
assert(Same(Nodes(Result).Line.path2d.segments, { { 120, 50, 320, 150 } }) and not table.find(Result.rebuilt, Id(Curve)))
Line.Thickness = 10
assert(Nodes(Check(Root, "thickness")).Line.path2d.thickness == 10)
-- a turned parent turns the points about its centre: the node has no matrix
A.Rotation = 90
Result = Check(Root, "turned")
local Turned = Nodes(Result).Line.path2d.segments[1]
assert(math.abs(Turned[1] - 270) < 0.001 and math.abs(Turned[2] - 0) < 0.001 and math.abs(Turned[3] - 170) < 0.001 and math.abs(Turned[4] - 200) < 0.001, table.concat(Turned, " "))
assert(Nodes(Result).Line.matrix == nil)
A.Rotation = 0

-- not drawn: Visible false, or an invisible parent
Line.Visible = false
Result = Check(Root, "hidden")
assert(Draw(Result) == "A Curve Child" and Nodes(Result).Line.visible == false)
Line.Visible = true
A.Visible = false
assert(Draw(Check(Root, "parent hidden")) == "")
A.Visible = true

-- the clip of the parent's children: a clipping parent's rectangle, never a turned one's
A.ClipsDescendants = true
Result = Check(Root, "clipped")
assert(Same(Nodes(Result).Line.clip, { 120, 50, 200, 100 }) == Modern, "a clipping parent clips its path (not before rotated clipping)")
A.Rotation = 30
assert(Nodes(Check(Root, "turned clip")).Line.clip == nil, "a turned parent adds no clip")
A.Rotation = 0
A.ClipsDescendants = false

-- removed: its node goes
Curve.Parent = nil
Result = Check(Root, "removed")
assert(Nodes(Result).Curve == nil and Draw(Result) == "A Line Child")
-- a path of the root itself resolves against the screen
local Screen = Path(Root, "Screen", { Point(UDim2.fromScale(0, 0)), Point(UDim2.fromScale(1, 1)) }, 1)
Result = Check(Root, "screen")
assert(Same(Nodes(Result).Screen.path2d.segments, { { 0, 0, 600, 500 } }))
assert(Draw(Result) == "Screen A Line Child", Draw(Result))
"#;

#[test]
fn path2d_nodes_order_clip_and_rebuild() {
    run(Options { clips_descendants_supports_rotation: true, ..options(600.0, 500.0, 1.0) }, PATH2D, &[("Modern", true)]);
}

/// Before Roblox clipped rotated elements no ancestor clipped a Path2D (its announcement; notes/path2d.md)
#[test]
fn path2d_legacy_rotation_clipping_has_no_clip() {
    run(Options { clips_descendants_supports_rotation: false, ..options(600.0, 500.0, 1.0) }, PATH2D, &[("Modern", false)]);
}

