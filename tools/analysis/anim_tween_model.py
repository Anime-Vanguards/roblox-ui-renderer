"""Reference model of Roblox's TweenService stepper, replayed on recorded frame times.

    python tools/analysis/anim_tween_model.py [semantics|override|all] [--ours]

The probes (tools/studio/anim-probe-tween-semantics.luau, anim-probe-tween-override.luau) log, at
every PreRender, the render dt and the value of every scenario, plus PlaybackState changes and
Completed events. This script re-runs each scenario through the model below with the same dts and
actions and reports every value/event that differs. With --ours it checks the offline runtime's log
(ours.json) against the model run on the offline dts instead, which validates
runtime/engine/services.luau independently of Studio's frame jitter.

Model (measured, see notes/animation.md): tweens advance once per render frame, after the
BindToRenderStep callbacks and before RenderStepped/PreRender, by the frame's float32 dt; elapsed
time accumulates in float32; delay leftovers are dropped; reversing tweens carry time through the
turn; each repeat restarts from the start value (after DelayTime again, reported as Delayed); the
start value is captured when the tween first enters its playing phase after Play(); Play() takes
ownership of the tween's properties and a tween that owns none of them is cancelled at the next step.
"""

import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from anim_easing import Ease  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
F = np.float32


def f32(x):
    return float(F(x))


class Target:
    def __init__(self, values):
        self.values = dict(values)


class Tween:
    def __init__(self, name, target, props, time, style="Linear", direction="Out", repeat=0, reverses=False, delay=0.0):
        self.name = name
        self.target = target
        self.props = props
        self.time, self.style, self.direction = time, style, direction
        self.repeat, self.reverses, self.delay = repeat, reverses, delay
        self.state = "Begin"
        self.phase = None
        self.delay_elapsed = 0.0
        self.elapsed = 0.0
        self.iteration = 0
        self.start = None


class Model:
    def __init__(self):
        self.active = []
        self.owners = {}
        self.events = []
        self.before_step = []
        self.displaced = []

    def set_state(self, tween, state):
        if tween.state != state:
            tween.state = state
            self.events.append(("State", tween.name, state))

    def claim(self, tween):
        for prop in tween.props:
            key = (id(tween.target), prop)
            previous = self.owners.get(key)
            if previous is not None and previous is not tween and previous not in self.displaced:
                self.displaced.append(previous)
            self.owners[key] = tween

    def play(self, tween):
        if tween.state in ("Playing", "Delayed"):
            return
        if tween.state == "Paused":
            self.set_state(tween, "Playing")
            return
        self.set_state(tween, "Delayed" if tween.state == "Begin" and tween.delay > 0 else "Playing")
        tween.phase = "delay" if tween.delay > 0 else "play"
        tween.delay_elapsed = 0.0
        tween.elapsed = 0.0
        tween.iteration = 0
        tween.start = None
        self.claim(tween)
        if tween not in self.active:
            self.active.append(tween)

    def pause(self, tween):
        if tween.state == "Playing":
            self.set_state(tween, "Paused")

    def remove(self, tween):
        # Measured: the active list is an array stepped from its end; a finished tween is replaced
        # by the last element (swap-remove), which reorders later completions.
        if tween in self.active:
            index = self.active.index(tween)
            self.active[index] = self.active[-1]
            self.active.pop()

    def finish(self, tween, state):
        self.set_state(tween, state)
        self.events.append(("Completed", tween.name, state))
        self.remove(tween)

    def cancel(self, tween):
        if tween.state in ("Playing", "Delayed", "Paused"):
            self.finish(tween, "Cancelled")

    def write(self, tween, owned, alpha=None, end=None):
        for prop in owned:
            start, goal = tween.start[prop], tween.props[prop]
            # Measured: iteration ends write lerp(start, goal, 1) / lerp(start, goal, 0)
            if end == "goal":
                eased = 1.0
            elif end == "start":
                eased = 0.0
            else:
                eased = float(Ease(tween.style, tween.direction, alpha))
            # NumberValue.Value is a double property: lerped in double at the float32 alpha
            tween.target.values[prop] = start + (goal - start) * eased

    def step(self, dt):
        for hook in self.before_step:
            hook(self)
        dt = f32(dt)
        # Measured: tweens displaced by a later Play() are cancelled before the step, in the order
        # of those Play() calls, once they own none of their properties
        displaced, self.displaced = self.displaced, []
        for tween in displaced:
            if tween in self.active and not any(self.owners.get((id(tween.target), p)) is tween for p in tween.props):
                self.finish(tween, "Cancelled")
        index = len(self.active) - 1
        while index >= 0:
            if index >= len(self.active):
                index = len(self.active) - 1
                continue
            tween = self.active[index]
            index -= 1
            owned = [p for p in tween.props if self.owners.get((id(tween.target), p)) is tween]
            if not owned:
                self.finish(tween, "Cancelled")
                continue
            if tween.state == "Paused":
                continue
            if tween.phase == "delay":
                tween.delay_elapsed = f32(tween.delay_elapsed + dt)
                if tween.delay_elapsed >= tween.delay:
                    tween.phase = "play"
                    tween.elapsed = 0.0
                    if tween.start is None:
                        tween.start = {p: tween.target.values[p] for p in tween.props}
                    if tween.state == "Delayed":
                        self.set_state(tween, "Playing")
                continue
            if tween.start is None:
                # Measured: a non-delayed tween captures its start values at its first step
                tween.start = {p: tween.target.values[p] for p in tween.props}
            tween.elapsed = f32(tween.elapsed + dt)
            duration = tween.time
            if tween.reverses:
                if duration <= 0 or tween.elapsed >= f32(duration * 2):
                    self.write(tween, owned, end="start")
                    self.end_iteration(tween)
                elif tween.elapsed >= duration:
                    self.write(tween, owned, alpha=f32(f32(2) - f32(tween.elapsed / f32(duration))))
                else:
                    self.write(tween, owned, alpha=f32(tween.elapsed / f32(duration)))
            elif duration <= 0 or tween.elapsed >= duration:
                self.write(tween, owned, end="goal")
                self.end_iteration(tween)
            else:
                self.write(tween, owned, alpha=f32(tween.elapsed / f32(duration)))

    def end_iteration(self, tween):
        tween.iteration += 1
        if tween.repeat < 0 or tween.iteration <= tween.repeat:
            tween.elapsed = 0.0
            if tween.delay > 0:
                tween.phase = "delay"
                tween.delay_elapsed = 0.0
                self.set_state(tween, "Delayed")
        else:
            self.finish(tween, "Completed")


def enum_name(text):
    return text.split(".")[-1]


def load(probe, which):
    return json.load(open(os.path.join(ROOT, "out", "probe", probe, f"{which}.json")))


def frames_and_events(log):
    """Splits a probe log into per-frame rows and the events of each frame's step. Events logged
    after a render-step binding played a tween ("PlayedFromBind") belong to the next frame: they
    ran in its render step, before its PreRender row."""
    frames = []
    pending = None
    for entry in log:
        if entry[0] == "Frame":
            frames.append({"frame": int(entry[2]), "dt": float(entry[3]), "values": [float(v) for v in entry[4:]], "events": pending or []})
            pending = None
        elif entry[0] == "PlayedFromBind":
            pending = []
        elif entry[0] in ("State", "Completed"):
            event = (entry[0], entry[2], enum_name(entry[3]))
            if pending is not None:
                pending.append(event)
            elif frames:
                frames[-1]["events"].append(event)
    return frames


def compare(label, frames, columns, actions, model, ignore_events=()):
    problems = 0
    checked = 0
    for row in frames:
        index = row["frame"]
        # The frame's tween step runs before its PreRender handlers (the logger, then the actions)
        model.events = []
        model.step(row["dt"])
        for column, getter in columns:
            expected = getter()
            actual = row["values"][column]
            checked += 1
            if abs(expected - actual) > 1e-6 * max(1, abs(expected)):
                problems += 1
                if problems <= 30:
                    print(f"  {label} F{index} col {column}: log {actual!r} model {expected!r}")
        for action in actions.get(index, []):
            action(model)
        model_events = [e for e in model.events if e[1] not in ignore_events]
        logged_events = [e for e in row["events"] if e[1] not in ignore_events]
        if model_events != logged_events:
            problems += 1
            if problems <= 30:
                print(f"  {label} F{index} events: log {logged_events} model {model_events}")
    print(f"{label}: {checked} values, {problems} mismatches")
    return problems


def semantics(which):
    data = load("anim-probe-tween-semantics", which)
    frames = frames_and_events(data["Log"])
    names = data["Columns"]
    targets = {name: Target({"v": 0.0}) for name in names if name not in ("OverrideValue", "FrameTransparency", "FrameRotation")}
    frame_target = Target({"t": 0.0, "r": 0.0})
    specs = {
        "Delay": dict(time=0.2, delay=0.1),
        "Repeat2": dict(time=0.15, repeat=2),
        "Reverses": dict(time=0.15, reverses=True),
        "ReverseRepeatDelay": dict(time=0.1, repeat=1, reverses=True, delay=0.05),
        "DelayRepeat": dict(time=0.1, repeat=2, delay=0.05),
        "PauseResume": dict(time=0.3),
        "CancelReplay": dict(time=0.3),
        "PlayTwice": dict(time=0.3),
        "PlayAfterComplete": dict(time=0.1),
        "SetDuring": dict(time=0.3),
        "FromBind": dict(time=0.3),
        "ZeroTime": dict(time=0),
        "NegativeTime": dict(time=-1),
        "DestroyTarget": dict(time=0.3),
        "DestroyTween": dict(time=0.3),
        "RepeatForever": dict(time=0.1, repeat=-1),
        "PauseDuringDelay": dict(time=0.1, delay=0.1),
        "CancelThenPlayDelay": dict(time=0.1, delay=0.05),
        "Override": dict(time=0.3),
    }
    T = {name: Tween(name, targets[name], {"v": 1.0}, **spec) for name, spec in specs.items()}
    T["Override2"] = Tween("Override2", targets["Override"], {"v": -1.0}, time=0.3)
    T["FrameA"] = Tween("FrameA", frame_target, {"t": 1.0}, time=0.2)
    T["FrameB"] = Tween("FrameB", frame_target, {"r": 90.0}, time=0.2)
    columns = []
    for index, name in enumerate(names):
        if name in targets:
            columns.append((index, (lambda t: lambda: float(t.values["v"]))(targets[name])))
    # Frame properties are float32 properties
    columns.append((names.index("FrameTransparency"), lambda: f32(frame_target.values["t"])))
    columns.append((names.index("FrameRotation"), lambda: f32(frame_target.values["r"])))

    def play(*keys):
        return lambda m: [m.play(T[k]) for k in keys]

    def set_value(key, value):
        def run(m):
            targets[key].values["v"] = value
        return run

    first = ["Delay", "Repeat2", "Reverses", "ReverseRepeatDelay", "DelayRepeat", "PauseResume", "CancelReplay", "PlayTwice", "PlayAfterComplete", "SetDuring", "ZeroTime", "NegativeTime", "DestroyTarget", "DestroyTween", "RepeatForever", "PauseDuringDelay", "CancelThenPlayDelay", "Override", "FrameA", "FrameB"]
    actions = {
        2: [play(*first)],
        4: [set_value("SetDuring", 5.0), lambda m: m.pause(T["PauseDuringDelay"]), lambda m: m.cancel(T["CancelThenPlayDelay"])],
        5: [lambda m: m.pause(T["PauseResume"]), lambda m: m.cancel(T["CancelReplay"]), play("PlayTwice", "Override2")],
        7: [play("PauseDuringDelay", "CancelThenPlayDelay")],
        10: [play("PauseResume", "CancelReplay")],
        20: [play("PlayAfterComplete")],
        40: [lambda m: m.cancel(T["RepeatForever"])],
    }
    model = Model()
    steps = [0]

    # FromBind is played from a render-step binding in frame 3, before that frame's tween step
    def bind(m):
        steps[0] += 1
        if steps[0] == 3:
            m.play(T["FromBind"])

    model.before_step.append(bind)
    # The Frame tweens' events are not logged with their state; destroyed tweens lose their handlers
    return compare(f"semantics ({which})", frames, columns, actions, model, ignore_events=("FrameA", "FrameB", "DestroyTween"))


def override(which):
    data = load("anim-probe-tween-override", which)
    frames = frames_and_events(data["Log"])
    names = data["Columns"]
    partial = Target({"a": 0.0, "b": 0.0})
    values = {name: Target({"v": 0.0}) for name in names}
    T = {
        "PartialT1": Tween("PartialT1", partial, {"a": 1.0, "b": 100.0}, time=0.4),
        "PartialT2": Tween("PartialT2", partial, {"a": 0.5}, time=0.4),
        "DelayedT1": Tween("DelayedT1", values["DelayedOverride"], {"v": 1.0}, time=0.5),
        "DelayedT2": Tween("DelayedT2", values["DelayedOverride"], {"v": -1.0}, time=0.2, delay=0.15),
        "ReplayT1": Tween("ReplayT1", values["Replay"], {"v": 1.0}, time=0.4),
        "ReplayT2": Tween("ReplayT2", values["Replay"], {"v": -1.0}, time=0.4),
        "PausedT1": Tween("PausedT1", values["PausedOverride"], {"v": 1.0}, time=0.4),
        "PausedT2": Tween("PausedT2", values["PausedOverride"], {"v": -1.0}, time=0.4),
        "DelayedStart": Tween("DelayedStart", values["DelayedStart"], {"v": 1.0}, time=0.2, delay=0.1),
        "ReverseQuad": Tween("ReverseQuad", values["ReverseQuad"], {"v": 1.0}, time=0.25, style="Quad", reverses=True),
        "ReverseBack": Tween("ReverseBack", values["ReverseBack"], {"v": 1.0}, time=0.25, style="Back", direction="In", repeat=1, reverses=True),
        "IdleCancel": Tween("IdleCancel", values["Idle"], {"v": 1.0}, time=0.1),
        "IdlePause": Tween("IdlePause", values["Idle"], {"v": 1.0}, time=0.1),
        "AfterDone": Tween("AfterDone", values["AfterDone"], {"v": 1.0}, time=0.1, delay=0.1),
        "ZeroDelay": Tween("ZeroDelay", values["ZeroDelay"], {"v": 1.0}, time=0, delay=0.1),
        "ZeroReverse": Tween("ZeroReverse", values["ZeroReverse"], {"v": 1.0}, time=0, repeat=2, reverses=True),
        "SameFrameT1": Tween("SameFrameT1", values["SameFrame"], {"v": 1.0}, time=0.3),
        "SameFrameT2": Tween("SameFrameT2", values["SameFrame"], {"v": -1.0}, time=0.3),
    }
    # The Frame's BackgroundTransparency and Rotation are float32 properties
    columns = [(0, lambda: f32(partial.values["a"])), (1, lambda: f32(partial.values["b"]))]
    for index, name in enumerate(names[2:], start=2):
        columns.append((index, (lambda t: lambda: float(t.values["v"]))(values[name])))

    def play(*keys):
        return lambda m: [m.play(T[k]) for k in keys]

    def set_value(target, value):
        def run(m):
            target.values["v"] = value
        return run

    actions = {
        2: [play("PartialT1", "DelayedT1", "ReplayT1", "PausedT1", "DelayedStart", "ReverseQuad", "ReverseBack"), lambda m: m.cancel(T["IdleCancel"]), lambda m: m.pause(T["IdlePause"]), play("AfterDone", "ZeroDelay", "ZeroReverse", "SameFrameT1", "SameFrameT2")],
        4: [set_value(values["DelayedStart"], 0.5), lambda m: m.pause(T["PausedT1"])],
        6: [play("PartialT2", "DelayedT2", "ReplayT2", "PausedT2")],
        9: [play("ReplayT1")],
        30: [play("AfterDone")],
    }
    return compare(f"override ({which})", frames, columns, actions, Model())


def main():
    which = "ours" if "--ours" in sys.argv else "studio"
    what = next((arg for arg in sys.argv[1:] if not arg.startswith("--")), "all")
    total = 0
    if what in ("semantics", "all"):
        total += semantics(which)
    if what in ("override", "all"):
        total += override(which)
    print(f"total mismatches: {total}")


if __name__ == "__main__":
    main()
