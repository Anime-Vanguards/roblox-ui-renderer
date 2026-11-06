"""Frame-structure rules from tools/studio/anim-probe-frames.luau and anim-probe-signals.luau, Studio
vs the offline runtime.

    python tools/analysis/anim_frames.py [--runs]

Reads out/probe/anim-probe-frames/{studio*,ours}.json and out/probe/anim-probe-signals/{studio,ours}.json
and prints each measured rule for Studio and for the emulation side by side (a "!" marks a rule the
two disagree on). --runs also prints per-run Heartbeat statistics of every studio*.json.
"""

import collections
import glob
import json
import math
import os
import statistics
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FRAMES = os.path.join(ROOT, "out", "probe", "anim-probe-frames")
SIGNALS = os.path.join(ROOT, "out", "probe", "anim-probe-signals")


def load(path):
    with open(path, encoding="utf8") as handle:
        return json.load(handle)


def frame_rules(log):
    rules = {}
    heartbeats = [(float(e[1]), float(e[2])) for e in log if e[0] == "Heartbeat"]
    renders = [(float(e[1]), float(e[2])) for e in log if e[0] == "PreRender"]
    per_frame = []
    for (start, _), (end, _) in zip(renders, renders[1:]):
        per_frame.append(sum(1 for time, _ in heartbeats if start < time <= end))
    counts = collections.Counter(per_frame)
    rules["Heartbeats per rendered frame (mode)"] = counts.most_common(1)[0][0] if counts else None
    rules["Heartbeat dt mean (ms)"] = round(statistics.mean(dt for _, dt in heartbeats) * 1000, 2)
    rules["render dt mean (ms)"] = round(statistics.mean(dt for _, dt in renders) * 1000, 2)
    rules["dts are float32"] = all(float(__import__("numpy").float32(dt)) == dt for _, dt in heartbeats + renders)
    # A spring animation controller: ceil(dt * 500) integration steps per Heartbeat
    steps = []
    index = 0
    for (start, _), (end, _) in zip(renders, renders[1:]):
        steps.append(sum(math.ceil(dt * 1000 / 2) for time, dt in heartbeats if start < time <= end))
    rules["Controller steps per frame (mean)"] = round(statistics.mean(steps), 2) if steps else None

    # Order inside one render step
    names = [e[0] for e in log]
    first_bind = names.index("Bind-100")
    step = []
    for name in names[first_bind:]:
        if name in ("Heartbeat", "Clocks", "TaskWait0") or name.startswith("Hb"):
            break
        if name.startswith("Bind") or name in ("RenderStepped", "PreRender"):
            step.append(name)
    rules["render step order"] = " ".join(step)

    # Heartbeat handler order (connected: Heartbeat logger, HbA, HbB, HbC, Clocks)
    first_hb = names.index("HbC") if "HbC" in names else None
    order = []
    if first_hb is not None:
        for name in names[first_hb - 1:]:
            if name in ("Clocks", "HbA", "HbB", "HbC", "Heartbeat"):
                order.append(name)
            if name == "Heartbeat":
                break
    rules["Heartbeat handler order"] = " ".join(order)
    rules["task.wait() resumes before Heartbeat handlers"] = names[names.index("TaskWait0") + 1] in ("Clocks", "HbC", "Heartbeat")

    edit_events = {name for name in names if name in ("Stepped", "PreSimulation", "PostSimulation", "PreAnimation")}
    rules["physics events in edit mode"] = sorted(edit_events) or "none"

    clocks = [e for e in log if e[0] == "Clocks"]
    rules["time() in edit mode"] = sorted({float(e[3]) for e in clocks})
    rules["DistributedGameTime in edit mode"] = sorted({float(e[6]) for e in clocks})
    os_clock = [float(e[2]) for e in clocks]
    rules["os.clock() advances with Heartbeats"] = all(b > a for a, b in zip(os_clock, os_clock[1:]))

    # Legacy wait(): resume interval
    legacy = [float(e[1]) for e in log if e[0] == "LegacyWait0"]
    intervals = [b - a for a, b in zip(legacy, legacy[1:])]
    rules["wait() interval (ms, mean)"] = round(statistics.mean(intervals) * 1000, 1) if intervals else None
    wait01 = [float(e[2]) for e in log if e[0] == "TaskWait01"]
    rules["task.wait(0.1) returns (ms, min)"] = round(min(wait01) * 1000, 1) if wait01 else None

    # Scheduling from inside a PreRender handler / a Heartbeat handler
    def position(marker, targets):
        start = names.index(marker)
        found = {}
        heartbeats_seen = 0
        renders_seen = 0
        for name in names[start + 1:]:
            if name == "Heartbeat":
                heartbeats_seen += 1
            if name == "PreRender":
                renders_seen += 1
            if name in targets and name not in found:
                found[name] = f"after {heartbeats_seen} HB"
        return found

    rules["from PreRender"] = position("ScheduleFromPreRender", ("TaskSpawnFromPreRender", "DeferFromPreRender", "Delay0FromPreRender", "TaskWaitFromPreRender", "SpawnFromPreRender", "LegacyDelay0FromPreRender", "Delay005FromPreRender"))
    rules["from Heartbeat"] = position("ScheduleFromHeartbeat", ("DeferFromHeartbeat", "Delay0FromHeartbeat", "TaskWaitFromHeartbeat", "SpawnFromHeartbeat"))
    return rules


def signal_rules(log):
    names = [e[0] for e in log]
    rules = {}
    changed = [n for n in names if n.startswith("Changed") or n.startswith("PropertyChanged")]
    rules["Changed handler order (connected A,B,C,D)"] = " ".join(changed[:4])
    rules["Changed deferred"] = names.index("AfterSet2") < names.index(changed[0])
    bindable = [n for n in names if n.startswith("Bindable")]
    rules["BindableEvent order (A,B,C,Once,Wait)"] = " ".join(bindable[:5])
    first_render = [n for n in names if n in ("RenderStepped", "PreRender")][:2]
    rules["RenderStepped vs PreRender"] = " ".join(first_render)
    nested = [n for n in names if n.startswith("Nested")]
    rules["connect during fire (next fire order)"] = " ".join(nested[1:3])
    return rules


def show(title, studio, ours):
    print(f"== {title}")
    for key in studio:
        mark = " " if str(studio[key]) == str(ours.get(key)) else "!"
        print(f"{mark} {key}")
        print(f"      studio: {studio[key]}")
        print(f"      ours:   {ours.get(key)}")


def main():
    studio_runs = sorted(glob.glob(os.path.join(FRAMES, "studio*.json")))
    ours = load(os.path.join(FRAMES, "ours.json"))
    # The reference Studio run: the first one recorded with a 60 Hz render (window visible)
    reference = None
    for path in studio_runs:
        rules = frame_rules(load(path)["Log"])
        if rules["Heartbeats per rendered frame (mode)"] == 4:
            reference = rules
            break
    if reference is None:
        reference = frame_rules(load(studio_runs[0])["Log"])
    show("frame structure (edit mode)", reference, frame_rules(ours["Log"]))
    if os.path.exists(os.path.join(SIGNALS, "ours.json")):
        show("signals", signal_rules(load(os.path.join(SIGNALS, "studio.json"))["Log"]), signal_rules(load(os.path.join(SIGNALS, "ours.json"))["Log"]))
    if "--runs" in sys.argv:
        print("== Studio runs")
        for path in studio_runs:
            rules = frame_rules(load(path)["Log"])
            print(f"  {os.path.basename(path)}: HB/frame {rules['Heartbeats per rendered frame (mode)']}, HB dt {rules['Heartbeat dt mean (ms)']} ms, render dt {rules['render dt mean (ms)']} ms, Controller steps/frame {rules['Controller steps per frame (mean)']}")


if __name__ == "__main__":
    main()
