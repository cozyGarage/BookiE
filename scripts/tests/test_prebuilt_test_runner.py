import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("prebuilt_tests", ROOT / "scripts/run-prebuilt-tests.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class PrebuiltTestRunnerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binary = self.root / "test-program"
        self.binary.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\"\n")
        self.binary.chmod(0o755)
        self.index = {"schema_version": 1, "source_commit": "a" * 40, "tests": [
            {"package": "sample", "kind": "test", "name": "integration", "path": "test-program"}
        ], "build_group": "default", "files": {"test-program": {"sha256": hashlib.sha256(self.binary.read_bytes()).hexdigest(), "size": self.binary.stat().st_size}}, "binaries": {}, "package_directories": {"sample": "crates/sample"}}
        (self.root / "index.json").write_text(json.dumps(self.index))

    def test_preserves_libtest_filters_and_single_process_fixture_lifetime(self):
        args = ["--locked", "-p", "sample", "--test", "integration", "rollback", "--", "--include-ignored", "--test-threads=1"]
        with patch.object(runner, "ROOT", self.root), patch.dict(os.environ, {"GITHUB_SHA": "a" * 40}, clear=True):
            command = runner.commands(self.root, args)[0]
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout.splitlines(), ["rollback", "--include-ignored", "--test-threads=1"])

    def test_missing_target_fails_instead_of_building_or_running_zero_tests(self):
        with self.assertRaises(ValueError):
            runner.commands(self.root, ["-p", "sample", "--test", "missing"])

    def test_another_source_revision_is_rejected(self):
        with patch.dict(os.environ, {"GITHUB_SHA": "b" * 40}, clear=True), self.assertRaises(ValueError):
            runner.commands(self.root, ["-p", "sample", "--test", "integration"])

    def test_executable_cannot_escape_the_verified_bundle(self):
        self.index["tests"][0]["path"] = "../outside"
        (self.root / "index.json").write_text(json.dumps(self.index))
        with self.assertRaises(ValueError):
            runner.commands(self.root, ["-p", "sample", "--test", "integration"])

    def test_modified_executable_is_rejected(self):
        self.binary.write_text('#!/bin/sh\nexit 0\n')
        with self.assertRaises(ValueError):
            runner.commands(self.root, ['-p', 'sample', '--test', 'integration'])

    def test_missing_package_cannot_be_silently_skipped(self):
        with self.assertRaises(ValueError):
            runner.commands(self.root, ['-p', 'sample', '-p', 'missing', '--tests'])

    def test_wildcard_selects_integration_targets(self):
        command = runner.commands(self.root, ['-p', 'sample', '--test', '*'])[0]
        self.assertEqual(command, [str(self.binary)])

    def test_unknown_build_options_are_rejected(self):
        with self.assertRaises(ValueError):
            runner.commands(self.root, ["-p", "sample", "--target", "another-architecture"])


if __name__ == "__main__":
    unittest.main()
