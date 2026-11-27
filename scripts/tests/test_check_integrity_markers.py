import contextlib
import importlib.util
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("check_integrity_markers", ROOT / "scripts/check-integrity-markers.py")
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)

CORPUS = """
engine = "mysql"
servers = ["a"]

[[known_failing]]
cases = ["one"]
route = "param_select"
issue = 11
expect = "x"
reason = "r"

[[case]]
id = "one"
known_failing = [{ route = "browse", issue = 12, expect = "y", reason = "r" }]
"""


def states(mapping):
    def run(command, **_):
        number = int(command[3])
        return subprocess.CompletedProcess(command, 0, f'{{"state": "{mapping[number]}"}}', "")

    return Mock(side_effect=run)


class IntegrityMarkerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        Path(self.directory.name, "mysql.toml").write_text(CORPUS, encoding="utf-8")

    def tearDown(self):
        self.directory.cleanup()

    def run_check(self, runner):
        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            status = checker.main([], runner, self.directory.name)
        return status, stdout.getvalue() + stderr.getvalue()

    def test_collects_corpus_and_case_markers(self):
        issues = checker.marker_issues(self.directory.name)
        self.assertEqual(sorted(issues), [11, 12])
        self.assertEqual(issues[12], ["mysql.toml:one"])

    def test_open_issues_pass(self):
        status, output = self.run_check(states({11: "OPEN", 12: "OPEN"}))
        self.assertEqual(status, 0, output)

    def test_a_closed_issue_fails_and_names_its_marker(self):
        status, output = self.run_check(states({11: "OPEN", 12: "CLOSED"}))
        self.assertEqual(status, 1)
        self.assertIn("#12 is closed but still marks mysql.toml:one", output)

    def test_an_unavailable_state_is_not_a_pass(self):
        failing = Mock(return_value=subprocess.CompletedProcess([], 1, "", "auth required"))
        status, output = self.run_check(failing)
        self.assertEqual(status, 2)
        self.assertIn("unavailable", output)

    def test_the_real_corpus_parses(self):
        self.assertTrue(checker.marker_issues(checker.CORPUS_DIR))


if __name__ == "__main__":
    unittest.main()
