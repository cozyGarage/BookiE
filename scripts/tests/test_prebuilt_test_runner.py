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
        self.enterContext(patch.dict(os.environ, {"GITHUB_SHA": "a" * 40, "BOOKIE_SOURCE_COMMIT": "a" * 40}))
        self.enterContext(patch.object(runner.subprocess, 'check_output', return_value='a' * 40))
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

    def test_invocation_requires_at_least_one_executed_test(self):
        self.index['package_directories']['sample'] = '.'
        (self.root / 'index.json').write_text(json.dumps(self.index))
        command = [str(self.binary)]
        empty = subprocess.CompletedProcess(command, 0, 'test result: ok. 0 passed; 0 failed;\n')
        passed = subprocess.CompletedProcess(command, 0, 'test result: ok. 1 passed; 0 failed;\n')
        with patch.object(runner, 'ROOT', self.root), patch.object(runner, 'commands', return_value=[command, command]), \
             patch.object(runner.subprocess, 'run', side_effect=[empty, passed]), \
             patch.dict(os.environ, {'BOOKIE_TEST_BUNDLE': str(self.root)}):
            self.assertEqual(runner.main(), 0)
        with patch.object(runner, 'ROOT', self.root), patch.object(runner, 'commands', return_value=[command]), \
             patch.object(runner.subprocess, 'run', return_value=empty), \
             patch.dict(os.environ, {'BOOKIE_TEST_BUNDLE': str(self.root)}), \
             self.assertRaisesRegex(ValueError, 'no prebuilt tests executed'):
            runner.main()

    def test_invocation_matches_executable_paths_through_symlinked_temp_directories(self):
        self.index['package_directories']['sample'] = '.'
        (self.root / 'index.json').write_text(json.dumps(self.index))
        alias = self.root.with_name(f'{self.root.name}-alias')
        alias.symlink_to(self.root, target_is_directory=True)
        command = [str(alias / self.binary.name)]
        result = subprocess.CompletedProcess(command, 0, 'test result: ok. 1 passed; 0 failed;\n')
        with patch.object(runner, 'ROOT', self.root), patch.object(runner, 'commands', return_value=[command]), \
             patch.object(runner.subprocess, 'run', return_value=result), \
             patch.dict(os.environ, {'BOOKIE_TEST_BUNDLE': str(self.root)}):
            self.assertEqual(runner.main(), 0)

    def test_nested_libtest_output_keeps_the_outer_test_run_valid(self):
        self.index['package_directories']['sample'] = '.'
        (self.root / 'index.json').write_text(json.dumps(self.index))
        command = [str(self.binary)]
        output = (
            'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 86 filtered out\n'
            'test result: ok. 86 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n'
        )
        with patch.object(runner, 'ROOT', self.root), patch.object(runner, 'commands', return_value=[command]), \
             patch.object(runner.subprocess, 'run', return_value=subprocess.CompletedProcess(command, 0, output)), \
             patch.dict(os.environ, {'BOOKIE_TEST_BUNDLE': str(self.root)}):
            self.assertEqual(runner.main(), 0)

    def test_failed_process_and_missing_summary_fail_the_invocation(self):
        self.index['package_directories']['sample'] = '.'
        (self.root / 'index.json').write_text(json.dumps(self.index))
        command = [str(self.binary)]
        with patch.object(runner, 'ROOT', self.root), patch.object(runner, 'commands', return_value=[command]), \
             patch.dict(os.environ, {'BOOKIE_TEST_BUNDLE': str(self.root)}):
            with patch.object(runner.subprocess, 'run', return_value=subprocess.CompletedProcess(command, 101, '')):
                self.assertEqual(runner.main(), 101)
            with patch.object(runner.subprocess, 'run', return_value=subprocess.CompletedProcess(command, 0, '')), \
                 self.assertRaisesRegex(ValueError, 'execution evidence missing'):
                runner.main()

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

    def test_archive_only_bundle_cannot_be_used_for_direct_test_execution(self):
        self.index['archive_only'] = True
        self.index['files']['nextest.tar.zst'] = {'sha256': hashlib.sha256(b'archive').hexdigest(), 'size': 7}
        (self.root / 'nextest.tar.zst').write_bytes(b'archive')
        (self.root / 'index.json').write_text(json.dumps(self.index))
        with self.assertRaisesRegex(ValueError, 'not direct test executables'):
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
