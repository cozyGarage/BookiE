#!/usr/bin/env python3
"""Tag a failed Forgejo CI job as infrastructure, GUI timing or code.

Usage:
  classify-ci-failure.py --log FILE
  classify-ci-failure.py --run RUN_NUMBER (reads FORGEJO_API and FORGEJO_TOKEN_FILE like forgejo-gate.sh)

RUN_NUMBER is the number in the run page URL (.../actions/runs/N), which is the
run's index in the repository. The API id is a different, larger number; pass
--api-id to give that one instead. --list prints both.

The tag is advice for the person reading a red run: infrastructure failures are
retried before anyone reads code, code failures are not. It never changes a result.

Tags: infra (retry), gui-timing (the window did not appear; retry), gui (a GUI
assertion failed; ambiguous, because a missing accessible node can be a real
regression as well as load), code (read the log), unknown.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import urllib.request
from pathlib import Path

TAIL_BYTES = 400_000
RUN_LOOKUP_LIMIT = 200

SIGNATURES = [
    ("infra", "image pull timed out", r"PullImage.*RequestTimeoutError"),
    ("infra", "container startup timed out", r"StartupTimeout"),
    ("infra", "database fixture did not become ready", r"fixture did not become ready"),
    ("infra", "database fixture refused the connection", r"connect: ConnectionRefused"),
    ("infra", "a test container did not become ready in time (runner load)", r"must become ready within \d+ seconds"),
    ("infra", "Docker Hub rate limit", r"(?i:429\s+too many requests|toomanyrequests)"),
    ("infra", "compiler crashed", r"Compiler killed by signal|rustc.*SIGSEGV|signal: 11"),
    ("infra", "runner volume path missing on the Docker host", r"bind source path does not exist"),
    ("infra", "the accessibility service could not start", r"Failed to execute program org\.a11y\.Bus"),
    ("infra", "network failure", r"Temporary failure in name resolution|Network is unreachable|Could not resolve host"),
    ("infra", "disk full", r"No space left on device|Out of diskspace"),
    ("infra", "the linker died on a bus error (cache or target volume problem)", r"ld terminated with signal 7|\[Bus error\]"),
    ("infra", "a fixture image is missing on the executor", r"No such image:"),
    ("infra", "a fixture container exited before the tests ran", r"container \S+ exited \(\d+\)"),
    ("code", "application crashed", r"application exited with -\d+"),
    ("code", "layout abort at startup", r"Gtk-CRITICAL.*for_size"),
    ("gui-timing", "window did not appear", r"application did not expose its expected window"),
    ("code", "compile error", r"error\[E\d+\]"),
    ("code", "test panicked", r"panicked at"),
    ("code", "test failed", r"test result: FAILED|\.\.\. FAILED"),
    ("code", "lint or format failure", r"error: could not compile|Diff in .*\.rs"),
    ("gui", "GUI assertion failed; rerun once, and treat it as code if the same scenario fails again on a quiet runner", r"expected (an )?accessible (node|name)"),
]


def classify_log(text):
    tail = text[-TAIL_BYTES:]
    for tag, label, pattern in SIGNATURES:
        match = re.search(pattern, tail)
        if match:
            line_start = tail.rfind("\n", 0, match.start()) + 1
            line_end = tail.find("\n", match.end())
            evidence = tail[line_start : line_end if line_end != -1 else len(tail)].strip()
            return tag, label, evidence[:200]
    if not re.search(r"Job (succeeded|failed)", text):
        return "infra", "the job log ends without a result (runner or Forgejo connection lost)", tail.strip().splitlines()[-1][:200] if tail.strip() else ""
    return "unknown", "no known signature", ""


def api_base():
    explicit = os.environ.get("FORGEJO_API")
    if explicit:
        return explicit
    url = subprocess.run(
        ["git", "remote", "get-url", os.environ.get("FORGEJO_REMOTE", "forgejo")],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    match = re.match(r"^(https?://)([^/@]*@)?([^/]+)/(.+)\.git$", url)
    if not match:
        raise SystemExit("cannot derive the Forgejo API from the remote; set FORGEJO_API")
    return f"{match.group(1)}{match.group(3)}/api/v1/repos/{match.group(4)}"


def fetch(url, token):
    request = urllib.request.Request(url, headers={"Authorization": f"token {token}"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read().decode("utf-8", "replace")


def read_token():
    default = Path.home() / ".config/forgejo/token"
    return Path(os.environ.get("FORGEJO_TOKEN_FILE", default)).read_text().strip()


def fetch_runs(limit):
    runs = json.loads(fetch(f"{api_base()}/actions/runs?limit={limit}", read_token()))
    return runs.get("workflow_runs", runs) if isinstance(runs, dict) else runs


def run_id_for_number(runs, number):
    for run in runs:
        if run.get("index_in_repo") == number:
            return run["id"]
    raise SystemExit(f"no run numbered {number} among the {len(runs)} most recent runs; use --api-id for an API id")


def resolve_run(number, api_id):
    return number if api_id else run_id_for_number(fetch_runs(RUN_LOOKUP_LIMIT), number)


def classify_run(number, tail=0, api_id=False):
    run_id = resolve_run(number, api_id)
    token = read_token()
    base = api_base()
    jobs = json.loads(fetch(f"{base}/actions/runs/{run_id}/jobs", token))
    jobs = jobs["jobs"] if isinstance(jobs, dict) else jobs
    results = []
    for job in jobs:
        if job["status"] != "failure":
            continue
        log = fetch(f"{base}/actions/jobs/{job['id']}/logs", token)
        tag, label, evidence = classify_log(log)
        results.append((job["name"], tag, label, evidence, log.splitlines()[-tail:] if tail else []))
    return results


def run_line(run):
    title = (run.get("display_title") or run.get("title") or "")[:60]
    number = run.get("index_in_repo", "?")
    ref = run.get("prettyref", run.get("head_branch", ""))[:28]
    api = f"(api {run.get('id', '?')})"
    return f"{number:>6} {api:<11} {run.get('status', '?'):10} {run.get('event', '?'):10} {ref:28} {title}"


def list_runs(limit):
    for run in fetch_runs(limit)[:limit]:
        print(run_line(run))


def run_jobs(number, api_id=False):
    run_id = resolve_run(number, api_id)
    token = read_token()
    base = api_base()
    jobs = json.loads(fetch(f"{base}/actions/runs/{run_id}/jobs", token))
    jobs = jobs["jobs"] if isinstance(jobs, dict) else jobs
    for job in jobs:
        print(f"{job['status']:10} {job['name']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--log", type=Path)
    group.add_argument("--run", type=int, metavar="NUMBER", help="the number in the run page URL")
    group.add_argument("--list", type=int, metavar="N", help="list the N most recent runs, any workflow, any event")
    group.add_argument("--jobs", type=int, metavar="NUMBER", help="list every job of a run with its status")
    parser.add_argument("--api-id", action="store_true", help="treat the given run number as an API id")
    parser.add_argument("--tail", type=int, default=0, help="also print the last N log lines of each failed job")
    args = parser.parse_args()
    if args.list:
        list_runs(args.list)
        return
    if args.jobs:
        run_jobs(args.jobs, args.api_id)
        return
    if args.log:
        tag, label, evidence = classify_log(args.log.read_text(errors="replace"))
        print(f"{tag}: {label}" + (f"\n  {evidence}" if evidence else ""))
        return
    results = classify_run(args.run, args.tail, args.api_id)
    if not results:
        print("no failed jobs")
        return
    for name, tag, label, evidence, tail_lines in results:
        print(f"{tag:11} {name}: {label}")
        if evidence:
            print(f"            {evidence}")
        for line in tail_lines:
            print(f"              | {line}")
    counts = {}
    for _, tag, _, _, _ in results:
        counts[tag] = counts.get(tag, 0) + 1
    print("summary: " + ", ".join(f"{count} {tag}" for tag, count in sorted(counts.items())))


if __name__ == "__main__":
    main()
