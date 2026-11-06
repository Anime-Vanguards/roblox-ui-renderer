// Studio frame clock for `rur compare` (animation agent, notes/animation.md "Recorded clock").
//
// Studio and the offline run must show the same moment. The Studio side (CLOCK_LUAU, spliced into the
// compare mount script) seeds math.random right before the story is built, logs every Heartbeat and
// render step with Studio's os.clock() from the end of the build to the capture (`task.wait(time)`
// after the build), and then freezes the target: a static clone replaces it, so the viewport capture
// taken afterwards shows exactly the captured moment while the story's animations keep running on the
// hidden original. The offline run replays that log (RUR_CLOCK, the emulator's src/scheduler.rs): the
// same Heartbeat / render dts, os.clock() / tick() / DateTime values and random seed, ending at the
// same resumption point.

import fs from "node:fs";
import path from "node:path";

// math.random seed of both sides (the offline runtime seeds plain renders with the same value)
export const CLOCK_SEED = 1;

// Luau helpers for the Studio mount script: __RUR_BeginMount() right before the build,
// __RUR_StartClock(Target) right after it, __RUR_Capture() in place of the wait before the capture (returns
// the clock log), __RUR_Freeze(Target) after it (returns the frozen clone, or nil when the target
// cannot be cloned faithfully).
export const CLOCK_LUAU = ({ time, seed = CLOCK_SEED }) => `
local __RUR_RunService = game:GetService("RunService")
local __RUR_ClockLog = {}
local __RUR_ClockInfo = { version = 2, seed = ${Number(seed)}, time = ${Number(time)} }
local __RUR_Recording = false
local __RUR_ClockConnections = {}

local function __RUR_Num(Value)
	return string.format("%.17g", Value)
end

-- Right before the build: the offline run seeds math.random the same way before its own mount
local function __RUR_BeginMount()
	__RUR_ClockInfo.mountStart = os.clock()
	math.randomseed(${Number(seed)})
end

-- Right after the build: the clocks at virtual time 0 and the instances the build made (the offline run
-- measures build progress in instance creations), then every Heartbeat's resumption phase ("t", a
-- task.wait() loop that is due first), its handlers ("h", newest connection, so first) and every render
-- step ("r", RenderStepped)
local function __RUR_StartClock(Built)
	local MountEnd = os.clock()
	__RUR_ClockInfo.mount = {
		start = __RUR_Num(__RUR_ClockInfo.mountStart or MountEnd),
		["end"] = __RUR_Num(MountEnd),
		instances = #Built:GetDescendants(),
	}
	__RUR_ClockInfo.mountStart = nil
	__RUR_ClockInfo.unix = __RUR_Num(DateTime.now().UnixTimestampMillis)
	__RUR_ClockInfo.tick = __RUR_Num(tick())
	__RUR_ClockInfo.server = __RUR_Num(workspace:GetServerTimeNow())
	__RUR_ClockInfo.elapsed = __RUR_Num(elapsedTime())
	__RUR_Recording = true
	table.insert(__RUR_ClockConnections, __RUR_RunService.Heartbeat:Connect(function(Dt)
		if __RUR_Recording then
			table.insert(__RUR_ClockLog, { "h", __RUR_Num(Dt), __RUR_Num(os.clock()) })
		end
	end))
	table.insert(__RUR_ClockConnections, __RUR_RunService.RenderStepped:Connect(function(Dt)
		if __RUR_Recording then
			table.insert(__RUR_ClockLog, { "r", __RUR_Num(Dt), __RUR_Num(os.clock()) })
		end
	end))
	task.spawn(function()
		while __RUR_Recording do
			task.wait()
			if __RUR_Recording then
				table.insert(__RUR_ClockLog, { "t", __RUR_Num(os.clock()) })
			end
		end
	end)
end

-- The capture: resumes where Studio's task.wait(Time) resumes, stops the log there
local function __RUR_Capture(Time)
	local Call = os.clock()
	local Waited = task.wait(Time)
	local At = os.clock()
	__RUR_Recording = false
	for _, Connection in __RUR_ClockConnections do
		Connection:Disconnect()
	end
	table.insert(__RUR_ClockLog, { "c", __RUR_Num(At), __RUR_Num(Call), __RUR_Num(Waited) })
	__RUR_ClockInfo.events = __RUR_ClockLog
	return __RUR_ClockInfo
end

-- Freezes the captured moment: a clone (property values as captured, no running animation) takes the
-- target's place. Engine state a clone does not keep is copied over once it is laid out: a
-- ScrollingFrame's CanvasPosition (measured: the clone shows its canvas at 0) and a UIPageLayout's
-- current page (jumped to without animating).
local function __RUR_Freeze(Original)
	local Ok, Frozen = pcall(function()
		return Original:Clone()
	end)
	if not Ok or typeof(Frozen) ~= "Instance" then
		return nil
	end
	local Descendants = Original:GetDescendants()
	local Copies = Frozen:GetDescendants()
	if #Copies ~= #Descendants then
		-- Archivable = false descendants are not cloned: keep the live target
		Frozen:Destroy()
		return nil
	end
	Frozen.Parent = Original.Parent
	Original.Visible = false
	for Index, Node in Descendants do
		local Copy = Copies[Index]
		if Node:IsA("ScrollingFrame") and Copy:IsA("ScrollingFrame") then
			pcall(function()
				local _ = Copy.AbsoluteCanvasSize
				Copy.CanvasPosition = Node.CanvasPosition
			end)
		elseif Node:IsA("UIPageLayout") and Copy:IsA("UIPageLayout") and Node.CurrentPage then
			pcall(function()
				local PageIndex = table.find(Descendants, Node.CurrentPage)
				if PageIndex then
					local Animated = Copy.Animated
					Copy.Animated = false
					Copy:JumpTo(Copies[PageIndex])
					Copy.Animated = Animated
				end
			end)
		end
	end
	return Frozen
end
`;

// Writes the clock log Studio returned as <outDir>/clock.json (numbers as JSON numbers); null when
// there is none.
export function writeClock(clock, outDir) {
	if (!clock || !Array.isArray(clock.events)) return null;
	const number = value => (typeof value === "string" ? Number(value) : value);
	const events = clock.events.map(([kind, ...values]) => [kind, ...values.map(number)]);
	const data = {
		version: 2,
		seed: number(clock.seed),
		time: number(clock.time),
		mount: { start: number(clock.mount.start), end: number(clock.mount.end), instances: number(clock.mount.instances) },
		unix: number(clock.unix),
		tick: number(clock.tick),
		server: number(clock.server),
		elapsed: number(clock.elapsed),
		events,
	};
	const file = path.join(outDir, "clock.json");
	fs.writeFileSync(file, JSON.stringify(data));
	return file;
}

// A summary of a clock log for report.json: counts, the mount's duration and the capture moment.
export function clockSummary(file) {
	if (!file) return null;
	const clock = JSON.parse(fs.readFileSync(file, "utf8"));
	const counts = { t: 0, h: 0, r: 0 };
	let capture = null;
	for (const event of clock.events) {
		if (event[0] in counts) counts[event[0]]++;
		if (event[0] === "c") capture = event;
	}
	const round = value => Math.round(value * 1e6) / 1e6;
	return {
		file,
		heartbeats: counts.h,
		renders: counts.r,
		mountMs: round((clock.mount.end - clock.mount.start) * 1000),
		// Studio's task.wait(time) return and the capture's time after the build
		waited: capture ? round(capture[3]) : null,
		capturedAt: capture ? round(capture[1] - clock.mount.end) : null,
	};
}

// Runs `fn` with RUR_CLOCK set to `file` (the offline runtime replays it), or unset when file is null.
export async function withRecordedClock(file, fn) {
	const previous = process.env.RUR_CLOCK;
	if (file) process.env.RUR_CLOCK = path.resolve(file);
	else delete process.env.RUR_CLOCK;
	try {
		return await fn();
	} finally {
		if (previous === undefined) delete process.env.RUR_CLOCK;
		else process.env.RUR_CLOCK = previous;
	}
}
