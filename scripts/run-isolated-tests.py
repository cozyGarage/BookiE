#!/usr/bin/env python3
"""Run each display/keyring test in its own process and reject empty selections."""
import importlib.util
import os
import json
from pathlib import Path
import subprocess
import sys

from rust_test_evidence import completed_tests

root = Path(__file__).resolve().parents[1]
registry = json.loads((root / "scripts/isolated-tests.json").read_text())
entries = registry[sys.argv[1]]
if not entries:
    raise SystemExit("empty isolated tier")
bundle = os.environ.get('BOOKIE_TEST_BUNDLE')
if bundle:
    spec = importlib.util.spec_from_file_location('prebuilt_tests', root / 'scripts/run-prebuilt-tests.py')
    prebuilt = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prebuilt)
    index = prebuilt.load_index(Path(bundle))
    os.environ.update(prebuilt.environment(Path(bundle), index))
if os.environ.get('BOOKIE_REQUIRE_PREBUILT') == '1' and not bundle:
    raise SystemExit('a source-matched prebuilt test bundle is required')
for package, target, name in entries:
    selection = [target] if target.startswith("--") else ["--test", target]
    command = ["cargo", "test", "--locked", "-p", package, *selection,
               "--", "--ignored", "--exact", name, "--test-threads=1"]
    if bundle:
        selected = prebuilt.commands(Path(bundle), command[2:], index)
        if len(selected) != 1:
            raise SystemExit('isolated test must select exactly one prebuilt executable')
        command = selected[0]
    result = subprocess.run(command, cwd=root, text=True, capture_output=True)
    print(result.stdout, flush=True)
    print(result.stderr, file=sys.stderr, flush=True)
    if result.returncode or not completed_tests(result.stdout, [name]):
        raise SystemExit(f"isolated test failed or did not execute exactly once: {name}")
print(f"{sys.argv[1]}: {len(entries)} tests executed successfully")
