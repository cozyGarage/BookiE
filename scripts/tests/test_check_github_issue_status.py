import importlib.util
import json
import subprocess
import unittest
from pathlib import Path
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "check_github_issue_status", ROOT / "scripts/check-github-issue-status.py"
)
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


class GitHubIssueStatusTests(unittest.TestCase):
    def test_fetch_reads_all_issue_pages_and_ignores_pull_requests(self):
        result = subprocess.CompletedProcess(
            args=[],
            returncode=0,
            stdout=json.dumps(
                [[
                    {"number": 1, "state": "open"},
                    {"number": 2, "state": "closed"},
                    {"number": 3, "state": "closed", "pull_request": {"url": "example"}},
                ]]
            ),
            stderr="",
        )
        runner = Mock(return_value=result)

        states, error = checker.fetch_issue_states("cozyGarage/BookiE", runner)

        self.assertIsNone(error)
        self.assertEqual(states, {1: "OPEN", 2: "CLOSED"})
        command = runner.call_args.args[0]
        self.assertIn("--paginate", command)
        self.assertIn("--slurp", command)
        self.assertIn("repos/cozyGarage/BookiE/issues?state=all&per_page=100", command)
        self.assertEqual(runner.call_args.kwargs["timeout"], checker.REQUEST_TIMEOUT_SECONDS)

    def test_closed_and_missing_issues_are_distinguished_from_open(self):
        entries = [("UI-1", 1), ("PEER-18", 2), ("PEER-19", 3)]

        found = checker.stale_entries(entries, {1: "OPEN", 2: "CLOSED"})

        self.assertEqual(
            found,
            [
                "PEER-18 (#2) is closed",
                "PEER-19 (#3) was not returned by GitHub; state is unverified",
            ],
        )

    def test_authentication_failure_is_unavailable_not_closed(self):
        result = subprocess.CompletedProcess(args=[], returncode=1, stdout="", stderr="authentication required")

        states, error = checker.fetch_issue_states("cozyGarage/BookiE", Mock(return_value=result))

        self.assertIsNone(states)
        self.assertEqual(error, "gh api failed: authentication required")

    def test_missing_cli_and_timeout_are_unavailable(self):
        missing = Mock(side_effect=FileNotFoundError)
        states, error = checker.fetch_issue_states("cozyGarage/BookiE", missing)
        self.assertIsNone(states)
        self.assertEqual(error, "gh CLI is not installed")

        timeout = Mock(side_effect=subprocess.TimeoutExpired(cmd="gh api", timeout=30))
        states, error = checker.fetch_issue_states("cozyGarage/BookiE", timeout)
        self.assertIsNone(states)
        self.assertEqual(error, "gh api timed out after 30 seconds")

    def test_invalid_api_json_is_unavailable(self):
        result = subprocess.CompletedProcess(args=[], returncode=0, stdout="not-json", stderr="")

        states, error = checker.fetch_issue_states("cozyGarage/BookiE", Mock(return_value=result))

        self.assertIsNone(states)
        self.assertEqual(error, "gh api returned invalid JSON")

    def test_unexpected_api_shapes_are_unavailable(self):
        for output, expected in (
            (json.dumps({"number": 1, "state": "open"}), "unexpected response shape"),
            (json.dumps([[{"number": 1}]]), "without a valid number and state"),
        ):
            result = subprocess.CompletedProcess(args=[], returncode=0, stdout=output, stderr="")
            states, error = checker.fetch_issue_states("cozyGarage/BookiE", Mock(return_value=result))
            self.assertIsNone(states)
            self.assertIn(expected, error)


if __name__ == "__main__":
    unittest.main()
