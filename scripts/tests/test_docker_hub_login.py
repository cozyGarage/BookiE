import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "docker-hub-login.sh"
STUB = """#!/usr/bin/env bash
cat >/dev/null
echo login >> "$STUB_LOG"
count=$(wc -l < "$STUB_LOG")
[ "$count" -gt "$STUB_FAIL_FIRST" ]
"""


def run_login(fail_first):
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        docker = root / "docker"
        docker.write_text(STUB)
        docker.chmod(docker.stat().st_mode | stat.S_IXUSR)
        sleep = root / "sleep"
        sleep.write_text("#!/bin/sh\nexit 0\n")
        sleep.chmod(sleep.stat().st_mode | stat.S_IXUSR)
        log = root / "calls"
        environment = dict(
            os.environ,
            PATH=f"{root}:{os.environ['PATH']}",
            STUB_LOG=str(log),
            STUB_FAIL_FIRST=str(fail_first),
            DOCKERHUB_USERNAME="test-user",
            DOCKERHUB_TOKEN="test-token",
        )
        result = subprocess.run(["bash", str(SCRIPT)], env=environment, capture_output=True, text=True)
        return result, log.read_text().splitlines()


class DockerHubLoginTests(unittest.TestCase):
    def test_transient_login_failure_is_retried(self):
        result, calls = run_login(1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, ["login", "login"])

    def test_persistent_login_failure_stops_after_three_attempts(self):
        result, calls = run_login(99)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, ["login", "login", "login"])


if __name__ == "__main__":
    unittest.main()
