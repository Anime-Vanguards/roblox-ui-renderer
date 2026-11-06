"""Re-runs every cached layout probe offline (no Studio) and prints one summary line per probe.

    python tools/analysis/layout_probes.py [filter regex] [--jobs 6] [--tolerance 0.01] [--all]

Each probe in out/probe/layout-probe-*/ with a studio.json is run offline through
tools/analysis/layout_probe.py --offline-only at the viewport its Studio run used (read from the saved
studio-probe.luau) with --time 1.5. Full diffs go to out/probe/<name>/offline.txt. Without --all only
probes that are not exact are listed, plus a total. Studio-only probes (the story bisects) are skipped.
"""

import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
STUDIO_ONLY = ("sliderbisect", "optionsbisect")


def viewport(name):
    path = os.path.join(ROOT, "out", "probe", name, "studio-probe.luau")
    try:
        with open(path, encoding="utf8") as handle:
            match = re.search(r"Target\.Size = UDim2\.fromOffset\((\d+), (\d+)\)", handle.read())
    except OSError:
        return "1200x900"
    return f"{match.group(1)}x{match.group(2)}" if match else "1200x900"


def run(name, tolerance):
    probe = os.path.join(ROOT, "tools", "studio", name + ".luau")
    command = [sys.executable, os.path.join(ROOT, "tools", "analysis", "layout_probe.py"), probe, "--offline-only",
               "--limit", "300", "--time", "1.5", "--tolerance", str(tolerance), "--viewport", viewport(name)]
    completed = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    output = completed.stdout + completed.stderr
    with open(os.path.join(ROOT, "out", "probe", name, "offline.txt"), "w", encoding="utf8") as handle:
        handle.write(output)
    match = re.search(r"== \S+: (\d+)/(\d+) values match", output)
    if not match:
        return name, None, None
    return name, int(match.group(1)), int(match.group(2))


def main():
    args = sys.argv[1:]
    pattern = "."
    jobs, tolerance, show_all = 6, 0.01, "--all" in args
    index = 0
    while index < len(args):
        if args[index] == "--jobs":
            jobs = int(args[index + 1])
            index += 2
            continue
        if args[index] == "--tolerance":
            tolerance = float(args[index + 1])
            index += 2
            continue
        if not args[index].startswith("--"):
            pattern = args[index]
        index += 1

    names = []
    for name in sorted(os.listdir(os.path.join(ROOT, "out", "probe"))):
        if not name.startswith("layout-probe-") or not re.search(pattern, name):
            continue
        if any(part in name for part in STUDIO_ONLY):
            continue
        if os.path.exists(os.path.join(ROOT, "out", "probe", name, "studio.json")) and os.path.exists(os.path.join(ROOT, "tools", "studio", name + ".luau")):
            names.append(name)

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(lambda name: run(name, tolerance), names))
    exact = total = 0
    for name, matched, count in results:
        if matched is None:
            print(f"{name:40} FAILED (see out/probe/{name}/offline.txt)")
            continue
        exact += matched
        total += count
        if show_all or matched != count:
            print(f"{name:40} {matched}/{count}")
    print(f"total {exact}/{total} values match in {len(results)} probes")


if __name__ == "__main__":
    main()
