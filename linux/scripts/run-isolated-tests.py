#!/usr/bin/env python3
"""Run each display/keyring test in its own process and reject empty selections."""
import json
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
registry = json.loads((root / "scripts/isolated-tests.json").read_text())
entries = registry[sys.argv[1]]
if not entries:
    raise SystemExit("empty isolated tier")
for package, target, name in entries:
    selection = [target] if target.startswith("--") else ["--test", target]
    command = ["cargo", "test", "--locked", "-p", package, *selection,
               "--", "--ignored", "--exact", name, "--test-threads=1"]
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    if result.returncode or "test result: ok. 1 passed;" not in result.stdout:
        raise SystemExit(f"isolated test failed or did not execute exactly once: {name}")
print(f"{sys.argv[1]}: {len(entries)} tests executed successfully")
