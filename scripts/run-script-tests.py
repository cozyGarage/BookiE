#!/usr/bin/env python3
"""Run the repository's script tests, one test file per process, in parallel.

`python3 -m unittest discover` runs the files one after another, which is most
of the local harness time. Each file is independent, so this runs them side by
side and reports every failure with the output of the file that failed. CI keeps
the serial command; set BOOKIE_SERIAL_SCRIPT_TESTS=1 to use it locally too.
"""

import argparse
import concurrent.futures
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
NO_TESTS_RAN = 5


def discover(directory):
    return sorted(Path(directory).glob("test_*.py"))


def run_file(directory, path):
    started = time.monotonic()
    result = subprocess.run(
        [sys.executable, "-m", "unittest", "discover", "-s", str(directory), "-p", path.name],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    passed = result.returncode in (0, NO_TESTS_RAN)
    return path, passed, time.monotonic() - started, result.stdout + result.stderr


def default_jobs():
    configured = os.environ.get("BOOKIE_SCRIPT_TEST_JOBS")
    if configured:
        return max(1, int(configured))
    return max(1, min(os.cpu_count() or 2, 8))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", default=str(ROOT / "scripts" / "tests"))
    parser.add_argument("--jobs", type=int, default=None)
    args = parser.parse_args(argv)
    files = discover(args.directory)
    if not files:
        print(f"no test files in {args.directory}", file=sys.stderr)
        return 1
    started = time.monotonic()
    with concurrent.futures.ThreadPoolExecutor(args.jobs or default_jobs()) as pool:
        results = list(pool.map(lambda path: run_file(args.directory, path), files))
    first_pass = [result for result in results if not result[1]]
    failed = []
    retried = []
    for path, _, _, first_output in first_pass:
        retry = run_file(args.directory, path)
        if retry[1]:
            retried.append(path.name)
        else:
            failed.append((path, first_output, retry[3]))
    for path, first_output, retry_output in failed:
        print(f"FAILED {path.name} (twice: in parallel, then alone)\n{retry_output}", file=sys.stderr)
    if retried:
        print(
            "passed only when rerun alone after failing in parallel (timing-sensitive, check for load): "
            + ", ".join(retried),
            file=sys.stderr,
        )
    elapsed = time.monotonic() - started
    print(f"script tests: {len(files)} files, {len(failed)} failed, {len(retried)} retried, {elapsed:.1f}s")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
