import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("check_forgejo_ci_jobs", ROOT / "scripts/check-forgejo-ci-jobs.py")
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class CheckForgejoCiJobsTests(unittest.TestCase):
    def test_missing_required_jobs_fail_closed(self):
        self.assertEqual(
            CHECKER.failures({"guards": {"result": "success"}}, "refs/heads/ci/test", "push"),
            {
                name: "missing"
                for name in (CHECKER.REQUIRED | CHECKER.OPTIONAL_ON_NON_MAIN) - {"guards"}
            },
        )

    def test_unexpected_job_results_fail_the_final_gate(self):
        results = {name: {"result": "success"} for name in CHECKER.REQUIRED}
        results["unreviewed-job"] = {"result": "success"}

        self.assertEqual(
            CHECKER.failures(results, "refs/heads/main", "push"),
            {"unreviewed-job": "unexpected", "floor-build": "missing", "distro-floor-scenarios": "missing"},
        )

    def test_non_main_push_allows_only_the_main_only_floor_jobs_to_skip(self):
        results = {name: {"result": "success"} for name in CHECKER.REQUIRED}
        results.update({
            "floor-build": {"result": "skipped"},
            "distro-floor-scenarios": {"result": "skipped"},
        })

        self.assertEqual(CHECKER.failures(results, "refs/heads/ci/test", "push"), {})

    def test_pull_request_allows_the_main_only_floor_jobs_to_skip(self):
        results = {name: {"result": "success"} for name in CHECKER.REQUIRED}
        results.update({
            "floor-build": {"result": "skipped"},
            "distro-floor-scenarios": {"result": "skipped"},
        })

        self.assertEqual(CHECKER.failures(results, "refs/pull/1/merge", "pull_request"), {})

    def test_manual_and_scheduled_runs_require_the_floor_jobs(self):
        results = {name: {"result": "success"} for name in CHECKER.REQUIRED}
        results.update({
            "floor-build": {"result": "skipped"},
            "distro-floor-scenarios": {"result": "skipped"},
        })

        for event in ("schedule", "workflow_dispatch"):
            with self.subTest(event=event):
                self.assertEqual(
                    CHECKER.failures(results, "refs/heads/main", event),
                    {"floor-build": "skipped", "distro-floor-scenarios": "skipped"},
                )

    def test_a_failed_job_and_an_unexpected_skip_fail_the_final_gate(self):
        results = {name: {"result": "success"} for name in CHECKER.REQUIRED | CHECKER.OPTIONAL_ON_NON_MAIN}
        results["guards"] = {"result": "failure"}
        results["clippy"] = {"result": "skipped"}

        self.assertEqual(
            CHECKER.failures(results, "refs/heads/main", "push"),
            {"guards": "failure", "clippy": "skipped"},
        )


if __name__ == "__main__":
    unittest.main()
