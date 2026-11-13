#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

REQUIRED = {
    "pin-revision",
    "guards",
    "gpui-tests",
    "gtk-build",
    "clippy",
    "test-build",
    "unit",
    "sandbox",
    "widgets",
    "floor-build",
    "distro-floor-scenarios",
    "driver",
    "b4-rollback",
    "security-policy",
    "consumers",
    "gtk",
    "pg-release",
    "gtk-server",
    "driver-tls",
    "keyring",
    "supply-chain",
}
OPTIONAL_ON_PULL_REQUEST = {"floor-build", "distro-floor-scenarios"}


def assess(results, event):
    expected = REQUIRED
    failures = []
    rows = []
    for name in sorted(expected | results.keys()):
        status = results.get(name, {}).get("result", "missing")
        allowed_skip = status == "skipped" and (
            name in OPTIONAL_ON_PULL_REQUEST and event == "pull_request"
        )
        accepted = name in expected and (status == "success" or allowed_skip)
        if not accepted:
            failures.append(name)
        if allowed_skip:
            detail = "not a pull-request gate"
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
