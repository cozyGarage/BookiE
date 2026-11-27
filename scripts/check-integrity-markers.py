#!/usr/bin/env python3
import argparse
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

REPOSITORY = "cozyGarage/BookiE"
REQUEST_TIMEOUT_SECONDS = 30
CORPUS_DIR = Path(__file__).resolve().parents[1] / "tests/data/integrity"


def marker_issues(corpus_dir):
    issues = {}
    for path in sorted(Path(corpus_dir).glob("*.toml")):
        corpus = tomllib.loads(path.read_text(encoding="utf-8"))
        markers = [("corpus", marker) for marker in corpus.get("known_failing", [])]
        for case in corpus.get("case", []):
            markers.extend((case.get("id", "?"), marker) for marker in case.get("known_failing", []))
        for owner, marker in markers:
            issues.setdefault(int(marker["issue"]), []).append(f"{path.name}:{owner}")
    return issues


def issue_state(number, repository, runner=None):
    run = subprocess.run if runner is None else runner
    try:
        result = run(
            ["gh", "issue", "view", str(number), "--repo", repository, "--json", "state"],
            capture_output=True,
            text=True,
            timeout=REQUEST_TIMEOUT_SECONDS,
            check=False,
        )
    except FileNotFoundError:
        return None, "gh CLI is not installed"
    except subprocess.TimeoutExpired:
        return None, f"gh issue view timed out after {REQUEST_TIMEOUT_SECONDS} seconds"
    if result.returncode != 0:
        return None, f"gh issue view #{number} failed: {result.stderr.strip() or result.returncode}"
    try:
        state = json.loads(result.stdout).get("state")
    except (json.JSONDecodeError, AttributeError):
        return None, f"gh issue view #{number} returned invalid JSON"
    if not isinstance(state, str) or state.upper() not in {"OPEN", "CLOSED"}:
        return None, f"gh issue view #{number} returned no valid state"
    return state.upper(), None


def main(argv=None, runner=None, corpus_dir=CORPUS_DIR):
    parser = argparse.ArgumentParser(description="Check that data-integrity known_failing markers point at open issues")
    parser.add_argument("--repo", default=REPOSITORY, help="GitHub repository in OWNER/NAME form")
    args = parser.parse_args(argv)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repo):
        parser.error("--repo must be OWNER/NAME")
    issues = marker_issues(corpus_dir)
    closed = []
    for number, owners in sorted(issues.items()):
        state, error = issue_state(number, args.repo, runner)
        if error is not None:
            print(f"GitHub issue status unavailable: {error}", file=sys.stderr)
            return 2
        if state != "OPEN":
            closed.append(f"#{number} is {state.lower()} but still marks {', '.join(owners)}")
    for problem in closed:
        print(f"data-integrity marker: {problem}", file=sys.stderr)
    if closed:
        return 1
    print(f"data-integrity markers: {len(issues)} issues verified open")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
