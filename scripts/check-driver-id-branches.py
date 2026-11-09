#!/usr/bin/env python3
"""Ratchet the number of places that branch on an engine id instead of asking a driver capability.

ADR 0041 keeps behaviour decisions in UI-free code, and `DriverCapabilities` is where an engine declares what
it can do. A non-test line that mentions `driver` and compares it with a quoted engine id counts as one branch.
Baselines are in baselines/driver-id-branch-baselines.txt; lower a count when you remove a branch, never raise it.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "baselines/driver-id-branch-baselines.txt"
SCAN = ("apps/linux/app/src", "crates/app-services/src")
ENGINES = "postgres|postgresql|mysql|mariadb|sqlite|duckdb|redis|mongodb|mssql|clickhouse"
BRANCH = re.compile(rf'(driver[^\n"]*(==|!=|eq_ignore_ascii_case)|matches!\([^\n]*driver)[^\n]*"({ENGINES})"')
FIELD_INIT = re.compile(rf'^\s*driver(_id)?: "({ENGINES})"')


def is_test_file(path: Path) -> bool:
    return path.name.endswith("_tests.rs") or path.name == "tests.rs" or "tests" in path.parts


def count(path: Path) -> int:
    total = 0
    for line in path.read_text().splitlines():
        if line.strip().startswith("#[cfg(test)]"):
            break
        if BRANCH.search(line) and not FIELD_INIT.match(line):
            total += 1
    return total


def counts() -> dict[str, int]:
    found = {}
    for base in SCAN:
        for path in sorted((ROOT / base).rglob("*.rs")):
            if is_test_file(path):
                continue
            number = count(path)
            if number:
                found[str(path.relative_to(ROOT))] = number
    return found


def read_baseline() -> dict[str, int]:
    entries = {}
    for line in BASELINE.read_text().splitlines():
        if line and not line.startswith("#"):
            name, number = line.rsplit(" ", 1)
            entries[name] = int(number)
    return entries


def main() -> int:
    current = counts()
    if "--update" in sys.argv:
        header = [line for line in BASELINE.read_text().splitlines() if line.startswith("#")] if BASELINE.exists() else []
        BASELINE.write_text("\n".join([*header, *(f"{name} {number}" for name, number in current.items())]) + "\n")
        return 0
    baseline = read_baseline()
    problems = [
        f"{name} has {number} engine-id branches (baseline {baseline.get(name, 0)}); use a driver capability"
        for name, number in current.items()
        if number > baseline.get(name, 0)
    ]
    for problem in problems:
        print(f"error: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"driver-id branches: ok ({sum(current.values())} in {len(current)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
