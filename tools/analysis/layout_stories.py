"""Re-runs `rur compare` for stories / fixtures and prints a layout summary table.

    python tools/analysis/layout_stories.py [--known] [--fixtures] [<story or fixture path> ...]
        [--time 1] [--viewport 1200x900] [--summary-only]

--known re-runs every story found in out/compare/*/report.json (their `source`), --fixtures every
fixtures/layout-*.luau (except layout-dynamic). Prints matched/exact/within1/TextBounds and the pixel exact % per entry.
"""

import glob
import json
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

FIXTURE_VIEWPORTS = {
    "layout-list": "1200x1100",
    "layout-autosize": "1200x800",
    "layout-text-overflow": "560x500",
    "layout-text-narrow": "880x350",
    "layout-truncate": "1100x260",
    "layout-truncate2": "500x180",
    "layout-truncate3": "960x140",
}


def known_stories():
    stories = []
    for report in glob.glob(os.path.join(ROOT, "out", "compare", "*", "report.json")):
        try:
            source = json.load(open(report, encoding="utf8")).get("source") or ""
        except Exception:
            continue
        if source.endswith(".story.luau") and os.path.exists(source):
            stories.append(source)
    return sorted(set(stories))


def summarize(name):
    path = os.path.join(ROOT, "out", "compare", name, "report.json")
    full = os.path.join(ROOT, "out", "compare", name, "layout-full.json")
    if not os.path.exists(path) or not os.path.exists(full):
        return f"{name:28} (no report)"
    report = json.load(open(path, encoding="utf8"))
    layout = json.load(open(full, encoding="utf8"))
    rows = layout["rows"]
    text = [row for row in rows if "textError" in row]
    text_exact = sum(1 for row in text if row["textError"] < 0.01)
    pixels = report.get("pixels", {}).get("exactPercent", 0)
    status = report.get("rur", {}).get("status")
    return (f"{name:28} nodes {layout['studioNodes']:4}/{layout['ourNodes']:<4} matched {layout['matched']:4} exact {layout['exact']:4} "
            f"within1 {layout['within1']:4} text {text_exact:3}/{len(text):<3} pixels {pixels:7.3f}% {status}")


def run(source, time, viewport):
    is_fixture = not source.endswith(".story.luau")
    name = os.path.basename(source).replace(".story.luau", "").replace(".luau", "")
    if is_fixture:
        viewport = FIXTURE_VIEWPORTS.get(name, viewport)
    command = ["node", os.path.join(ROOT, "cli", "rur.mjs"), "compare", "--fixture" if is_fixture else "--story", source,
               "--viewport", viewport, "--time", str(0.3 if is_fixture else time)]
    if is_fixture:
        command += ["--background", "20,20,26"]
    subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    return name


def main():
    args = sys.argv[1:]
    time, viewport = 1, "1200x900"
    sources = []
    summary_only = "--summary-only" in args
    index = 0
    while index < len(args):
        arg = args[index]
        if arg == "--time":
            time = float(args[index + 1])
            index += 2
            continue
        if arg == "--viewport":
            viewport = args[index + 1]
            index += 2
            continue
        if arg == "--known":
            sources += known_stories()
        elif arg == "--fixtures":
            # layout-dynamic is a live-session workout, not a parity fixture
            sources += sorted(path for path in glob.glob(os.path.join(ROOT, "fixtures", "layout-*.luau")) if not path.endswith("layout-dynamic.luau"))
        elif not arg.startswith("--"):
            sources.append(arg)
        index += 1
    for source in sources:
        name = os.path.basename(source).replace(".story.luau", "").replace(".luau", "")
        if not summary_only:
            run(source, time, viewport)
        print(summarize(name), flush=True)


if __name__ == "__main__":
    main()
