"""Extracts the frame clock of a Studio probe log for an offline replay (RUR_CLOCK).

    python tools/analysis/anim_clock.py out/probe/anim-probe-springs/studio.json out/probe/anim-probe-springs/clock.json

Understands the logs of anim-probe-springs.luau ({"h", dt} / {"r", dt, ...} rows) and
anim-probe-frames.luau (["Heartbeat", t, dt] / ["PreRender", t, dt] entries). Writes
{"events": [["h", dt], ["r", dt], ...]}: the Heartbeats and render steps in the order Studio ran
them, with its float32 dts. runtime/engine/scheduler.luau replays it (Scheduler.Step consumes events
up to the next render step), so a probe run offline with anim_probe.mjs --clock sees exactly Studio's
dt sequence.
"""

import json
import sys


def extract(log):
    events = []
    for entry in log:
        kind = entry[0]
        if kind in ("h", "Heartbeat"):
            events.append(["h", float(entry[1] if kind == "h" else entry[2])])
        elif kind in ("r", "PreRender"):
            events.append(["r", float(entry[1] if kind == "r" else entry[2])])
    return events


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(2)
    with open(sys.argv[1], encoding="utf8") as handle:
        data = json.load(handle)
    events = extract(data["Log"])
    with open(sys.argv[2], "w", encoding="utf8") as handle:
        json.dump({"events": events}, handle)
    heartbeats = sum(1 for kind, _ in events if kind == "h")
    print(f"{len(events)} events ({heartbeats} Heartbeats, {len(events) - heartbeats} render steps) -> {sys.argv[2]}")


if __name__ == "__main__":
    main()
