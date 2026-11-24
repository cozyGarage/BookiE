#!/usr/bin/env python3
"""Ratchet UI patterns that docs/ui-design.md says to stop growing in the GTK and GPUI apps.

Counted per file outside test code: `thread_local!` (UI state is passed in), a margin or spacing number written as a
literal instead of a `ui::spacing` constant, and a hand-built `ToolbarView::new()` (use `dialog_shell` for dialogs).
Baselines are in baselines/ui-pattern-baselines.txt; lower a count when you remove one, never raise it.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "baselines/ui-pattern-baselines.txt"
SCAN = ("apps/linux/app/src", "apps/gpui/src")
GPUI_PX = re.compile(r"\bpx\(\s*(\d+(?:\.\d*)?)")
NAMES = r"(?:margin_top|margin_bottom|margin_start|margin_end|spacing|child_spacing|line_spacing|column_spacing|row_spacing)"
SPACING = re.compile(rf"\b(?:set_)?{NAMES}(?:\((\d+)\)|: (\d+),)|Box::new\(gtk::Orientation::\w+, (\d+)\)")
RAW_COLOUR = re.compile(r"\brgba?\(0x|\bhsla\(")
TEXT_SIZE_CLASS = re.compile(r"\.text_(?:xs|sm)\(\)")
EMPTY_LABEL = re.compile(r"\b(?:ghost|button|primary_button)\([^)]*,\s*\"\"")
SHADOW = re.compile(r"\.shadow_(?:sm|md|lg|xl|2xl)\(\)")
RAW_COLOUR_ALLOWED = {"type_colors.rs"}
SHADOW_ALLOWED = {"surfaces.rs"}
PATTERNS = {
    "thread_local": lambda line: "thread_local!" in line,
    "literal-spacing": lambda line: any(int(a or b or c) != 0 for a, b, c in SPACING.findall(line)),
    "hand-built-toolbar": lambda line: "ToolbarView::new()" in line,
    "gpui-literal-px": lambda line: any(float(number) != 0 for number in GPUI_PX.findall(line)),
}
GPUI_PATTERNS = {
    "gpui-raw-colour": lambda line, name: name not in RAW_COLOUR_ALLOWED and bool(RAW_COLOUR.search(line)),
    "gpui-text-size-class": lambda line, name: len(TEXT_SIZE_CLASS.findall(line)),
    "gpui-empty-label": lambda line, name: bool(EMPTY_LABEL.search(line)),
    "gpui-info-colour": lambda line, name: "theme.info" in line,
    "gpui-shadow": lambda line, name: name not in SHADOW_ALLOWED and bool(SHADOW.search(line)),
}


def is_test_file(path: Path) -> bool:
    return (
        path.name.endswith("_tests.rs")
        or path.name == "tests.rs"
        or "tests" in path.parts
        or path.name == "theme.rs" and "gpui" in path.parts
    )


def without_test_items(lines: list[str]) -> list[str]:
    kept = []
    skipping = False
    depth = 0
    opened = False
    for line in lines:
        if not skipping and line.strip().startswith("#[cfg(test)]"):
            skipping, depth, opened = True, 0, False
            continue
        if not skipping:
            kept.append(line)
            continue
        if not opened and line.strip().startswith("#["):
            continue
        depth += line.count("{") - line.count("}")
        opened = opened or "{" in line
        if (opened and depth <= 0) or (not opened and line.rstrip().endswith(";")):
            skipping = False
    return kept


def count(path: Path) -> dict[str, int]:
    found: dict[str, int] = {}
    gpui = "gpui" in path.parts
    for line in without_test_items(path.read_text().splitlines()):
        for name, matches in PATTERNS.items():
            if matches(line):
                found[name] = found.get(name, 0) + 1
        if gpui:
            for name, matches in GPUI_PATTERNS.items():
                hits = int(matches(line, path.name))
                if hits:
                    found[name] = found.get(name, 0) + hits
    return found


def counts() -> dict[str, int]:
    found = {}
    for base in SCAN:
        for path in sorted((ROOT / base).rglob("*.rs")):
            if is_test_file(path):
                continue
            for name, number in count(path).items():
                found[f"{path.relative_to(ROOT)} {name}"] = number
    return found


def read_baseline() -> dict[str, int]:
    entries = {}
    for line in BASELINE.read_text().splitlines():
        if line and not line.startswith("#"):
            key, number = line.rsplit(" ", 1)
            entries[key] = int(number)
    return entries


def main() -> int:
    current = counts()
    if "--update" in sys.argv:
        header = [line for line in BASELINE.read_text().splitlines() if line.startswith("#")] if BASELINE.exists() else []
        BASELINE.write_text("\n".join([*header, *(f"{key} {number}" for key, number in current.items())]) + "\n")
        return 0
    baseline = read_baseline()
    problems = [
        f"{key}: {number} (baseline {baseline.get(key, 0)}); see docs/ui-design.md"
        for key, number in current.items()
        if number > baseline.get(key, 0)
    ]
    problems += [
        f"{key}: {current.get(key, 0)} (baseline {number}); lower the baseline with --update"
        for key, number in baseline.items()
        if current.get(key, 0) < number
    ]
    for problem in problems:
        print(f"error: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"ui patterns: ok ({sum(current.values())} in {len(current)} places)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
