// Studio parity comparison: mount the same story/model in Roblox Studio (CoreGui, temporary),
// capture its viewport losslessly, render it offline at the same DPI and compare layout numbers
// (every GuiObject's rect / TextBounds) and pixels.

import { captureStretch, STRETCH_LIMIT } from "../tools/sweep/stretch.mjs";
import fs from "node:fs";
import { createHash } from "node:crypto";
import path from "node:path";
import { PNG } from "pngjs";
import { fileURLToPath } from "node:url";

import { runLuau, captureViewport, luauString, focusStudio, restoreFocus, ensureWholeViewport, DEFAULT_STUDIO } from "./studio.mjs";
import { withStudioLock } from "./lock.mjs";
import { OUT_DIR, sharedStatePath } from "./paths.mjs";
import { render } from "./rur.mjs";
import { CLOCK_LUAU, writeClock, clockSummary, withRecordedClock } from "./clock.mjs";

function storyInstancePath(file, roots) {
	const normalized = path.resolve(file).replace(/\\/g, "/");
	for (const root of roots) {
		const base = path.resolve(root).replace(/\\/g, "/") + "/";
		if (normalized.toLowerCase().startsWith(base.toLowerCase())) {
			const relative = normalized.slice(base.length);
			const parts = relative.split("/");
			const last = parts.pop().replace(/\.(server|client)?\.?luau?$/i, "").replace(/\.luau$/i, "");
			if (last !== "init") parts.push(last.replace(/\.lua$/, ""));
			return parts;
		}
	}
	throw new Error(`${file} is not under any project root`);
}

// Windows display scales tried, in order, when --dpi is not given (1.5 first: the usual one here)
const DISPLAY_SCALES = [1.5, 1, 1.25, 1.75, 2, 2.25, 2.5, 3, 3.5];

const MOUNT_LUAU = ({ pathParts, width, height, background, time, modelB64, fixtureSource, probes = {} }) => `
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")
local ContentProvider = game:GetService("ContentProvider")

local Old = CoreGui:FindFirstChild("__RUR_Compare")
if Old then
	pcall(function()
		if _G.__RUR_UNMOUNT then
			_G.__RUR_UNMOUNT()
		end
	end)
	Old:Destroy()
end

local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_Compare"
Gui.IgnoreGuiInset = true
Gui.DisplayOrder = 2000000000
Gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
Gui.ResetOnSpawn = false

local Backdrop = Instance.new("Frame")
Backdrop.Name = "Backdrop"
Backdrop.Size = UDim2.fromScale(1, 1)
Backdrop.BackgroundColor3 = Color3.fromRGB(${background.join(", ")})
Backdrop.BorderSizePixel = 0
Backdrop.ZIndex = -1000
Backdrop.Parent = Gui

local Target = Instance.new("Frame")
Target.Name = "StoryTarget"
Target.Size = UDim2.fromOffset(${width}, ${height})
Target.BackgroundTransparency = 1
Target.BorderSizePixel = 0
Target.Parent = Gui
Gui.Parent = CoreGui
-- rur.json compareProbes, read before the story mounts: module state Studio kept from earlier runs
local Probes = {}
${Object.entries(probes).map(([Name, Probe]) => `do
	local Ok, Value = pcall(function()
		${Probe.luau}
	end)
	Probes[${luauString(Name)}] = if Ok then Value else "error: " .. tostring(Value)
end`).join(String.fromCharCode(10))}
${CLOCK_LUAU({ time })}
local Status = "ok"
local Failure = nil
local PathParts = HttpService:JSONDecode(${luauString(JSON.stringify(pathParts))})
local Module = game
for _, Part in PathParts do
	Module = Module and Module:FindFirstChild(Part)
end

-- Studio keeps require() results for the whole session: a module edited or synced after its first
-- require (a pull) would still mount its old code and state. Every mount loads the story and what it
-- requires from their current Source instead, once each, like the offline run's fresh VM.
local FreshModules = {}
-- require("./x"), "../x", "@self/x", "@game/x": resolved against the requiring module (a loadstring
-- chunk has no requiring context of its own)
local function ResolvePath(From, Path)
	local Parts = string.split(Path, "/")
	local Current
	local First = Parts[1]
	if First == "." then
		Current = From.Parent
	elseif First == ".." then
		Current = From.Parent and From.Parent.Parent
	elseif First == "@self" then
		Current = From
	elseif First == "@game" then
		Current = game
	else
		error("unsupported require path " .. Path .. " in " .. From:GetFullName(), 3)
	end
	for Index = 2, #Parts do
		local Part = Parts[Index]
		if Part == ".." then
			Current = Current and Current.Parent
		elseif Part ~= "." and Part ~= "" then
			Current = Current and (Current:FindFirstChild(Part) or (Current == game and game:FindService(Part)))
		end
		if not Current then
			error("require path " .. Path .. " not found from " .. From:GetFullName(), 3)
		end
	end
	return Current
end
local FreshRequire
local function RequireFrom(From)
	return function(Target)
		if type(Target) == "string" then
			Target = ResolvePath(From, Target)
		end
		return FreshRequire(Target)
	end
end
function FreshRequire(Target)
	if typeof(Target) ~= "Instance" or not Target:IsA("ModuleScript") then
		return require(Target)
	end
	local Entry = FreshModules[Target]
	if Entry then
		if Entry.loading then
			error("cyclic require of " .. Target:GetFullName(), 2)
		end
		return Entry.value
	end
	Entry = { loading = true }
	FreshModules[Target] = Entry
	local Chunk, CompileError = loadstring(Target.Source, "=" .. Target:GetFullName())
	if not Chunk then
		error(CompileError, 0)
	end
	local Environment = setmetatable({ script = Target, require = RequireFrom(Target) }, { __index = getfenv(0) })
	setfenv(Chunk, Environment)
	local Value = Chunk()
	Entry.value = Value
	Entry.loading = false
	return Value
end

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

local ModelB64 = ${luauString(modelB64 || "")}
local FixtureBuilder = ${fixtureSource ? `(function()
${fixtureSource}
end)()` : "nil"}
if FixtureBuilder then
	local Ok, Error = pcall(function()
		__RUR_BeginMount()
		local Result = FixtureBuilder(Target)
		if typeof(Result) == "Instance" and Result.Parent == nil then
			Result.Parent = Target
		end
	end)
	if not Ok then
		Status = "error"
		Failure = tostring(Error)
	end
elseif ModelB64 ~= "" then
	local Ok, Error = pcall(function()
		local Alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
		local Lookup = {}
		for Index = 1, 64 do
			Lookup[string.byte(Alphabet, Index)] = Index - 1
		end
		local Clean = string.gsub(ModelB64, "[^%w%+/]", "")
		local Out = buffer.create(math.floor(#Clean * 3 / 4))
		local Write = 0
		for Index = 1, #Clean, 4 do
			local A = Lookup[string.byte(Clean, Index)] or 0
			local B = Lookup[string.byte(Clean, Index + 1) or 65] or 0
			local C = Lookup[string.byte(Clean, Index + 2) or 65]
			local D = Lookup[string.byte(Clean, Index + 3) or 65]
			local N = A * 262144 + B * 4096 + (C or 0) * 64 + (D or 0)
			if Write < buffer.len(Out) then
				buffer.writeu8(Out, Write, bit32.rshift(N, 16))
				Write += 1
			end
			if C and Write < buffer.len(Out) then
				buffer.writeu8(Out, Write, bit32.band(bit32.rshift(N, 8), 255))
				Write += 1
			end
			if D and Write < buffer.len(Out) then
				buffer.writeu8(Out, Write, bit32.band(N, 255))
				Write += 1
			end
		end
		local Decoded = Out
		local Roots = game:GetService("SerializationService"):DeserializeInstancesAsync(Decoded)
		for _, Root in Roots do
			if Root:IsA("LayerCollector") then
				for _, Child in Root:GetChildren() do
					Child.Parent = Target
				end
			else
				Root.Parent = Target
			end
		end
	end)
	if not Ok then
		Status = "error"
		Failure = tostring(Error)
	end
elseif not Module or not Module:IsA("ModuleScript") then
	Status = "error"
	Failure = "story module not found in Studio: " .. table.concat(PathParts, ".")
else
	local Ok, Error = pcall(function()
		local Story = FreshRequire(Module)
		__RUR_BeginMount()
		if type(Story) == "table" and Story.story then
			local Vide = Story.vide
			local Controls = ConvertControls(Story.controls, if Vide then Vide.source else function(Value)
				return Value
			end)
			local Props = { controls = Controls, target = Target, converted = Story.controls }
			if Vide then
				_G.__RUR_UNMOUNT = Vide.mount(function()
					return Story.story(Props)
				end, Target)
			else
				local Result = Story.story(Props)
				if typeof(Result) == "Instance" then
					Result.Parent = Target
				end
			end
		elseif type(Story) == "function" then
			_G.__RUR_UNMOUNT = Story(Target)
		end
	end)
	if not Ok then
		Status = "error"
		Failure = tostring(Error)
	end
end
__RUR_StartClock(Target)

-- The captured moment (cli/clock.mjs): the frame clock stops at task.wait(time) after the build and a
-- frozen clone of the target shows that moment; the layout dump and the viewport capture both read the
-- clone once its images are loaded
local Clock = __RUR_Capture(${time})
local Captured = __RUR_Freeze(Target)
pcall(function()
	ContentProvider:PreloadAsync({ Captured or Target })
end)
task.wait(0.25)

local Nodes = {}
-- Hidden: under an invisible GuiObject (the offline export stops at the invisible node itself)
local function Visit(Node, RelativePath, Hidden)
	for Index, Child in Node:GetChildren() do
		if Child:IsA("GuiBase2d") then
			local Entry = {
				hidden = Hidden or nil,
				path = RelativePath .. "/" .. Child.Name,
				class = Child.ClassName,
				x = Child.AbsolutePosition.X,
				y = Child.AbsolutePosition.Y,
				w = Child.AbsoluteSize.X,
				h = Child.AbsoluteSize.Y,
				r = Child.AbsoluteRotation,
				visible = if Child:IsA("GuiObject") then Child.Visible else true,
			}
			if Child:IsA("TextLabel") or Child:IsA("TextButton") or Child:IsA("TextBox") then
				Entry.tbx = Child.TextBounds.X
				Entry.tby = Child.TextBounds.Y
				Entry.text = Child.Text
			end
			if Child:IsA("ScrollingFrame") then
				Entry.cw = Child.AbsoluteCanvasSize.X
				Entry.ch = Child.AbsoluteCanvasSize.Y
			end
			-- Visual state, for property diffs against the offline render
			local Props = {}
			local function Color3Of(Value)
				return { math.floor(Value.R * 255 + 0.5), math.floor(Value.G * 255 + 0.5), math.floor(Value.B * 255 + 0.5) }
			end
			if Child:IsA("GuiObject") then
				Props.bgColor = Color3Of(Child.BackgroundColor3)
				Props.bgTransparency = Child.BackgroundTransparency
			end
			if Child:IsA("ImageLabel") or Child:IsA("ImageButton") then
				Props.imageColor = Color3Of(Child.ImageColor3)
				Props.imageTransparency = Child.ImageTransparency
				Props.image = Child.Image
			end
			if Child:IsA("TextLabel") or Child:IsA("TextButton") or Child:IsA("TextBox") then
				-- An empty TextBox draws its placeholder in PlaceholderColor3
				local Placeholder = Child:IsA("TextBox") and Child.Text == ""
				Props.textColor = Color3Of(if Placeholder then Child.PlaceholderColor3 else Child.TextColor3)
				Props.textTransparency = Child.TextTransparency
			end
			if Child:IsA("CanvasGroup") then
				Props.groupColor = Color3Of(Child.GroupColor3)
				Props.groupTransparency = Child.GroupTransparency
			end
			for _, Component in Child:GetChildren() do
				if Component:IsA("UIGradient") then
					Props.gradient = { enabled = Component.Enabled, rotation = Component.Rotation, offset = { Component.Offset.X, Component.Offset.Y } }
				elseif Component:IsA("UIStroke") then
					Props.strokes = Props.strokes or {}
					table.insert(Props.strokes, { enabled = Component.Enabled, mode = Component.ApplyStrokeMode.Name, thickness = Component.Thickness, transparency = Component.Transparency, color = Color3Of(Component.Color), zIndex = Component.ZIndex })
				elseif Component:IsA("UIScale") then
					Props.uiScale = Component.Scale
				end
			end
			Entry.props = Props
			table.insert(Nodes, Entry)
			Visit(Child, Entry.path, Hidden or not Entry.visible)
		elseif Child:IsA("Path2D") then
			-- A path has no Absolute* members: its rectangle is the box its stroke reaches. One with fewer than
			-- two points reaches nothing (GetBoundingRect answers every number there is): its parent's corner
			local Found, Bounds = pcall(Child.GetBoundingRect, Child)
			local Origin = Node.AbsolutePosition
			local Reaches = Found and Bounds.Width < 1e30
			table.insert(Nodes, {
				hidden = Hidden or nil,
				path = RelativePath .. "/" .. Child.Name,
				class = Child.ClassName,
				x = if Reaches then Bounds.Min.X else Origin.X,
				y = if Reaches then Bounds.Min.Y else Origin.Y,
				w = if Reaches then Bounds.Width else 0,
				h = if Reaches then Bounds.Height else 0,
				r = 0,
				visible = Child.Visible,
				props = { pathThickness = Child.Thickness, pathColor = { math.floor(Child.Color3.R * 255 + 0.5), math.floor(Child.Color3.G * 255 + 0.5), math.floor(Child.Color3.B * 255 + 0.5) } },
			})
		elseif Child:IsA("Folder") then
			Visit(Child, RelativePath .. "/" .. Child.Name, Hidden)
		end
	end
end
Visit(Captured or Target, "")

local Camera = workspace.CurrentCamera
return HttpService:JSONEncode({
	status = Status,
	failure = Failure,
	viewport = { Camera.ViewportSize.X, Camera.ViewportSize.Y },
	placeId = game.PlaceId,
	localPlayer = (function()
		local Player = game:GetService("Players").LocalPlayer
		return if Player then { id = Player.UserId, name = Player.Name, display = Player.DisplayName } else nil
	end)(),
	target = { Target.AbsolutePosition.X, Target.AbsolutePosition.Y, Target.AbsoluteSize.X, Target.AbsoluteSize.Y },
	nodes = Nodes,
	clock = Clock,
	frozen = Captured ~= nil,
	probes = Probes,
})
`;

const UNMOUNT_LUAU = `
local CoreGui = game:GetService("CoreGui")
pcall(function()
	if _G.__RUR_UNMOUNT then
		_G.__RUR_UNMOUNT()
	end
end)
_G.__RUR_UNMOUNT = nil
local Old = CoreGui:FindFirstChild("__RUR_Compare")
if Old then
	Old:Destroy()
end
return "unmounted"
`;

function readPng(file) {
	return PNG.sync.read(fs.readFileSync(file));
}

function writePng(file, png) {
	fs.writeFileSync(file, PNG.sync.write(png));
}

// The viewport capture reads Studio's RGB(13, 11, 12) as transparent black: PrintWindow treats it as the
// window's colour key. Measured over 122 captures (265 M pixels): that exact colour never appears while
// every neighbour does, and alpha 0 occurs only on those (0, 0, 0) pixels, where we render (13, 11, 12)
// or a colour within 2 of it. Restores the colour so such pixels compare like any other.
export function restoreColourKey(png) {
	let restored = 0;
	for (let k = 0; k < png.data.length; k += 4) {
		if (png.data[k + 3] === 0) {
			png.data[k] = 13;
			png.data[k + 1] = 11;
			png.data[k + 2] = 12;
			png.data[k + 3] = 255;
			restored++;
		}
	}
	return restored;
}

// Runs `fn` with Studio's LocalPlayer identity in the environment the offline runtime reads
// (RUR_PLAYER / RUR_PLAYER_ID / RUR_PLAYER_DISPLAY, native/src/engine/cli.rs); `player` = { name, id, display }.
export async function withStudioPlayer(player, fn) {
	const keys = { RUR_PLAYER: player && player.name, RUR_PLAYER_ID: player && player.id, RUR_PLAYER_DISPLAY: player && player.display };
	const previous = {};
	for (const [key, value] of Object.entries(keys)) {
		previous[key] = process.env[key];
		if (value !== undefined && value !== null) process.env[key] = String(value);
		else delete process.env[key];
	}
	try {
		return await fn();
	} finally {
		for (const [key, value] of Object.entries(previous)) {
			if (value === undefined) delete process.env[key];
			else process.env[key] = value;
		}
	}
}

// Studio's sibling order, replayed by the offline run (RUR_ORDER, native/src/engine/order.rs): lists built
// from tables keyed by tables or instances come out in an address-dependent order on both sides
export async function withStudioOrder(file, fn) {
	const previous = process.env.RUR_ORDER;
	if (file) process.env.RUR_ORDER = file;
	else delete process.env.RUR_ORDER;
	try {
		return await fn();
	} finally {
		if (previous === undefined) delete process.env.RUR_ORDER;
		else process.env.RUR_ORDER = previous;
	}
}

function crop(png, x, y, w, h) {
	const out = new PNG({ width: w, height: h });
	for (let row = 0; row < h; row++) {
		const src = ((row + y) * png.width + x) * 4;
		png.data.copy(out.data, row * w * 4, src, src + w * 4);
	}
	return out;
}

// Pixel comparison: per-pixel max channel difference.
export function comparePngs(a, b, diffFile) {
	const w = Math.min(a.width, b.width), h = Math.min(a.height, b.height);
	const diff = new PNG({ width: w, height: h });
	let over2 = 0, over8 = 0, over32 = 0, total = 0, sumAbs = 0, sumSq = 0;
	for (let y = 0; y < h; y++) {
		for (let x = 0; x < w; x++) {
			const ia = (y * a.width + x) * 4, ib = (y * b.width + x) * 4, id = (y * w + x) * 4;
			let max = 0;
			for (let c = 0; c < 3; c++) {
				const d = Math.abs(a.data[ia + c] - b.data[ib + c]);
				if (d > max) max = d;
				sumAbs += d;
				sumSq += d * d;
			}
			total++;
			if (max > 2) over2++;
			if (max > 8) over8++;
			if (max > 32) over32++;
			const gray = Math.round((a.data[ia] + a.data[ia + 1] + a.data[ia + 2]) / 3 * 0.35);
			diff.data[id] = max > 8 ? 255 : gray;
			diff.data[id + 1] = max > 8 ? Math.max(0, 80 - max) : gray;
			diff.data[id + 2] = max > 8 ? Math.max(0, 80 - max) : gray;
			diff.data[id + 3] = 255;
		}
	}
	if (diffFile) writePng(diffFile, diff);
	const mse = sumSq / (total * 3);
	return {
		width: w,
		height: h,
		exactPercent: 100 * (1 - over2 / total),
		within8Percent: 100 * (1 - over8 / total),
		within32Percent: 100 * (1 - over32 / total),
		meanAbsError: sumAbs / (total * 3),
		psnr: mse === 0 ? Infinity : 10 * Math.log10(255 * 255 / mse),
	};
}

function sideBySide(a, b, file) {
	const gap = 8;
	const out = new PNG({ width: a.width + b.width + gap, height: Math.max(a.height, b.height) });
	out.data.fill(255);
	for (let y = 0; y < a.height; y++) a.data.copy(out.data, (y * out.width) * 4, y * a.width * 4, (y + 1) * a.width * 4);
	for (let y = 0; y < b.height; y++) b.data.copy(out.data, (y * out.width + a.width + gap) * 4, y * b.width * 4, (y + 1) * b.width * 4);
	writePng(file, out);
}

// Path of every exported node below the story target, "/"-joined like the Studio dump. Built from the
// parent chain with each node's own name, so names containing dots ("14.02") stay whole; only the
// non-GUI containers between a node and its exported parent (Folders) are split from the dotted path.
export function relativePaths(scene) {
	const byId = new Map(scene.nodes.map(node => [node.id, node]));
	const targetNode = scene.nodes.find(node => node.name === "StoryTarget");
	const paths = new Map();
	if (targetNode) {
		paths.set(targetNode.id, "");
	}
	// A mounted model's LayerCollector (a ScreenGui at the model's root) lands beside the story in the
	// offline run, while Studio mounts its children into the target (MOUNT_LUAU): its children are the
	// target's
	for (const node of scene.nodes) {
		if (/^CoreGui\.[^.]+$/.test(node.path) && node.name !== "RURStory" && /Gui$/.test(node.class)) {
			paths.set(node.id, "");
		}
	}
	const relativeOf = node => {
		if (paths.has(node.id)) {
			return paths.get(node.id);
		}
		const parent = byId.get(node.parent);
		let result = null;
		if (parent && node.path.startsWith(parent.path + ".") && node.path.endsWith("." + node.name)) {
			const base = relativeOf(parent);
			if (base !== null) {
				const middle = node.path.slice(parent.path.length + 1, node.path.length - node.name.length - 1);
				result = `${base}${middle ? "/" + middle.split(".").join("/") : ""}/${node.name}`;
			}
		}
		paths.set(node.id, result);
		return result;
	};
	for (const node of scene.nodes) {
		relativeOf(node);
	}
	return paths;
}

// A failed offline run exports no nodes (an empty table encodes as {}): compare against none
function readScene(file) {
	const scene = JSON.parse(fs.readFileSync(file, "utf8"));
	if (!Array.isArray(scene.nodes)) {
		scene.nodes = [];
	}
	return scene;
}

// Layout comparison: match nodes by relative path (+ occurrence index for duplicates).
export function compareLayout(studioNodes, scene, dpi, targetOffset) {
	const ours = [];
	const byId = new Map(scene.nodes.map(node => [node.id, node]));
	const relative = relativePaths(scene);
	for (const node of scene.nodes) {
		const path = relative.get(node.id);
		if (path) {
			ours.push({ node, relative: path });
		}
	}
	const index = new Map();
	for (const entry of ours) {
		if (!index.has(entry.relative)) index.set(entry.relative, []);
		index.get(entry.relative).push(entry.node);
	}
	// Same-path siblings (several children with one name) pair by geometry, nearest first: their child
	// order can differ between Studio and the offline tree while the set of rects is the same
	const rectError = (studio, node) => {
		const [x, y] = node.absolute || node.rect;
		const [, , w, h] = node.rect;
		return Math.max(Math.abs(x - (studio.x - targetOffset[0])), Math.abs(y - (studio.y - targetOffset[1])), Math.abs(w - studio.w), Math.abs(h - studio.h));
	};
	const pairs = new Map();
	const byPath = new Map();
	for (const studio of studioNodes) {
		if (!studio.hidden) {
			if (!byPath.has(studio.path)) byPath.set(studio.path, []);
			byPath.get(studio.path).push(studio);
		}
	}
	for (const [pathKey, group] of byPath) {
		const list = index.get(pathKey) || [];
		if (group.length === 1 || list.length <= 1) {
			group.forEach((studio, k) => pairs.set(studio, list[k]));
			continue;
		}
		const candidates = [];
		for (const studio of group) {
			list.forEach((node, k) => candidates.push({ studio, k, error: rectError(studio, node) }));
		}
		candidates.sort((a, b) => a.error - b.error);
		const taken = new Set();
		for (const { studio, k } of candidates) {
			if (!pairs.has(studio) && !taken.has(k)) {
				pairs.set(studio, list[k]);
				taken.add(k);
			}
		}
	}

	const rows = [];
	let matched = 0, exact = 0, within1 = 0, propMismatches = 0, hidden = 0, contentMismatches = 0;
	const studioRect = studio => [studio.x - targetOffset[0], studio.y - targetOffset[1], studio.w, studio.h].map(v => +v.toFixed(3));
	for (const studio of studioNodes) {
		// Descendants of invisible nodes draw nothing and are not exported offline
		if (studio.hidden) {
			hidden++;
			continue;
		}
		const node = pairs.get(studio);
		if (!node) {
			rows.push({ path: studio.path, class: studio.class, missing: true, visible: studio.visible, studio: studioRect(studio) });
			continue;
		}
		matched++;
		// node.absolute: AbsolutePosition as scripts see it (differs from the unrotated rect under rotated ancestors)
		const [x, y] = node.absolute || node.rect;
		const [, , w, h] = node.rect;
		const dx = Math.max(Math.abs(x - (studio.x - targetOffset[0])), Math.abs(y - (studio.y - targetOffset[1])));
		const ds = Math.max(Math.abs(w - studio.w), Math.abs(h - studio.h));
		const error = Math.max(dx, ds);
		if (error < 0.01) exact++;
		if (error <= 1) within1++;
		const row = { path: studio.path, class: studio.class, error: +error.toFixed(3), studio: [studio.x - targetOffset[0], studio.y - targetOffset[1], studio.w, studio.h].map(v => +v.toFixed(3)), ours: [x, y, w, h] };
		if (studio.tbx !== undefined && node.text) {
			row.textBounds = { studio: [studio.tbx, studio.tby], ours: [node.text.width / dpi, node.text.height / dpi] };
			row.textError = Math.max(Math.abs(studio.tbx - node.text.width / dpi), Math.abs(studio.tby - node.text.height / dpi));
		}
		if (studio.props) {
			const diffs = propertyDiffs(studio.props, node);
			if (diffs.length) {
				row.props = diffs;
				propMismatches++;
			}
		}
		// Content the offline run produced differently (text, image asset): not a rendering difference
		const content = contentDiffs(studio, node);
		if (content.length) {
			row.content = content;
			contentMismatches++;
		}
		rows.push(row);
	}

	// Offline nodes without a Studio counterpart; hiddenInStudio: Studio has the path under an invisible
	// ancestor (a visibility state that differs)
	const paired = new Set(pairs.values());
	const hiddenPaths = new Set(studioNodes.filter(studio => studio.hidden).map(studio => studio.path));
	let extra = 0;
	for (const { node, relative } of ours) {
		if (paired.has(node)) {
			continue;
		}
		extra++;
		const [x, y] = node.absolute || node.rect;
		rows.push({ path: relative, class: node.class, extra: true, visible: node.visible, hiddenInStudio: hiddenPaths.has(relative) || undefined, ours: [x, y, node.rect[2], node.rect[3]] });
	}
	void byId;
	const weight = row => row.error || (row.missing || row.extra ? 1e9 : 0);
	rows.sort((a, b) => weight(b) - weight(a));
	return { studioNodes: studioNodes.length - hidden, hiddenStudioNodes: hidden, ourNodes: ours.length, matched, exact, within1, propMismatches, contentMismatches, extraNodes: extra, rows };
}

// Asset ids compare by number ("rbxassetid://1" = "http://www.roblox.com/asset/?id=1")
function assetKey(uri) {
	const text = String(uri || "");
	const id = text.match(/(?:rbxassetid:\/\/|[?&]id=)(\d+)/i);
	return id ? id[1] : text.trim().toLowerCase();
}

// Text / image content that differs between Studio's dump and the offline export.
function contentDiffs(studio, node) {
	const diffs = [];
	if (studio.text !== undefined && node.props && node.props.text !== undefined && studio.text !== node.props.text) {
		diffs.push({ prop: "Text", studio: studio.text, ours: node.props.text });
	}
	if (studio.props && studio.props.image !== undefined) {
		const ours = node.image ? node.image.source || node.image.uri : "";
		if (assetKey(studio.props.image) !== assetKey(ours)) {
			diffs.push({ prop: "Image", studio: studio.props.image, ours });
		}
	}
	return diffs;
}

// Visual property differences between a Studio node dump and an exported scene node.
function propertyDiffs(props, node) {
	const diffs = [];
	const color255 = c => (c ? c.map(v => Math.round(v * 255)) : null);
	const near = (a, b, tolerance) => Math.abs((a ?? 0) - (b ?? 0)) <= tolerance;
	const nearColor = (a, b) => !a || !b || a.every((v, i) => Math.abs(v - b[i]) <= 1);
	const check = (name, studioValue, ourValue, ok) => {
		if (!ok) diffs.push({ prop: name, studio: studioValue, ours: ourValue });
	};
	if (props.bgTransparency !== undefined && node.bg) {
		check("BackgroundTransparency", props.bgTransparency, node.bg.transparency, near(props.bgTransparency, node.bg.transparency, 0.005));
		if (props.bgTransparency < 0.999) check("BackgroundColor3", props.bgColor, color255(node.bg.color), nearColor(props.bgColor, color255(node.bg.color)));
	}
	// An ImageLabel with no image draws nothing (the export has no node.image): nothing to compare
	if (props.imageTransparency !== undefined && node.image) {
		const image = node.image;
		check("ImageTransparency", props.imageTransparency, image.transparency ?? 0, near(props.imageTransparency, image.transparency ?? 0, 0.005));
		if (props.imageTransparency < 0.999) check("ImageColor3", props.imageColor, color255(image.color || [1, 1, 1]), nearColor(props.imageColor, color255(image.color || [1, 1, 1])));
	}
	if (props.textTransparency !== undefined && node.text) {
		check("TextTransparency", props.textTransparency, node.text.transparency ?? 0, near(props.textTransparency, node.text.transparency ?? 0, 0.005));
		if (props.textTransparency < 0.999) check("TextColor3", props.textColor, color255(node.text.color), nearColor(props.textColor, color255(node.text.color)));
	}
	if (props.pathThickness !== undefined && node.path2d) {
		check("Thickness", props.pathThickness, node.path2d.thickness, near(props.pathThickness, node.path2d.thickness, 0.005));
		check("Color3", props.pathColor, color255(node.path2d.color), nearColor(props.pathColor, color255(node.path2d.color)));
	}
	if (props.groupTransparency !== undefined) {
		const group = node.group || {};
		check("GroupTransparency", props.groupTransparency, group.transparency ?? 0, near(props.groupTransparency, group.transparency ?? 0, 0.005));
		check("GroupColor3", props.groupColor, color255(group.color || [1, 1, 1]), nearColor(props.groupColor, color255(group.color || [1, 1, 1])));
	}
	if (props.gradient && props.gradient.enabled) {
		const gradient = node.gradient;
		const offset = (gradient && gradient.offset) || [0, 0];
		check("UIGradient", props.gradient, gradient ? { rotation: gradient.rotation, offset } : null,
			Boolean(gradient) && near(props.gradient.rotation, gradient.rotation, 0.05) && near(props.gradient.offset[0], offset[0], 0.002) && near(props.gradient.offset[1], offset[1], 0.002));
	}
	// Strokes pair up by ApplyStrokeMode in ZIndex order (child order among equals), the order our export
	// lists them in; Studio dumps them in child order
	const ours = [...(node.strokes || [])];
	const studioStrokes = (props.strokes || []).map((stroke, index) => ({ stroke, index }));
	studioStrokes.sort((a, b) => (a.stroke.zIndex ?? 1) - (b.stroke.zIndex ?? 1) || a.index - b.index);
	for (const { stroke: studioStroke } of studioStrokes) {
		if (!studioStroke.enabled) {
			continue;
		}
		const index = ours.findIndex(stroke => stroke.mode === studioStroke.mode);
		const stroke = index >= 0 ? ours.splice(index, 1)[0] : null;
		check(`UIStroke (${studioStroke.mode})`, { transparency: studioStroke.transparency, color: studioStroke.color }, stroke ? { transparency: stroke.transparency, color: color255(stroke.color || [1, 1, 1]) } : null,
			Boolean(stroke) && near(studioStroke.transparency, stroke.transparency, 0.005) && nearColor(studioStroke.color, color255(stroke.color || [1, 1, 1])));
	}
	return diffs;
}

// Whether Studio's viewport projection is stretched right now (tools/sweep/stretch.mjs): the 60-label placement
// probe captured on its own (always recaptured: a reused capture of the unchanged probe would report the stretch
// state of whenever it was first captured), fitted for dy growth; reused for 2 minutes so a batch of compares
// pays once.
// Returns { overHeight, labels, stretched } or null when the probe could not be captured.
async function captureCalibration(studio) {
	const dir = path.join(OUT_DIR, "compare", "_calibration");
	const cacheFile = path.join(dir, "calibration.json");
	try {
		const cached = JSON.parse(fs.readFileSync(cacheFile, "utf8"));
		if (Date.now() - cached.at < 120000 && cached.studio === studio) return cached.result;
	} catch {}
	try {
		await compare({ fixture: path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "fixtures", "text-probe-place.luau"), viewport: "1200x780", time: 0.2, background: "0,0,0", out: dir, studio, rootsResolved: calibrationRoots, "no-calibrate": true, recapture: true });
	} catch {
		return null;
	}
	const stretch = captureStretch(dir);
	const result = stretch ? { overHeight: stretch.overHeight, labels: stretch.labels, stretched: stretch.overHeight >= STRETCH_LIMIT } : null;
	fs.writeFileSync(cacheFile, JSON.stringify({ at: Date.now(), studio, result }));
	return result;
}
let calibrationRoots = null;

// rur.json at the project root (the parent of the roots): { "compareProbes": { name: { luau, fresh } } }
function compareProbes(roots) {
	try {
		const config = JSON.parse(fs.readFileSync(path.join(path.dirname(path.resolve(roots[0])), "rur.json"), "utf8"));
		return config && typeof config.compareProbes === "object" ? config.compareProbes : {};
	} catch {
		return {};
	}
}

// The probes whose Studio value differs from a fresh session's, e.g. { usePxScale: { studio: 1.068, fresh: 1 } }
function staleProbes(probes, values = {}) {
	const stale = {};
	for (const [Name, Probe] of Object.entries(probes)) {
		const value = values[Name];
		if ("fresh" in Probe && value !== undefined && value !== Probe.fresh) stale[Name] = { studio: value, fresh: Probe.fresh };
	}
	if (Object.keys(stale).length) {
		process.stderr.write(`rur compare: Studio kept state from earlier runs (${Object.entries(stale).map(([Name, V]) => `${Name} = ${V.studio}, fresh ${V.fresh}`).join("; ")}); reopen the place for a clean capture
`);
	}
	return Object.keys(stale).length ? stale : undefined;
}

// Studio captures are reused (studio-capture.json beside studio.png) while nothing the capture depends on
// changed: the mount script Studio runs (story path, viewport, background, time, probes, fixture / model
// bytes), the Studio instance and DPI, and the contents of every source file the story loaded (the
// offline run's dependencies.json, the story included). A capture with a genuine issue is never reused:
// Studio failed or kept state from earlier runs, the projection was stretched, the offline run did not
// finish (its dependency list may be incomplete), or a calibration later marked it (invalidateCapture).
// --recapture always captures; --reuse-only never touches Studio (it throws CaptureNeeded instead).
const CAPTURE_VERSION = 1;
const sha1 = data => createHash("sha1").update(data).digest("hex");
const fileHash = file => {
	try {
		return sha1(fs.readFileSync(file));
	} catch {
		return null;
	}
};

function reusableCapture(outDir, key) {
	let record;
	try {
		record = JSON.parse(fs.readFileSync(path.join(outDir, "studio-capture.json"), "utf8"));
	} catch {
		return { reason: "no earlier capture" };
	}
	if (record.version !== CAPTURE_VERSION || record.key !== key) return { reason: "the mount (story, options or probes) changed" };
	if (!record.valid) return { reason: `the earlier capture is not reusable (${(record.issues || []).join("; ") || "marked invalid"})` };
	for (const file of ["studio.png", "studio-nodes.json"]) {
		if (!fs.existsSync(path.join(outDir, file))) return { reason: `${file} is missing` };
	}
	for (const [file, hash] of Object.entries(record.dependencies || {})) {
		if (fileHash(file) !== hash) return { reason: `${path.basename(file)} changed` };
	}
	return { record };
}

// Marks a directory's Studio capture as not reusable (e.g. a calibration found Studio stretched)
export function invalidateCapture(outDir, issue) {
	const recordFile = path.join(outDir, "studio-capture.json");
	try {
		const record = JSON.parse(fs.readFileSync(recordFile, "utf8"));
		record.valid = false;
		record.issues = [...(record.issues || []), issue];
		fs.writeFileSync(recordFile, JSON.stringify(record, null, 1));
	} catch {
		// no record: nothing would be reused
	}
}

export class CaptureNeeded extends Error {
	constructor(reason) {
		super(`a Studio capture is needed (${reason})`);
		this.reason = reason;
	}
}

export async function compare(args) {
	const [width, height] = String(args.viewport || "1200x900").split("x").map(Number);
	const background = args.background ? String(args.background).split(",").map(Number) : [46, 46, 46];
	const time = Number(args.time || 1);
	const studio = args.studio || DEFAULT_STUDIO;
	const roots = String(args.rootsResolved).split(";");
	const source = args.story || args.model || args.fixture;
	const name = path.basename(String(source)).replace(/\.(story\.)?luau$|\.rbxmx?$/, "");
	const outDir = path.resolve(args.out || path.join(OUT_DIR, "compare", name));
	fs.mkdirSync(outDir, { recursive: true });

	let modelB64 = "";
	let pathParts = [];
	let fixtureSource = null;
	if (args.model) modelB64 = fs.readFileSync(path.resolve(String(args.model))).toString("base64");
	else if (args.fixture) fixtureSource = fs.readFileSync(path.resolve(String(args.fixture)), "utf8");
	else pathParts = storyInstancePath(String(args.story), roots);

	// rur.json compareProbes: Luau snippets Studio evaluates before mounting, with the value a fresh session
	// has. Studio keeps required modules loaded between compares, so a story can inherit state an earlier
	// one left (a UI kit's usePx: its px scale follows the camera for the rest of the session once any story
	// calls usePx(); a fresh session, like the offline run, has 1). Differences are reported, not hidden.
	const probes = compareProbes(roots);
	// Studio's display scale: --dpi, or detected from its viewport window (the monitor it is on)
	let dpi = args.dpi ? Number(args.dpi) : undefined;
	const mountLuau = MOUNT_LUAU({ pathParts, width, height, background, time, modelB64, fixtureSource, probes });
	const captureKey = sha1(JSON.stringify({ studio, dpi: dpi || "auto", mount: sha1(mountLuau) }));
	const reuse = args.recapture ? { reason: "--recapture" } : reusableCapture(outDir, captureKey);
	if (!reuse.record && args["reuse-only"]) throw new CaptureNeeded(reuse.reason);
	const dumpFile = path.join(outDir, "studio-nodes.json");
	const studioFile = path.join(outDir, "studio.png");
	let studioResult, calibration, colourKeyed, studioPng;
	if (reuse.record) {
		// The earlier capture, as it was measured
		const dump = JSON.parse(fs.readFileSync(dumpFile, "utf8"));
		studioResult = { ...reuse.record.studio, target: dump.target, nodes: dump.nodes };
		dpi = dpi || studioResult.dpi || 1.5;
		calibration = reuse.record.calibration || undefined;
		colourKeyed = reuse.record.colourKeyed;
		studioPng = readPng(studioFile);
		process.stderr.write(`rur compare: reusing the Studio capture from ${reuse.record.capturedAt} (its inputs are unchanged; --recapture captures again)
`);
	} else {
		if (reuse.reason !== "no earlier capture") process.stderr.write(`rur compare: capturing in Studio: ${reuse.reason}
`);
		// Studio's projection stretch (notes/text.md "Capture environment"), unless disabled or this is the probe
		calibrationRoots = args.rootsResolved;
		calibration = args["no-calibrate"] ? undefined : await captureCalibration(studio);
		if (calibration && calibration.stretched) {
			process.stderr.write(`rur compare: Studio's viewport projection is stretched (text dy grows ${calibration.overHeight} px over 1350 px); text and images in this capture are blurred
`);
		}
		// Studio has a single viewport: mount + capture + unmount must not interleave with other runs
		const capturePath = path.join(outDir, "studio-full.png");
		// The scale the earlier capture was taken at: a capture at another scale (Studio moved to the other
		// monitor) must not replace it unless asked (--recapture)
		let previousDpi = null, previousRecord = null;
		try {
			previousRecord = fs.readFileSync(path.join(outDir, "studio-capture.json"), "utf8");
			previousDpi = JSON.parse(previousRecord).studio.dpi || null;
		} catch {
			// no earlier capture
		}
		fs.rmSync(path.join(outDir, "studio-capture.json"), { force: true });
		studioResult = await withStudioLock(`compare ${name}`, async () => {
			// A fractional viewport size blurs the capture: Studio's window is nudged to a whole one first
			// (cli/studio.mjs ensureWholeViewport); focus only with --focus (not needed once it's whole)
			await ensureWholeViewport({ studio });
			let previousFocus = null;
			if (args.focus) {
				process.env.RUR_FOCUS = "1";
				const target = await runLuau("return 0", { studio });
				previousFocus = focusStudio(target.studio.name);
				if (previousFocus) await new Promise(resolve => setTimeout(resolve, 400));
			}
			let mounted;
			try {
				mounted = await runLuau(mountLuau, { studio });
			} catch (error) {
				restoreFocus(previousFocus);
				throw error;
			}
			const result = JSON.parse(mounted.text);
			try {
				result.dpi = captureViewport(mounted.studio.name, result.viewport[0], result.viewport[1], capturePath, dpi ? [dpi] : DISPLAY_SCALES);
			} finally {
				restoreFocus(previousFocus);
				await runLuau(UNMOUNT_LUAU, { studio });
			}
			return result;
		});
		if (previousDpi && studioResult.dpi && previousDpi !== studioResult.dpi && !args.recapture) {
			fs.rmSync(capturePath, { force: true });
			fs.writeFileSync(path.join(outDir, "studio-capture.json"), previousRecord);
			throw new Error(`Studio is at ${studioResult.dpi}x but ${outDir} holds a ${previousDpi}x capture: not replacing it (move Studio back to that monitor, or pass --recapture to capture at ${studioResult.dpi}x)`);
		}
		// Studio's dump as captured (GetChildren order, rects, props): the layout rows are sorted by error
		fs.writeFileSync(dumpFile, JSON.stringify({ target: studioResult.target, nodes: studioResult.nodes }));
		dpi = studioResult.dpi;
		// Measured 2026-09-28: with a fractional logical viewport height (1724 px / 1.5 = 1149.33) Studio's
		// projection is stretched by ~0.4 px over the height and text / images blur; a whole-number height
		// (resize the Studio window by a pixel) captures clean (fixtures/text-probe-place.luau)
		if (Math.abs(studioResult.viewport[1] - Math.round(studioResult.viewport[1])) > 0.01) {
			process.stderr.write(`rur compare: Studio's viewport is ${studioResult.viewport.map(v => +v.toFixed(2)).join("x")} logical px; a fractional height stretches its projection (blurred captures). Resize the Studio window by a pixel so the height is whole.
`);
		}
		if (studioResult.viewport[0] < width || studioResult.viewport[1] < height) {
			throw new Error(`Studio's viewport is ${studioResult.viewport.map(Math.floor).join("x")} (at ${dpi}x), smaller than the ${width}x${height} being compared: enlarge Studio's viewport or pass a smaller --viewport`);
		}
		const full = readPng(capturePath);
		colourKeyed = restoreColourKey(full);
		studioPng = crop(full, 0, 0, Math.round(width * dpi), Math.round(height * dpi));
		writePng(studioFile, studioPng);
		fs.rmSync(capturePath, { force: true });
	}
	const viewport = studioResult.viewport;
	// --no-order: the offline run keeps its own sibling order
	const orderFile = args["no-order"] ? null : dumpFile;

	// Camera.ViewportSize: Studio's real viewport around the mounted target (stories that size themselves
	// from the camera, e.g. a UI kit's usePx, then see the same screen offline)
	const camera = viewport ? `${viewport[0]}x${viewport[1]}` : undefined;
	// game.PlaceId: stories branch on the place (one place id or another), so the offline run sees Studio's
	const placeId = args["place-id"] || (studioResult.placeId ? String(studioResult.placeId) : undefined);
	// Studio's frame clock from the build to the capture, replayed offline (cli/clock.mjs); --no-clock
	// renders with the ideal clock instead
	const clockFile = args["no-clock"] ? null : writeClock(studioResult.clock, outDir);
	// Studio-only instances the stories reach (GUI templates in ServerStorage...), exported from this Studio
	// by tools/sweep/studio-assets.mjs: the offline run loads them instead of default-property placeholders
	const defaultSnapshots = sharedStatePath(path.join("snapshots", studio));
	const snapshotDir = args["snapshot-dir"] || (!args["no-snapshots"] && fs.existsSync(defaultSnapshots) ? defaultSnapshots : undefined);
	const rotationClipping = args["rotation-clipping"] || process.env.RUR_ROTATION_CLIPPING || "enabled";
	const renderArgs = { ...args, out: path.join(outDir, "rur"), viewport: `${width}x${height}`, scale: String(dpi), time: String(time), background: background.join(","), camera, "place-id": placeId, "snapshot-dir": snapshotDir, "rotation-clipping": rotationClipping };
	// Players.LocalPlayer: the offline run gets Studio's (name, UserId, DisplayName); stories show it and
	// Players:GetNameFromUserIdAsync answers it for its own id
	const player = studioResult.localPlayer || null;
	const summary = await withStudioPlayer(player, () => withStudioOrder(orderFile, () => withRecordedClock(clockFile, () => render(renderArgs))));
	const ours = readPng(summary.png);
	const pixels = comparePngs(ours, studioPng, path.join(outDir, "diff.png"));
	sideBySide(ours, studioPng, path.join(outDir, "side-by-side.png"));

	const scene = readScene(path.join(outDir, "rur", "scene.json"));
	const layout = compareLayout(studioResult.nodes, scene, dpi, [studioResult.target[0], studioResult.target[1]]);

	// --ideal: the same capture against an offline run on the ideal clock (rur-ideal/), i.e. what the
	// replayed clock changes
	let ideal;
	if (args.ideal && clockFile) {
		const idealSummary = await withStudioPlayer(player, () => withStudioOrder(orderFile, () => withRecordedClock(null, () => render({ ...renderArgs, out: path.join(outDir, "rur-ideal") }))));
		const idealScene = readScene(path.join(outDir, "rur-ideal", "scene.json"));
		const idealLayout = compareLayout(studioResult.nodes, idealScene, dpi, [studioResult.target[0], studioResult.target[1]]);
		ideal = {
			pixels: comparePngs(readPng(idealSummary.png), studioPng, null),
			layout: { matched: idealLayout.matched, exact: idealLayout.exact, within1: idealLayout.within1, propMismatches: idealLayout.propMismatches },
		};
	}
	const report = {
		source,
		studio: { status: studioResult.status, failure: studioResult.failure, nodes: studioResult.nodes.length, viewport, placeId: studioResult.placeId, colourKeyed,
			...(calibration ? { captureStretch: calibration } : {}),
			...(Object.keys(probes).length ? { probes: studioResult.probes || {}, staleState: staleProbes(probes, studioResult.probes) } : {}) },
		rur: { status: summary.status, failure: summary.failure, nodes: summary.nodes, scriptErrors: summary.scriptErrors, missingImages: summary.missingImages },
		// The offline run's options (tools/sweep/attribute.mjs re-runs the story with them)
		options: { viewport: `${width}x${height}`, scale: dpi, time, background, camera, placeId, place: args.place, controls: args.controls, clock: clockFile || undefined, player: player || undefined, order: orderFile || undefined, snapshotDir, rotationClipping },
		// The recorded frame clock (null: ideal clock) and whether Studio's target was frozen for the capture
		clock: { ...(clockSummary(clockFile) || { replayed: false }), frozen: Boolean(studioResult.frozen), ideal },
		pixels,
		layout: { ...layout, rows: layout.rows.slice(0, 60) },
		files: { studio: studioFile, ours: summary.png, diff: path.join(outDir, "diff.png"), sideBySide: path.join(outDir, "side-by-side.png") },
	};
	report.studio.reused = reuse.record ? reuse.record.capturedAt : false;
	if (!reuse.record) {
		// What this capture depends on, for the next compare (reusableCapture)
		const issues = [];
		if (studioResult.status && studioResult.status !== "ok") issues.push(`Studio status ${studioResult.status}`);
		if (studioResult.failure) issues.push(`Studio failure: ${String(studioResult.failure).split("\n")[0]}`);
		if (calibration && calibration.stretched) issues.push("Studio's projection was stretched");
		if (report.studio.staleState) issues.push("Studio kept state from earlier runs");
		if (summary.status !== "ok") issues.push(`the offline run ended ${summary.status}, so its dependency list may be incomplete`);
		let files = [];
		try {
			files = JSON.parse(fs.readFileSync(path.join(outDir, "rur", "dependencies.json"), "utf8")).files || [];
		} catch {
			issues.push("the offline run reported no dependencies");
		}
		const dependencies = {};
		for (const file of new Set([...(args.story ? [path.resolve(String(args.story))] : []), ...files])) dependencies[file] = fileHash(file);
		fs.writeFileSync(path.join(outDir, "studio-capture.json"), JSON.stringify({
			version: CAPTURE_VERSION,
			key: captureKey,
			capturedAt: new Date().toISOString(),
			valid: issues.length === 0,
			issues,
			studio: { ...studioResult, target: undefined, nodes: undefined },
			calibration: calibration || null,
			colourKeyed,
			dependencies,
		}, null, 1));
	}
	fs.writeFileSync(path.join(outDir, "report.json"), JSON.stringify(report, null, 2));
	fs.writeFileSync(path.join(outDir, "layout-full.json"), JSON.stringify(layout, null, 1));
	return report;
}
