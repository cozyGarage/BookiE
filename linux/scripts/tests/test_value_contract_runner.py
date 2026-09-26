import importlib.util
from pathlib import Path
import unittest
import tempfile
from unittest.mock import patch

path = Path(__file__).resolve().parents[1] / "test-value-contracts.py"
spec = importlib.util.spec_from_file_location("value_runner", path)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class ValueRunnerTests(unittest.TestCase):
    def test_all_drivers_have_required_suites(self):
        suites = runner.expected_suites(gtk=True, duckdb=True)
        self.assertEqual(sum(target == "integration" for target in suites.values()), 8)
        self.assertEqual(len(suites), 11)

    def test_build_selection_is_explicit_and_locked(self):
        self.assertIn("--locked", runner.build_command())
        self.assertIn("tablepro-app", runner.build_command())
        self.assertIn("tablepro-driver-duckdb", runner.build_command())
        self.assertNotIn("--exclude", runner.build_command(gtk=True, duckdb=True))

    def test_a_suite_with_no_matching_tests_is_a_failure(self):
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root)
            executable = directory / "empty-suite"
            executable.write_text("#!/bin/sh\nexit 0\n")
            executable.chmod(0o755)
            result = runner.run_suite("crates/core/Cargo.toml", str(executable), directory)
            self.assertEqual(result["tests"], 0)
            self.assertNotEqual(result["exit_code"], 0)

    def test_a_timed_out_suite_is_reported_as_failure(self):
        listed = runner.subprocess.CompletedProcess([], 0, "value_contract_case: test\n", "")
        expired = runner.subprocess.TimeoutExpired("fixture", 300)
        with tempfile.TemporaryDirectory() as root, patch.object(runner.subprocess, "run", side_effect=[listed, expired]):
            result = runner.run_suite("crates/core/Cargo.toml", "/tmp/fixture", Path(root))
            self.assertEqual(result["tests"], 1)
            self.assertEqual(result["exit_code"], 124)

    def test_only_expected_test_executables_are_selected(self):
        message = {"reason": "compiler-artifact", "executable": "/tmp/test", "manifest_path": str(runner.ROOT / "crates/core/Cargo.toml"), "target": {"name": "tablepro_core"}, "profile": {"test": True}}
        self.assertEqual(runner.select_artifact(message, runner.expected_suites()), ("crates/core/Cargo.toml", "/tmp/test"))
        message["profile"]["test"] = False
        self.assertIsNone(runner.select_artifact(message, runner.expected_suites()))


if __name__ == "__main__":
    unittest.main()
