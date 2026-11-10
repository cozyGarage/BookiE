#!/usr/bin/env python3
import json
import os
import sys


def failures(results, ref, event):
    optional = (
        {"floor-build", "distro-floor-scenarios"}
        if ref != "refs/heads/main" and event not in {"schedule", "workflow_dispatch"}
        else set()
    )
    return {
        name: job.get("result", "missing")
        for name, job in results.items()
        if job.get("result") != "success" and not (name in optional and job.get("result") == "skipped")
    }


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
