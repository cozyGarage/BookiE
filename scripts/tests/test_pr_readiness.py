import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("pr_readiness", ROOT / "scripts/check-pr-readiness.py")
readiness = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(readiness)


class PullRequestReadinessTests(unittest.TestCase):
    def test_required_contexts_still_match_the_workflow_job_names(self):
        sources = {
            "pr-title": ("pr-title.yml", "  pr-title:"),
            "release-note": ("release-note-check.yml", "  release-note:"),
            "Pin tested revision": ("build-linux.yml", "name: Pin tested revision"),
            "Ensure CI debian-testing GTK image": ("build-linux.yml", "name: Ensure CI debian-testing GTK image"),
            "Preflight and sandbox tier (no GTK app)": ("build-linux.yml", "name: Preflight and sandbox tier (no GTK app)"),
            "Fast checks (GTK)": ("build-linux.yml", "name: Fast checks (GTK)"),
            "flatpak-builder (default)": ("flatpak-linux.yml", "name: flatpak-builder (${{ matrix.profile }})"),
            "resolve-ref": ("linux-security.yml", "  resolve-ref:"),
            "Security (security-policy)": ("linux-security.yml", "name: Security (${{ matrix.layer }})"),
            "Security (supply-chain)": ("linux-security.yml", "layer: [security-policy, supply-chain]"),
            "workflow-lint": ("linux-ci-contracts.yml", "  workflow-lint:"),
            "workflow-security": ("linux-ci-contracts.yml", "  workflow-security:"),
            "harness": ("linux-ci-contracts.yml", "  harness:"),
        }

        for context, (filename, marker) in sources.items():
            with self.subTest(context=context):
                workflow = (ROOT / ".github/workflows" / filename).read_text()
                self.assertIn(marker, workflow)

    def test_code_changes_require_build_security_and_flatpak_contexts(self):
        required = readiness.required_contexts(["crates/core/src/query.rs"])

        self.assertTrue(
            {
                "pr-title",
                "release-note",
                "Pin tested revision",
                "Ensure CI debian-testing GTK image",
                "Preflight and sandbox tier (no GTK app)",
                "Fast checks (GTK)",
                "flatpak-builder (default)",
                "resolve-ref",
                "Security (security-policy)",
                "Security (supply-chain)",
            }.issubset(required)
        )
        self.assertNotIn("harness", required)

    def test_docs_only_changes_require_contract_checks_without_build_checks(self):
        required = readiness.required_contexts(["docs/testing.md", "README.md"])

        self.assertEqual(
            set(required),
            {"pr-title", "release-note", "harness", "workflow-lint", "workflow-security"},
        )

    def test_mixed_changes_require_code_and_contract_checks(self):
        required = readiness.required_contexts(["docs/testing.md", "scripts/check-pr-title.py"])

        self.assertIn("Fast checks (GTK)", required)
        self.assertIn("harness", required)
        self.assertIn("workflow-lint", required)

    def test_a_check_suite_must_match_the_pull_request_and_exact_head(self):
        suite = {
            "head_sha": "a" * 40,
            "app": {"slug": "github-actions"},
            "pull_requests": [{"number": 7}],
        }

        self.assertTrue(readiness.check_suite_matches(suite, 7, "a" * 40))
        self.assertFalse(readiness.check_suite_matches(suite, 8, "a" * 40))
        self.assertFalse(readiness.check_suite_matches(suite, 7, "b" * 40))

    def test_checks_from_other_apps_do_not_satisfy_the_gate(self):
        suite = {
            "head_sha": "a" * 40,
            "app": {"slug": "sonarqubecloud"},
            "pull_requests": [{"number": 7}],
        }

        self.assertFalse(readiness.check_suite_matches(suite, 7, "a" * 40))

    def test_missing_checks_remain_pending(self):
        result = readiness.evaluate_checks(["pr-title"], [])

        self.assertEqual(result["state"], "waiting")
        self.assertEqual(result["waiting"], ["pr-title: missing"])

    def test_a_pending_latest_attempt_does_not_inherit_an_older_success(self):
        runs = [
            {"name": "pr-title", "status": "completed", "conclusion": "success", "started_at": "2026-01-01T00:00:00Z", "id": 1},
            {"name": "pr-title", "status": "in_progress", "conclusion": None, "started_at": "2026-01-01T00:01:00Z", "id": 2},
        ]

        result = readiness.evaluate_checks(["pr-title"], runs)

        self.assertEqual(result["state"], "waiting")
        self.assertEqual(result["waiting"], ["pr-title: in_progress"])

    def test_a_newer_successful_attempt_replaces_an_older_failure(self):
        runs = [
            {"name": "pr-title", "status": "completed", "conclusion": "failure", "started_at": "2026-01-01T00:00:00Z", "id": 1},
            {"name": "pr-title", "status": "completed", "conclusion": "success", "started_at": "2026-01-01T00:01:00Z", "id": 2},
        ]

        result = readiness.evaluate_checks(["pr-title"], runs)

        self.assertEqual(result["state"], "passed")

    def test_a_failed_or_skipped_applicable_check_fails_closed(self):
        for conclusion in ("failure", "cancelled", "skipped", "timed_out", "neutral"):
            with self.subTest(conclusion=conclusion):
                result = readiness.evaluate_checks(
                    ["pr-title"],
                    [{"name": "pr-title", "status": "completed", "conclusion": conclusion, "started_at": "2026-01-01T00:00:00Z", "id": 1}],
                )

                self.assertEqual(result["state"], "failed")

    def test_all_required_checks_must_succeed(self):
        runs = [
            {"name": name, "status": "completed", "conclusion": "success", "started_at": "2026-01-01T00:00:00Z", "id": index}
            for index, name in enumerate(("pr-title", "release-note"), start=1)
        ]

        result = readiness.evaluate_checks(["pr-title", "release-note"], runs)

        self.assertEqual(result["state"], "passed")
        self.assertEqual(result["passed"], ["pr-title", "release-note"])

    def test_a_transient_github_api_error_retries_before_reporting_readiness(self):
        class Client:
            attempts = 0

            def pull_request_paths(self, _pull_number):
                return ["README.md"]

            def check_runs_for_pull_request(self, _pull_number, _head_sha):
                self.attempts += 1
                if self.attempts == 1:
                    raise readiness.TransientGitHubError("temporary")
                names = readiness.required_contexts(["README.md"])
                return [
                    {"name": name, "status": "completed", "conclusion": "success", "started_at": "2026-01-01T00:00:00Z", "id": index}
                    for index, name in enumerate(names, start=1)
                ]

        sleeps = []
        result = readiness.wait_for_checks(
            Client(),
            7,
            "a" * 40,
            timeout_seconds=10,
            poll_seconds=1,
            sleep=sleeps.append,
            monotonic=lambda: 0,
        )

        self.assertEqual(result, 0)
        self.assertEqual(sleeps, [1])

    def test_readiness_workflow_checks_exact_head_with_read_only_permissions(self):
        workflow = (ROOT / ".github/workflows/linux-pr-readiness.yml").read_text()
        checkout = workflow.split("- uses: actions/checkout", 1)[1].split("\n      - name", 1)[0]

        self.assertIn("pull_request:", workflow)
        self.assertNotIn("pull_request_target:", workflow)
        self.assertIn("ref: ${{ github.event.pull_request.head.sha }}", checkout)
        self.assertIn("  checks: read", workflow)
        self.assertIn("  contents: read", workflow)
        self.assertIn("  pull-requests: read", workflow)
        self.assertNotIn("contents: write", workflow)


if __name__ == "__main__":
    unittest.main()
