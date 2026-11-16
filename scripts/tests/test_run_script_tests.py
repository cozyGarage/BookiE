import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "run-script-tests.py"
SPEC = importlib.util.spec_from_file_location("run_script_tests", SCRIPT)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)

PASSING = "import unittest\nclass T(unittest.TestCase):\n    def test_ok(self):\n        self.assertTrue(True)\n"
FAILING = "import unittest\nclass T(unittest.TestCase):\n    def test_bad(self):\n        self.fail('boom marker')\n"
EMPTY = "VALUE = 1\n"


class RunScriptTestsTests(unittest.TestCase):
    def run_directory(self, files, jobs=2):
        with tempfile.TemporaryDirectory() as directory:
            for name, text in files.items():
                (Path(directory) / name).write_text(text)
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = RUNNER.main(["--directory", directory, "--jobs", str(jobs)])
            return code, out.getvalue(), err.getvalue()

    def test_passing_files_succeed_and_are_counted(self):
        code, out, _ = self.run_directory({"test_a.py": PASSING, "test_b.py": PASSING})
        self.assertEqual(code, 0)
        self.assertIn("2 files, 0 failed", out)

    def test_a_failing_file_fails_the_run_and_shows_its_own_output(self):
        code, out, err = self.run_directory({"test_a.py": PASSING, "test_bad.py": FAILING})
        self.assertEqual(code, 1)
        self.assertIn("2 files, 1 failed", out)
        self.assertIn("FAILED test_bad.py (twice", err)
        self.assertIn("boom marker", err)
        self.assertNotIn("FAILED test_a.py", err)

    def test_a_file_that_fails_only_while_running_in_parallel_is_retried_alone_and_reported(self):
        with tempfile.TemporaryDirectory() as scratch:
            marker = Path(scratch) / "seen"
            flaky = (
                "import unittest\nfrom pathlib import Path\nclass T(unittest.TestCase):\n"
                "    def test_once(self):\n"
                f"        marker = Path({str(marker)!r})\n"
                "        first = not marker.exists()\n"
                "        marker.write_text('x')\n"
                "        self.assertFalse(first, 'fails the first time only')\n"
            )
            code, out, err = self.run_directory({"test_flaky.py": flaky, "test_ok.py": PASSING})
        self.assertEqual(code, 0)
        self.assertIn("2 files, 0 failed, 1 retried", out)
        self.assertIn("test_flaky.py", err)

    def test_a_file_that_runs_no_unittest_cases_is_not_a_failure(self):
        code, out, _ = self.run_directory({"test_script.py": EMPTY, "test_a.py": PASSING})
        self.assertEqual(code, 0)
        self.assertIn("0 failed", out)

    def test_an_empty_directory_is_an_error_and_not_a_silent_pass(self):
        code, _, err = self.run_directory({})
        self.assertEqual(code, 1)
        self.assertIn("no test files", err)

    def test_the_job_count_can_be_set_from_the_environment(self):
        with patch.dict(RUNNER.os.environ, {"BOOKIE_SCRIPT_TEST_JOBS": "3"}):
            self.assertEqual(RUNNER.default_jobs(), 3)
        with patch.dict(RUNNER.os.environ, {"BOOKIE_SCRIPT_TEST_JOBS": "0"}):
            self.assertEqual(RUNNER.default_jobs(), 1)


if __name__ == "__main__":
    unittest.main()
