import json
import hashlib
import os
import platform
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
NAME = json.loads((ROOT / "isolated-tests.json").read_text())["gtk"][0][2]
SUMMARY = "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"


class IsolatedRunnerTests(unittest.TestCase):
    def test_prebuilt_test_runs_in_its_package_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            subprocess.run(['git', '-C', str(root), '-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                            'commit', '--allow-empty', '-qm', 'fixture'], check=True)
            revision = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
            scripts = root / 'scripts'
            scripts.mkdir()
            for name in ('run-isolated-tests.py', 'run-prebuilt-tests.py', 'ci-test-bundle.py', 'ci-test-bundle-groups.json', 'rust_test_evidence.py'):
                shutil.copy2(ROOT / name, scripts / name)
            (scripts / 'isolated-tests.json').write_text(json.dumps({'gtk': [['sample', 'integration', 'example']]}))
            package = root / 'crates/sample'
            package.mkdir(parents=True)
            bundle = root / 'bundle'
            bundle.mkdir()
            binary = bundle / 'test-program'
            binary.write_text('#!/bin/sh\ntest "$PWD" = "$EXPECTED_CWD" || exit 1\nprintf "test example ... ok\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\\n"\n')
            binary.chmod(0o755)
            index = {'schema_version': 1, 'source_commit': revision, 'architecture': platform.machine(),
                     'tests': [{'package': 'sample', 'kind': 'test', 'name': 'integration', 'path': 'test-program'}],
                     'files': {'test-program': {'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'size': binary.stat().st_size}},
                     'binaries': {}, 'package_directories': {'sample': 'crates/sample'}}
            (bundle / 'index.json').write_text(json.dumps(index))
            result = subprocess.run(['/usr/bin/python3', str(scripts / 'run-isolated-tests.py'), 'gtk'],
                                    env=os.environ | {'BOOKIE_TEST_BUNDLE': str(bundle), 'BOOKIE_SOURCE_COMMIT': revision,
                                                      'EXPECTED_CWD': str(package.resolve())}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def run_fixture(self, output, status=0, diagnostic=""):
        with tempfile.TemporaryDirectory() as directory:
            cargo = Path(directory) / "cargo"
            cargo.write_text(
                "#!/usr/bin/python3\nimport os, sys\n"
                "name = sys.argv[sys.argv.index('--exact') + 1]\n"
                "print(os.environ['FIXTURE_OUTPUT'].replace('__MATCH__', name))\n"
                "print(os.environ['FIXTURE_DIAGNOSTIC'], file=sys.stderr)\n"
                "sys.exit(int(os.environ['FIXTURE_STATUS']))\n"
            )
            cargo.chmod(0o755)
            environment = os.environ.copy()
            for key in ("BOOKIE_REQUIRE_PREBUILT", "BOOKIE_TEST_BUNDLE", "BOOKIE_SOURCE_COMMIT", "BOOKIE_EXPECTED_GROUP"):
                environment.pop(key, None)
            environment.update(
                {
                    "PATH": directory,
                    "FIXTURE_OUTPUT": output,
                    "FIXTURE_STATUS": str(status),
                    "FIXTURE_DIAGNOSTIC": diagnostic,
                }
            )
            return subprocess.run(
                ["/usr/bin/python3", str(ROOT / "run-isolated-tests.py"), "gtk"],
                env=environment,
                capture_output=True, text=True,
            )

    def test_wrong_test_name_cannot_satisfy_a_registered_test(self):
        result = self.run_fixture("test wrong::test ... ok\n" + SUMMARY)
        self.assertNotEqual(result.returncode, 0)

    def test_duplicate_summaries_cannot_satisfy_a_registered_test(self):
        result = self.run_fixture(f"test {NAME} ... ok\n" + SUMMARY + SUMMARY)
        self.assertNotEqual(result.returncode, 0)

    def test_ignored_tests_cannot_satisfy_a_registered_test(self):
        result = self.run_fixture(f"test {NAME} ... ok\n" + SUMMARY.replace("0 ignored", "1 ignored"))
        self.assertNotEqual(result.returncode, 0)

    def test_nonzero_exit_with_valid_output_still_fails(self):
        result = self.run_fixture(f"test {NAME} ... ok\n" + SUMMARY, 101)
        self.assertNotEqual(result.returncode, 0)

    def test_every_registered_name_must_execute(self):
        result = self.run_fixture(f"test {NAME} ... ok\n" + SUMMARY)
        self.assertNotEqual(result.returncode, 0)

    def test_all_registered_tests_with_matching_names_pass(self):
        result = self.run_fixture("test __MATCH__ ... ok\n" + SUMMARY)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_stderr_diagnostics_do_not_change_test_execution_evidence(self):
        result = self.run_fixture("test __MATCH__ ... ok\n" + SUMMARY, diagnostic="test result: FAILED. diagnostic only")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("diagnostic only", result.stderr)

    def test_unbundled_runner_fixtures_ignore_prebuilt_ci_environment(self):
        with patch.dict(
            os.environ,
            {"BOOKIE_REQUIRE_PREBUILT": "1", "BOOKIE_TEST_BUNDLE": "/missing-test-bundle"},
        ):
            result = self.run_fixture("test __MATCH__ ... ok\n" + SUMMARY)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
