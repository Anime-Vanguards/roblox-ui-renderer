// Measure the dedicated MCP wheel actions. Starts Play from Edit, then always restores Edit.
// node tools/interaction/experiments/scroll-mcp.mjs [output.json] [Studio name or id]
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { withStudioLock } from "../../../cli/lock.mjs";
import { DEFAULT_STUDIO } from "../../../cli/studio.mjs";

const output = path.resolve(process.argv[2] || "out/interaction/scroll-mcp-actions.json");
const query = process.argv[3] || DEFAULT_STUDIO;
const argsFile = path.resolve(`out/interaction/scroll-mcp-args-${process.pid}.json`);
let studioId;
fs.mkdirSync(path.dirname(argsFile), { recursive: true });
function call(name, args = {}) {
	fs.writeFileSync(argsFile, JSON.stringify({ studio_id: studioId, ...args }));
	const result = spawnSync(process.execPath, ["tools/studio-mcp.cjs", "call", name, "@" + argsFile],
		{ encoding: "utf8", windowsHide: true, timeout: 150000, maxBuffer: 8 * 1024 * 1024 });
	if (result.status !== 0) throw new Error(result.stderr || result.stdout || result.error?.message);
	return result.stdout.trim();
}
const setup = `
local Players, UIS, Run = game:GetService("Players"), game:GetService("UserInputService"), game:GetService("RunService")
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_McpScroll"
Gui.IgnoreGuiInset = true
Gui.DisplayOrder = 1000000
Gui.ResetOnSpawn = false
local Scroll = Instance.new("ScrollingFrame")
Scroll.Name = "MeasuredScroll"
Scroll.Position = UDim2.fromOffset(100, 100)
Scroll.Size = UDim2.fromOffset(400, 300)
Scroll.CanvasSize = UDim2.fromOffset(400, 6000)
Scroll.CanvasPosition = Vector2.new(0, 1000)
Scroll.BorderSizePixel = 0
Scroll.ScrollBarThickness = 0
Scroll.BackgroundColor3 = Color3.fromRGB(30, 60, 90)
Scroll.Parent = Gui
Gui.Parent = Players.LocalPlayer:WaitForChild("PlayerGui")
local C = { gui = Gui, scroll = Scroll, connections = {}, samples = {}, inputs = {}, pointers = {}, frame = 0, start = os.clock() }
_G.__RUR_MCP_SCROLL = C
table.insert(C.connections, Run.PreRender:Connect(function(Dt)
	C.frame += 1
	local P, V = Scroll.CanvasPosition, Scroll:GetScrollVelocity()
	table.insert(C.samples, { f = C.frame, t = os.clock() - C.start, dt = Dt, x = P.X, y = P.Y, vx = V.X, vy = V.Y })
end))
table.insert(C.connections, UIS.InputChanged:Connect(function(Input)
	if Input.UserInputType == Enum.UserInputType.MouseWheel then
		table.insert(C.inputs, { f = C.frame, t = os.clock() - C.start, z = Input.Position.Z, delta = Input.Delta.Z })
	end
end))
table.insert(C.connections, UIS.PointerAction:Connect(function(Wheel, Pan, Pinch, Processed)
	table.insert(C.pointers, { f = C.frame, t = os.clock() - C.start, wheel = Wheel, processed = Processed })
end))
return "MCP scrolling fixture ready"
`;
try {
	await withStudioLock("MCP scroll action measurement", async () => {
		const studios = JSON.parse(call("list_roblox_studios")).studios;
		const studio = studios.find(item => item.id === query) || studios.find(item => item.name.includes(query));
		if (!studio) throw new Error(`No Studio matches ${query}`);
		studioId = studio.id;
		if (!call("get_studio_state").includes("Mode: Edit")) throw new Error("Expected Edit mode; leaving existing session untouched");
		try {
			console.log(call("start_stop_play", { is_start: true }));
			// Play becomes available before the client execution bridge has finished loading.
			await new Promise(resolve => setTimeout(resolve, 10000));
			console.log(call("execute_luau", { datamodel_type: "Client", code: setup }));
			console.log(call("user_mouse_input", { datamodel_type: "Client", actions: [
				{ action: "moveTo", instance_path: "LocalPlayer.PlayerGui.__RUR_McpScroll.MeasuredScroll" },
				{ action: "wait", wait_time_ms: 250 },
				{ action: "scrollDown" }, { action: "wait", wait_time_ms: 1000 },
				{ action: "scrollUp" }, { action: "wait", wait_time_ms: 1000 },
				{ action: "scrollDown" }, { action: "scrollDown" }, { action: "wait", wait_time_ms: 1000 },
			] }));
			const capture = JSON.parse(call("execute_luau", { datamodel_type: "Client", code: `
local C = _G.__RUR_MCP_SCROLL
for _, Connection in C.connections do Connection:Disconnect() end
return game:GetService("HttpService"):JSONEncode({ version = version(), samples = C.samples, inputs = C.inputs, pointers = C.pointers,
	window = { C.scroll.AbsoluteWindowSize.X, C.scroll.AbsoluteWindowSize.Y },
	canvas = { C.scroll.AbsoluteCanvasSize.X, C.scroll.AbsoluteCanvasSize.Y } })` }));
			const reference = { schema: 1, studioVersion: capture.version, source: "Studio MCP user_mouse_input", cases: [{
				name: "mcp-play/wheel-actions", scale: 1, window: capture.window, canvas: capture.canvas,
				actions: capture.inputs.map(({ f, z }) => ({ frame: f, wheel: z })),
				inputs: capture.inputs, pointers: capture.pointers, samples: capture.samples,
			}] };
			if (!capture.inputs.length) throw new Error("MCP produced no recorded wheel input");
			fs.mkdirSync(path.dirname(output), { recursive: true });
			fs.writeFileSync(output, JSON.stringify(reference));
			console.log(JSON.stringify({ output, frames: capture.samples.length, inputs: capture.inputs, pointers: capture.pointers }));
		} finally {
			try {
				call("execute_luau", { datamodel_type: "Client", code: `local C = _G.__RUR_MCP_SCROLL
if C then for _, V in C.connections do V:Disconnect() end C.gui:Destroy() _G.__RUR_MCP_SCROLL = nil end return true` });
			} catch (error) { console.error("Fixture cleanup:", error.message); }
			console.log(call("start_stop_play", { is_start: false }));
			console.log(call("get_studio_state"));
		}
	});
} finally {
	fs.rmSync(argsFile, { force: true });
}
