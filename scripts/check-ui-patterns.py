#!/usr/bin/env python3
"""Ratchet three UI patterns that docs/ui-design.md says to stop growing in the GTK app.

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
GPUI_PX = re.compile(r"\bpx\((\d+(?:\.\d*)?)\)")
NAMES = r"(?:margin_top|margin_bottom|margin_start|margin_end|spacing|child_spacing|line_spacing|column_spacing|row_spacing)"
SPACING = re.compile(rf"\b(?:set_)?{NAMES}(?:\((\d+)\)|: (\d+),)|Box::new\(gtk::Orientation::\w+, (\d+)\)")
PATTERNS = {
    "thread_local": lambda line: "thread_local!" in line,
    "literal-spacing": lambda line: any(int(a or b or c) != 0 for a, b, c in SPACING.findall(line)),
    "hand-built-toolbar": lambda line: "ToolbarView::new()" in line,
    "gpui-literal-px": lambda line: any(float(number) != 0 for number in GPUI_PX.findall(line)),
}


def is_test_file(path: Path) -> bool:
    return (
        path.name.endswith("_tests.rs")
        or path.name == "tests.rs"
        or "tests" in path.parts
        or path.name == "theme.rs" and "gpui" in path.parts
    )


def count(path: Path) -> dict[str, int]:
    found: dict[str, int] = {}
    for line in path.read_text().splitlines():
        if line.strip().startswith("#[cfg(test)]"):
            break
        for name, matches in PATTERNS.items():
            if matches(line):
                found[name] = found.get(name, 0) + 1
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
    for problem in problems:
        print(f"error: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"ui patterns: ok ({sum(current.values())} in {len(current)} places)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
