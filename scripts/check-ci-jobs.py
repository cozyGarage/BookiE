#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

REQUIRED = {
    "pin-revision",
    "preflight",
    "fast",
    "gtk-safety",
    "integration",
    "b4-rollback",
    "driver-tls",
    "postgres-release",
}
SCHEDULED = "current-stable-clippy"
MERGE_ONLY = set()
PULL_REQUEST_ONLY = set()


def assess(results, event):
    expected = REQUIRED | {SCHEDULED}
    failures = []
    rows = []
    for name in sorted(expected | results.keys()):
        status = results.get(name, {}).get("result", "missing")
        allowed_skip = name == SCHEDULED and event in {"push", "pull_request"} and status == "skipped"
        accepted = name in expected and (status == "success" or allowed_skip)
        if not accepted:
            failures.append(name)
        if allowed_skip:
            detail = "scheduled/manual only"
        else:
            detail = status
        rows.append(f"| {name} | {detail} | {'accepted' if accepted else 'FAILED'} |")
    return failures, rows


def main():
    results = json.loads(os.environ["CI_JOB_RESULTS"])
    failures, rows = assess(results, os.environ["GITHUB_EVENT_NAME"])
    report = "## Linux regression jobs\n\n| Job | Result | Gate |\n| --- | --- | --- |\n" + "\n".join(rows) + "\n"
    report += "\nMutation/coverage and installed Wayland acceptance are separate checks.\n"
    print(report)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as output:
            output.write(report)
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
