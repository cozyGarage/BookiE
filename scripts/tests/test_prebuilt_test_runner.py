import hashlib
import importlib.util
import json
import os
import platform
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
        self.enterContext(patch.dict(os.environ, {"GITHUB_SHA": "a" * 40}))
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.binary = self.root / "test-program"
        self.binary.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\"\n")
        self.binary.chmod(0o755)
        self.index = {"schema_version": 1, "source_commit": "a" * 40, "architecture": platform.machine(), "tests": [
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

    def test_another_architecture_is_rejected(self):
        self.index['architecture'] = 'different-architecture'
        (self.root / 'index.json').write_text(json.dumps(self.index))
        with self.assertRaisesRegex(ValueError, 'architecture mismatch'):
            runner.commands(self.root, ['-p', 'sample', '--test', 'integration'])

    def test_explicit_source_revision_takes_precedence_for_manual_runs(self):
        with patch.dict(os.environ, {"GITHUB_SHA": "b" * 40, "BOOKIE_SOURCE_COMMIT": "a" * 40}, clear=True):
            self.assertEqual(len(runner.commands(self.root, ["-p", "sample", "--test", "integration"])), 1)

    def test_bundle_preparation_rejects_a_different_checkout(self):
        result = subprocess.run(['bash', str(ROOT / 'scripts/prepare-test-bundle.sh'), '0' * 40, 'default'],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('differs from the checkout', result.stderr)

    def test_bundle_builder_rejects_a_different_checkout(self):
        result = subprocess.run(['bash', str(ROOT / 'scripts/build-test-bundles.sh'), str(self.root / 'out')],
                                env={**os.environ, 'BOOKIE_SOURCE_COMMIT': '0' * 40}, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('differs from the checkout', result.stderr)
        self.assertFalse((self.root / 'out').exists())

    def test_executable_cannot_escape_the_verified_bundle(self):
        self.index["tests"][0]["path"] = "../outside"
        (self.root / "index.json").write_text(json.dumps(self.index))
        with self.assertRaises(ValueError):
            runner.commands(self.root, ["-p", "sample", "--test", "integration"])

    def test_modified_executable_is_rejected(self):
        self.binary.write_text('#!/bin/sh\nexit 0\n')
        with self.assertRaises(ValueError):
            runner.commands(self.root, ['-p', 'sample', '--test', 'integration'])

    def test_unverified_helper_is_rejected_before_execution(self):
        self.index['binaries']['helper'] = 'unverified'
        (self.root / 'index.json').write_text(json.dumps(self.index))
        with self.assertRaisesRegex(ValueError, 'unverified helper executable'):
            runner.load_index(self.root)

    def test_unverified_nextest_archive_is_rejected(self):
        (self.root / 'nextest.tar.zst').write_bytes(b'unverified')
        with self.assertRaisesRegex(ValueError, 'unverified nextest archive'):
            runner.load_index(self.root)

    def test_missing_package_cannot_be_silently_skipped(self):
        with self.assertRaises(ValueError):
            runner.commands(self.root, ['-p', 'sample', '-p', 'missing', '--tests'])

    def test_missing_target_in_one_package_cannot_be_silently_skipped(self):
        self.index['package_directories']['other'] = 'crates/other'
        with self.assertRaises(ValueError):
            runner.commands(self.root, ['-p', 'sample', '-p', 'other', '--test', 'integration'], self.index)

    def test_wildcard_selects_integration_targets(self):
        command = runner.commands(self.root, ['-p', 'sample', '--test', '*'])[0]
        self.assertEqual(command, [str(self.binary.resolve())])

    def test_unknown_build_options_are_rejected(self):
        with self.assertRaises(ValueError):
            runner.commands(self.root, ["-p", "sample", "--target", "another-architecture"])


if __name__ == "__main__":
    unittest.main()
