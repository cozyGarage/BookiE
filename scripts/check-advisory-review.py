#!/usr/bin/env python3
from datetime import date
from pathlib import Path
import re
import sys
import tomllib


def problems(entries, today):
    errors = []
    for entry in entries:
        if not isinstance(entry, dict) or not isinstance(entry.get("id"), str) or not isinstance(entry.get("reason"), str):
            errors.append("advisory ignores must have an id and reason with a review date")
            continue
        reason = entry["reason"]
        match = re.fullmatch(r"(.+\S)\s*;\s*review-by:\s*(\d{4}-\d{2}-\d{2})", reason)
        if not match or "review-by:" in match.group(1):
            errors.append(f"{entry['id']}: reason must end with '; review-by: YYYY-MM-DD'")
            continue
        try:
            review = date.fromisoformat(match.group(2))
        except ValueError:
            errors.append(f"{entry['id']}: invalid review date")
            continue
        if review < today:
            errors.append(f"{entry['id']}: review date {review} has passed")
    return errors


def main():
    config = Path(__file__).resolve().parents[1] / "deny.toml"
    try:
        entries = tomllib.loads(config.read_text())["advisories"]["ignore"]
        if not isinstance(entries, list):
            raise ValueError("advisory ignore must be a list")
        errors = problems(entries, date.today())
    except (OSError, ValueError, KeyError) as error:
        print(f"advisory review check failed: {error}", file=sys.stderr)
        return 1
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("advisory review dates: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
