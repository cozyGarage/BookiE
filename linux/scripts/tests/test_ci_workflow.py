from pathlib import Path
import importlib.util
import unittest

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("ci_jobs", ROOT / "linux/scripts/check-ci-jobs.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class CiWorkflowTests(unittest.TestCase):
    def test_jobs_share_an_immutable_checkout(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        self.assertNotIn("continue-on-error:", workflow)
        self.assertIn("commit: ${{ steps.revision.outputs.commit }}", workflow)
        self.assertIn("ref: ${{ needs.preflight.outputs.commit }}", workflow)
        for line in workflow.splitlines():
            if line.strip().startswith("ref:"):
                self.assertNotIn("github.ref", line)
        quality = (ROOT / ".github/workflows/linux-quality.yml").read_text()
        self.assertEqual(quality.count("ref: ${{ needs.resolve-ref.outputs.commit }}"), 2)

    def test_required_jobs_never_accept_failure_skip_cancel_or_missing(self):
        success = {name: {"result": "success"} for name in checker.REQUIRED | {checker.SCHEDULED}}
        self.assertEqual(checker.assess(success, "push")[0], [])
        for name in checker.REQUIRED:
            for state in ["failure", "skipped", "cancelled", "missing"]:
                results = success | {name: {"result": state}}
                self.assertEqual(checker.assess(results, "push")[0], [name])
        self.assertEqual(set(checker.assess({}, "push")[0]), checker.REQUIRED | {checker.SCHEDULED})

    def test_only_push_and_pr_can_skip_current_stable_clippy(self):
        results = {name: {"result": "success"} for name in checker.REQUIRED}
        results[checker.SCHEDULED] = {"result": "skipped"}
        for event in ["push", "pull_request"]:
            self.assertEqual(checker.assess(results, event)[0], [])
        for event in ["schedule", "workflow_dispatch"]:
            self.assertEqual(checker.assess(results, event)[0], [checker.SCHEDULED])

    def test_summary_depends_on_every_required_job_and_always_runs(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        section = workflow.split("  regression-gate:\n", 1)[1]
        self.assertIn("if: always()", section)
        self.assertIn("CI_JOB_RESULTS: ${{ toJSON(needs) }}", section)
        self.assertIn("python3 linux/scripts/check-ci-jobs.py", section)
        for name in checker.REQUIRED | {checker.SCHEDULED}:
            self.assertIn(f"      - {name}\n", section)

    def test_docker_ssh_targets_run_in_hosted_and_local_integration(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        hosted = workflow.split("  integration:\n", 1)[1].split("  driver-tls:\n", 1)[0]
        local = (ROOT / "linux/scripts/ci-local.sh").read_text().split("run_integration() {", 1)[1].split("run_release()", 1)[0]
        self.assertIn("run-test-layer.py drivers", hosted)
        self.assertIn("scripts/test-ssh.sh", local)
        script = (ROOT / "linux/scripts/test-ssh.sh").read_text()
        for required in ["set -euo pipefail", "--test agent_auth", "--test openssh_session", "--include-ignored", "--test-threads=1"]:
            self.assertIn(required, script)


if __name__ == "__main__":
    unittest.main()
