#!/usr/bin/env python3
"""Plan which shards the nightly workflow runs today.

Usage: ci-nightly-plan.py --commit SHA [--weekday NAME|0-6] [--last-green SHA | --lookup]
                          [--changed-file FILE | --changed-from-git] [--force]
                          [--plan-only] [--github-output FILE] [--api URL]

Prints the plan as JSON. The rotation tables below are the single place to edit what runs
on which night; scripts/tests/test_ci_nightly_plan.py keeps them complete. Weekdays count
from Monday as 0, and Sunday is the full run.
"""
import argparse
import datetime
import json
import os
import re
import subprocess
import sys
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_API = "http://10.61.0.11:3000/api/v1/repos/Bookie/bookie"
WORKFLOW_FILE = "nightly.yml"
GREEN_MARKER_JOB = "coverage"
FULL_WEEKDAY = 6
WEEKDAY_NAMES = ("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday")

MAX_PARALLEL = 3
BUDGET_MINUTES = {"weeknight": 420, "full": 960}
JOB_TIMEOUT_MINUTES = {
    "plan": 15,
    "coverage": 90,
    "docker-coverage": 120,
    "mutation": 120,
    "values": 60,
    "gtk-soak": 90,
    "arch-package": 60,
}

DRIVERS = ("postgres", "mysql", "sqlite", "mongodb", "redis", "clickhouse")
DOCKER_ROTATION = {
    0: ("postgres", "mysql"),
    2: ("sqlite", "mongodb"),
    4: ("redis", "clickhouse"),
}

POSTGRES_FILES = (
    "crates/drivers/postgres/src/array.rs",
    "crates/drivers/postgres/src/numeric.rs",
    "crates/drivers/postgres/src/temporal.rs",
    "crates/drivers/postgres/src/decode.rs",
)
MUTATION_SHARDS = {
    "core-1": {"package": "bookie-core", "shard": "0/4", "dir": "crates/core"},
    "core-2": {"package": "bookie-core", "shard": "1/4", "dir": "crates/core"},
    "core-3": {"package": "bookie-core", "shard": "2/4", "dir": "crates/core"},
    "core-4": {"package": "bookie-core", "shard": "3/4", "dir": "crates/core"},
    "policy-1": {"package": "bookie-policy", "shard": "0/2", "dir": "crates/policy"},
    "policy-2": {"package": "bookie-policy", "shard": "1/2", "dir": "crates/policy"},
    "ssh": {"package": "bookie-ssh", "shard": "0/1", "dir": "crates/ssh"},
    "redis": {"package": "bookie-driver-redis", "shard": "0/1", "dir": "crates/drivers/redis"},
    "postgres": {
        "package": "bookie-driver-postgres",
        "shard": "0/1",
        "dir": "crates/drivers/postgres",
        "files": POSTGRES_FILES,
    },
    "agentd": {"package": "bookie-agentd", "shard": "0/1", "dir": "crates/agentd"},
    "transport": {"package": "bookie-transport", "shard": "0/1", "dir": "crates/transport"},
    "app-services": {"package": "bookie-app-services", "shard": "0/1", "dir": "crates/app-services"},
}
MUTATION_ROTATION = {
    0: ("core-1", "core-2"),
    1: ("core-3", "core-4"),
    2: ("policy-1", "policy-2"),
    3: ("agentd", "app-services"),
    4: ("transport", "ssh"),
    5: ("redis", "postgres"),
}

VALUES_WEEKDAYS = (0, 1, 2, 3, 4, 5, 6)
GTK_SOAK_WEEKDAYS = (6,)
GTK_SOAK_RUNS = (1, 2, 3)
ARCH_PACKAGE_WEEKDAYS = (6,)

DOC_PREFIXES = ("docs/", "changelog.d/")
DOC_SUFFIXES = (".md",)
SAFE_PATH = re.compile(r"[A-Za-z0-9_./-]+")
MUTATION_BASELINE_DIR = ROOT / "baselines" / "mutants"


def is_documentation(path):
    return path.endswith(DOC_SUFFIXES) or path.startswith(DOC_PREFIXES)


def mutation_baselines(directory=MUTATION_BASELINE_DIR):
    if not Path(directory).is_dir():
        return frozenset()
    return frozenset(item.stem for item in Path(directory).glob("*.txt"))


def rotation_for(table, weekday):
    return tuple(table.get(weekday, ()))


def skip_reason(weekday, commit, last_green, changed, force):
    if force or weekday == FULL_WEEKDAY:
        return None
    if last_green and last_green == commit:
        return "nothing changed since the last green nightly"
    if changed is None:
        return None
    if not changed and last_green:
        return "nothing changed since the last green nightly"
    if changed and all(is_documentation(path) for path in changed):
        return "only documentation changed since the last green nightly"
    return None


def scheduled_mutation_labels(weekday, full):
    if full:
        return tuple(MUTATION_SHARDS)
    return rotation_for(MUTATION_ROTATION, weekday)


def mutation_entry(label, files, partial):
    spec = MUTATION_SHARDS[label]
    return {
        "label": label,
        "package": spec["package"],
        "shard": spec["shard"],
        "files": " ".join(files),
        "partial": partial,
    }


def plan_mutation(labels, changed, baselines, full):
    limit = not full and changed is not None
    entries = []
    seen = set()
    for label in labels:
        spec = MUTATION_SHARDS[label]
        if limit and spec["package"] in baselines:
            if spec["package"] in seen:
                continue
            seen.add(spec["package"])
            files = sorted(
                path
                for path in changed
                if path.endswith(".rs") and path.startswith(spec["dir"] + "/") and SAFE_PATH.fullmatch(path)
            )
            if files:
                entries.append(
                    {**mutation_entry(label, files, True), "shard": "0/1"}
                )
            continue
        entries.append(mutation_entry(label, spec.get("files", ()), spec["shard"] != "0/1"))
    return entries


def waves(count):
    return -(-count // MAX_PARALLEL)


def estimate_minutes(plan):
    timeouts = JOB_TIMEOUT_MINUTES
    total = timeouts["plan"]
    if plan["skip"]:
        return total
    if plan["coverage"]:
        total += timeouts["coverage"]
    total += waves(len(plan["drivers"])) * timeouts["docker-coverage"]
    total += waves(len(plan["mutation"])) * timeouts["mutation"]
    tail = [0]
    if plan["values"]:
        tail.append(timeouts["values"])
    if plan["gtk_soak"]:
        tail.append(waves(len(plan["gtk_soak"])) * timeouts["gtk-soak"])
    if plan["arch_package"]:
        tail.append(timeouts["arch-package"])
    return total + max(tail)


def plan_nightly(weekday, commit, last_green=None, changed=None, force=False, baselines=frozenset()):
    full = bool(force) or weekday == FULL_WEEKDAY
    reason = skip_reason(weekday, commit, last_green, changed, force)
    plan = {
        "skip": reason is not None,
        "reason": reason,
        "weekday": WEEKDAY_NAMES[weekday],
        "full": full,
        "commit": commit,
        "last_green": last_green or None,
        "max_parallel": MAX_PARALLEL,
        "coverage": False,
        "drivers": [],
        "mutation": [],
        "values": False,
        "gtk_soak": [],
        "arch_package": False,
    }
    if reason is None:
        plan["coverage"] = True
        plan["drivers"] = list(DRIVERS if full else rotation_for(DOCKER_ROTATION, weekday))
        plan["mutation"] = plan_mutation(scheduled_mutation_labels(weekday, full), changed, baselines, full)
        plan["values"] = full or weekday in VALUES_WEEKDAYS
        plan["gtk_soak"] = list(GTK_SOAK_RUNS) if full or weekday in GTK_SOAK_WEEKDAYS else []
        plan["arch_package"] = full or weekday in ARCH_PACKAGE_WEEKDAYS
        if last_green is None or changed is None:
            plan["reason"] = "last green nightly unknown: running everything planned for today"
        elif full:
            plan["reason"] = "full run"
        else:
            plan["reason"] = "changes since the last green nightly: running today's rotation"
    plan["budget_minutes"] = BUDGET_MINUTES["full" if full else "weeknight"]
    plan["estimated_worst_case_minutes"] = estimate_minutes(plan)
    return plan


def http_get_json(url, token=None, timeout=20):
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    if token:
        request.add_header("Authorization", f"token {token}")
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.load(response)


def job_list(payload):
    return payload.get("jobs", []) if isinstance(payload, dict) else payload


def is_real_green(run, fetch, api, token):
    jobs = job_list(fetch(f"{api}/actions/runs/{run['id']}/jobs", token))
    return any(job.get("name") == GREEN_MARKER_JOB and job.get("status") == "success" for job in jobs)


def find_last_green(api, token=None, fetch=http_get_json, pages=6, candidates=10):
    checked = 0
    for page in range(1, pages + 1):
        query = urllib.parse.urlencode({"status": "success", "limit": 50, "page": page})
        runs = fetch(f"{api}/actions/runs?{query}", token).get("workflow_runs", [])
        if not runs:
            return None
        nightly = sorted(
            (run for run in runs if run.get("workflow_id") == WORKFLOW_FILE), key=lambda run: -run["id"]
        )
        for run in nightly:
            if checked >= candidates:
                return None
            checked += 1
            if is_real_green(run, fetch, api, token):
                return run["commit_sha"]
    return None


def lookup_last_green(api, environ=None, fetch=http_get_json):
    environ = os.environ if environ is None else environ
    token = environ.get("FORGEJO_TOKEN") or environ.get("GITHUB_TOKEN") or None
    try:
        return find_last_green(api, token, fetch)
    except Exception as error:
        print(f"last green lookup unavailable: {type(error).__name__}", file=sys.stderr)
        return None


def changed_from_git(last_green, commit, run=subprocess.run):
    try:
        result = run(
            ["git", "diff", "--name-only", f"{last_green}..{commit}"],
            capture_output=True,
            text=True,
            check=True,
            cwd=ROOT,
        )
    except (OSError, subprocess.CalledProcessError):
        print("changed paths unavailable: git diff failed", file=sys.stderr)
        return None
    return [line for line in result.stdout.splitlines() if line]


def read_changed_file(name):
    text = sys.stdin.read() if name == "-" else Path(name).read_text(encoding="utf-8")
    return [line.strip() for line in text.splitlines() if line.strip()]


def parse_weekday(value):
    if value is None:
        return datetime.datetime.now(datetime.timezone.utc).weekday()
    lowered = str(value).strip().lower()
    for index, name in enumerate(WEEKDAY_NAMES):
        if lowered in (name.lower(), name[:3].lower(), str(index)):
            return index
    raise ValueError(f"unknown weekday: {value}")


def output_lines(plan, plan_only):
    return [
        f"skip={'true' if plan['skip'] or plan_only else 'false'}",
        f"reason={plan['reason']}",
        f"coverage={'true' if plan['coverage'] else 'false'}",
        f"drivers={json.dumps(plan['drivers'], separators=(',', ':'))}",
        f"mutation={json.dumps(plan['mutation'], separators=(',', ':'))}",
        f"values={'true' if plan['values'] else 'false'}",
        f"gtk_soak={json.dumps(plan['gtk_soak'], separators=(',', ':'))}",
        f"arch_package={'true' if plan['arch_package'] else 'false'}",
    ]


def build_parser():
    parser = argparse.ArgumentParser(description="Plan the nightly shards.")
    parser.add_argument("--commit", required=True)
    parser.add_argument("--weekday")
    parser.add_argument("--last-green", default="")
    parser.add_argument("--lookup", action="store_true")
    parser.add_argument("--api", default=os.environ.get("FORGEJO_API", DEFAULT_API))
    parser.add_argument("--changed-file")
    parser.add_argument("--changed-from-git", action="store_true")
    parser.add_argument("--force", "--full", dest="force", action="store_true")
    parser.add_argument("--plan-only", action="store_true")
    parser.add_argument("--github-output")
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        weekday = parse_weekday(args.weekday)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 2
    last_green = args.last_green or None
    if last_green is None and args.lookup and not args.force:
        last_green = lookup_last_green(args.api)
    changed = None
    if args.changed_file:
        changed = read_changed_file(args.changed_file)
    elif args.changed_from_git and last_green:
        changed = changed_from_git(last_green, args.commit)
    plan = plan_nightly(
        weekday, args.commit, last_green, changed, args.force, mutation_baselines()
    )
    plan["plan_only"] = args.plan_only
    print(json.dumps(plan, indent=2, sort_keys=True))
    if args.github_output:
        with open(args.github_output, "a", encoding="utf-8") as handle:
            handle.write("\n".join(output_lines(plan, args.plan_only)) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
