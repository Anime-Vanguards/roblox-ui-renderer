"""Compact summary of `rur compare` layout results.

    python tools/analysis/layout_report.py <name> [<name> ...] [--rows N] [--all]

Reads out/compare/<name>/layout-full.json and prints matched/exact/TextBounds counts plus the worst
rect and TextBounds rows (studio vs ours).
"""

import json
import os
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def fmt(values):
    return "[" + ", ".join(f"{v:.3f}".rstrip("0").rstrip(".") for v in values) + "]"


def summarize(name, rows_limit, show_all):
    path = os.path.join(ROOT, "out", "compare", name, "layout-full.json")
    with open(path, encoding="utf8") as handle:
        layout = json.load(handle)
    rows = layout["rows"]
    text_rows = [row for row in rows if "textError" in row]
    text_exact = sum(1 for row in text_rows if row["textError"] < 0.01)
    missing = [row for row in rows if row.get("missing")]
    print(f"== {name}: studio {layout['studioNodes']} ours {layout['ourNodes']} matched {layout['matched']} "
          f"exact {layout['exact']} within1 {layout['within1']} textBounds {text_exact}/{len(text_rows)}")
    for row in missing[:rows_limit]:
        print(f"  MISSING {row['path']} ({row['class']})")
    bad = [row for row in rows if not row.get("missing") and (show_all or row["error"] >= 0.01)]
    for row in bad[:rows_limit]:
        print(f"  rect {row['error']:8.3f} {row['path']} ({row['class']}) studio {fmt(row['studio'])} ours {fmt(row['ours'])}")
    bad_text = sorted([row for row in text_rows if show_all or row["textError"] >= 0.01], key=lambda row: -row["textError"])
    for row in bad_text[:rows_limit]:
        print(f"  text {row['textError']:8.3f} {row['path']} studio {fmt(row['textBounds']['studio'])} ours {fmt(row['textBounds']['ours'])}")


def main():
    args = sys.argv[1:]
    rows_limit = 25
    show_all = False
    names = []
    index = 0
    while index < len(args):
        if args[index] == "--rows":
            rows_limit = int(args[index + 1])
            index += 2
            continue
        if args[index] == "--all":
            show_all = True
        else:
            names.append(args[index])
        index += 1
    for name in names:
        summarize(name, rows_limit, show_all)


if __name__ == "__main__":
    main()
