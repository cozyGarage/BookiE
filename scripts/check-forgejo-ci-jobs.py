#!/usr/bin/env python3
import json
import os
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
    "driver",
    "b4-rollback",
    "e2e",
    "security-policy",
    "consumers",
    "gtk",
    "pg-release",
    "gtk-server",
    "driver-tls",
    "keyring",
    "supply-chain",
}
OPTIONAL_ON_NON_MAIN = {"floor-build", "distro-floor-scenarios"}


def failures(results, ref, event):
    optional = (
        OPTIONAL_ON_NON_MAIN
        if ref != "refs/heads/main" and event not in {"schedule", "workflow_dispatch"}
        else set()
    )
    expected = REQUIRED | OPTIONAL_ON_NON_MAIN
    failed = {name: "unexpected" for name in results.keys() - expected}
    for name in expected:
        result = results.get(name, {}).get("result", "missing")
        if result != "success" and not (name in optional and result == "skipped"):
            failed[name] = result
    return failed


def main():
    results = json.loads(os.environ["CI_JOB_RESULTS"])
    failed = failures(results, os.environ["GITHUB_REF"], os.environ["GITHUB_EVENT_NAME"])
    if failed:
        for name, result in sorted(failed.items()):
            print(f"{name}: {result}")
        return 1
    print(f"all {len(results)} Forgejo CI jobs completed and finalized")
    return 0


if __name__ == "__main__":
    sys.exit(main())
