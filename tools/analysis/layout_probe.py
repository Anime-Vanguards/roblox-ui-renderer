"""Runs a layout probe in Roblox Studio and offline (rur runtime) and diffs the results.

    python tools/analysis/layout_probe.py tools/studio/layout-probe-foo.luau [--viewport 1200x900]
        [--studio-only] [--offline-only] [--keep] [--tolerance 0.01] [--limit 60] [--time 0.5]

A probe is a Luau chunk returning `function(Target) ... return Results end`: it builds UI under
Target (a Frame sized to the viewport at (0, 0) inside a ScreenGui with IgnoreGuiInset), may yield
(task.wait()), and returns a JSON-able table of property reads. Studio runs it under CoreGui while
holding the Studio lock (the GUI is destroyed afterwards); offline it runs as a fixture in the rur
runtime at DPI 1.5. Results are written to out/probe/<name>/{studio,ours}.json and compared
numerically (keys/paths that differ by more than the tolerance are listed).
"""

import json
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
MARKER = "__RURPROBE__"

STUDIO_WRAPPER = """
local HttpService = game:GetService("HttpService")
local CoreGui = game:GetService("CoreGui")
local Old = CoreGui:FindFirstChild("__RUR_LayoutProbe")
if Old then
	Old:Destroy()
end
local Gui = Instance.new("ScreenGui")
Gui.Name = "__RUR_LayoutProbe"
Gui.IgnoreGuiInset = true
Gui.DisplayOrder = 2000000000
Gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
local Target = Instance.new("Frame")
Target.Name = "StoryTarget"
Target.Size = UDim2.fromOffset(%(width)d, %(height)d)
Target.BackgroundTransparency = 1
Target.BorderSizePixel = 0
Target.Parent = Gui
Gui.Parent = CoreGui
local Probe = (function()
%(source)s
end)()
local Ok, Result = pcall(Probe, Target)
Gui:Destroy()
if not Ok then
	return HttpService:JSONEncode({ __error = tostring(Result) })
end
return HttpService:JSONEncode(Result)
"""

OFFLINE_WRAPPER = """
return function(Target)
	local Probe = (function()
%(source)s
	end)()
	task.spawn(function()
		local Ok, Result = pcall(Probe, Target)
		local HttpService = game:GetService("HttpService")
		print("%(marker)s" .. HttpService:JSONEncode(if Ok then Result else { __error = tostring(Result) }))
	end)
end
"""


def lune_path():
    candidate = os.path.join(os.path.expanduser("~"), ".rokit", "tool-storage", "lune-org", "lune", "0.10.4", "lune.exe")
    return candidate if os.path.exists(candidate) else "lune"


def run_studio(source, width, height, out_dir):
    script = os.path.join(out_dir, "studio-probe.luau")
    with open(script, "w", encoding="utf8") as handle:
        handle.write(STUDIO_WRAPPER % {"width": width, "height": height, "source": source})
    result_file = os.path.join(out_dir, "studio.json")
    if os.path.exists(result_file):
        os.remove(result_file)
    # studio-lock spawns through a shell on Windows: pass space-free paths relative to the repo root
    relative = lambda path: os.path.relpath(path, ROOT).replace(os.sep, "/")
    command = ["node", "tools/studio-lock.mjs", "node", "tools/studio-mcp.cjs", "luau-big", relative(script), relative(result_file)]
    completed = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    if not os.path.exists(result_file):
        raise RuntimeError(f"studio probe failed: {completed.stdout[-3000:]} {completed.stderr[-3000:]}")
    with open(result_file, encoding="utf8") as handle:
        return json.load(handle)


def run_offline(source, width, height, out_dir, time):
    fixture = os.path.join(out_dir, "offline-probe.luau")
    with open(fixture, "w", encoding="utf8") as handle:
        handle.write(OFFLINE_WRAPPER % {"source": source, "marker": MARKER})
    # The Luau runtime this used to run (runtime/main.luau) is retired: the native engine runs the fixture
    # (`rur render --fixture`, no PNG) and writes scene.json with the printed output in diagnostics.
    render_dir = os.path.join(out_dir, "render")
    scene_path = os.path.join(render_dir, "scene.json")
    if os.path.exists(scene_path):
        os.remove(scene_path)
    command = ["node", os.path.join(ROOT, "cli", "rur.mjs"), "render", "--fixture", fixture, "--out", render_dir, "--no-png",
               "--viewport", f"{width}x{height}", "--scale", "1.5", "--time", str(time)]
    completed = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    if completed.returncode != 0 or not os.path.exists(scene_path):
        raise RuntimeError(f"offline probe failed: {completed.stdout[-3000:]} {completed.stderr[-3000:]}")
    with open(scene_path, encoding="utf8") as handle:
        scene = json.load(handle)
    output = (scene.get("diagnostics") or {}).get("output") or []
    for entry in output:
        message = entry.get("message") or ""
        if message.startswith(MARKER):
            result = json.loads(message[len(MARKER):])
            with open(os.path.join(out_dir, "ours.json"), "w", encoding="utf8") as handle:
                json.dump(result, handle, indent=1)
            return result
    errors = [entry.get("message") for entry in output if entry.get("kind") == "error"]
    raise RuntimeError(f"offline probe printed no result; errors: {errors[:5]}")


def diff(studio, ours, path, tolerance, rows):
    if isinstance(studio, dict) and isinstance(ours, dict):
        for key in studio:
            if key not in ours:
                rows.append((float("inf"), f"{path}.{key}", studio[key], "<missing>"))
            else:
                diff(studio[key], ours[key], f"{path}.{key}", tolerance, rows)
        return
    if isinstance(studio, list) and isinstance(ours, list):
        numeric = all(isinstance(value, (int, float)) and not isinstance(value, bool) for value in studio + ours)
        if numeric and len(studio) == len(ours):
            error = max((abs(a - b) for a, b in zip(studio, ours)), default=0)
            if error > tolerance:
                rows.append((error, path, studio, ours))
            return
        if len(studio) != len(ours):
            rows.append((float("inf"), f"{path} (length {len(studio)} vs {len(ours)})", studio if len(str(studio)) < 200 else "...", ours if len(str(ours)) < 200 else "..."))
        for index, (a, b) in enumerate(zip(studio, ours)):
            diff(a, b, f"{path}[{index}]", tolerance, rows)
        return
    if isinstance(studio, (int, float)) and isinstance(ours, (int, float)) and not isinstance(studio, bool):
        error = abs(studio - ours)
        if error > tolerance:
            rows.append((error, path, studio, ours))
        return
    if studio != ours:
        rows.append((float("inf"), path, studio, ours))


def count_leaves(value):
    if isinstance(value, dict):
        return sum(count_leaves(item) for item in value.values())
    if isinstance(value, list):
        if all(isinstance(item, (int, float)) for item in value):
            return 1
        return sum(count_leaves(item) for item in value)
    return 1


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        sys.exit(2)
    probe = args[0]
    width, height = 1200, 900
    tolerance = 0.01
    limit = 60
    time = 0.5
    studio_only = "--studio-only" in args
    offline_only = "--offline-only" in args
    for index, arg in enumerate(args):
        if arg == "--viewport":
            width, height = (int(part) for part in args[index + 1].split("x"))
        elif arg == "--tolerance":
            tolerance = float(args[index + 1])
        elif arg == "--limit":
            limit = int(args[index + 1])
        elif arg == "--time":
            time = float(args[index + 1])

    name = os.path.splitext(os.path.basename(probe))[0]
    out_dir = os.path.join(ROOT, "out", "probe", name)
    os.makedirs(out_dir, exist_ok=True)
    with open(probe, encoding="utf8") as handle:
        source = handle.read()

    studio_file = os.path.join(out_dir, "studio.json")
    if offline_only:
        with open(studio_file, encoding="utf8") as handle:
            studio = json.load(handle)
    else:
        studio = run_studio(source, width, height, out_dir)
    if isinstance(studio, dict) and studio.get("__error"):
        print("studio error:", studio["__error"])
        sys.exit(1)
    if studio_only:
        print(f"studio: {count_leaves(studio)} values -> {studio_file}")
        return

    ours = run_offline(source, width, height, out_dir, time)
    if isinstance(ours, dict) and ours.get("__error"):
        print("offline error:", ours["__error"])
        sys.exit(1)

    rows = []
    diff(studio, ours, "", tolerance, rows)
    total = count_leaves(studio)
    print(f"== {name}: {total - len(rows)}/{total} values match (tolerance {tolerance})")
    rows.sort(key=lambda row: -row[0] if row[0] != float("inf") else -1e18)
    for error, path, a, b in rows[:limit]:
        print(f"  {error:9.3f} {path}: studio {json.dumps(a)} ours {json.dumps(b)}")


if __name__ == "__main__":
    main()
