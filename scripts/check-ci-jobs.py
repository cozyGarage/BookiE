#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

TEST_LAYERS = json.loads((Path(__file__).with_name("test-layers.json")).read_text())
REQUIRED = set(TEST_LAYERS["gate_jobs"]) - {"regression-gate"}
PUSH_REQUIRED = set(TEST_LAYERS["push_required_jobs"])
OPTIONAL_ON_PUSH = REQUIRED - PUSH_REQUIRED


def assess(results, event):
    required = PUSH_REQUIRED if event == "push" else REQUIRED
    expected = REQUIRED | results.keys()
    failures = []
    rows = []
    for name in sorted(expected | results.keys()):
        status = results.get(name, {}).get("result", "missing")
        allowed_skip = status == "skipped" and name in OPTIONAL_ON_PUSH and event == "push"
        allowed_success = status == "success" and (name in required or event == "push")
        accepted = name in REQUIRED and (allowed_success or allowed_skip)
        if not accepted:
            failures.append(name)
        if allowed_skip:
            detail = "full gate runs on pull requests, schedules and dispatch"
        else:
            detail = status
        rows.append(f"| {name} | {detail} | {'accepted' if accepted else 'FAILED'} |")
    return failures, rows


def main():
    results = json.loads(os.environ["CI_JOB_RESULTS"])
    failures, rows = assess(results, os.environ["GITHUB_EVENT_NAME"])
    report = "## BookiE regression jobs\n\n| Job | Result | Gate |\n| --- | --- | --- |\n" + "\n".join(rows) + "\n"
    report += "\nMutation/coverage and installed Wayland acceptance are separate checks.\n"
    print(report)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as output:
            output.write(report)
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(main())
