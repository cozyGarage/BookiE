from pathlib import Path
import importlib.util
import json
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("ci_jobs", ROOT / "linux/scripts/check-ci-jobs.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
bounded_spec = importlib.util.spec_from_file_location("bounded_operations", ROOT / "linux/scripts/check-bounded-operations.py")
bounded_checker = importlib.util.module_from_spec(bounded_spec)
bounded_spec.loader.exec_module(bounded_checker)


class CiWorkflowTests(unittest.TestCase):
    def test_bounded_operation_guard_catches_multiline_calls(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "fixture.rs"
            source.write_text("connection\n    .query(\"SELECT 1\");\nconn.fetch_columns(None, \"x\");\nconnection.execute_controlled(\"ok\", &control);\n")
            found = list(bounded_checker.unbounded_calls([source]))
        self.assertEqual([row[2] for row in found], ["connection .query(", "conn.fetch_columns("])

    def test_cross_consumer_server_targets_are_gated_and_documented(self):
        local = (ROOT / "linux/scripts/ci-local.sh").read_text()
        for package, target in [("tablepro-mcp", "mongodb_extended_json"), ("tablepro-policy", "session_postgres")]:
            self.assertIn(f"-p {package} --test {target} -- --include-ignored --test-threads=1", local)
        registry = json.loads((ROOT / "linux/scripts/isolated-tests.json").read_text())
        self.assertEqual(
            {entry[2].split("::")[-1] for entry in registry["app-server"]},
            {
                "postgres_numeric_parser_outputs_round_trip_through_server",
                "postgres_extended_temporal_grid_edits_preserve_native_values",
                "value_contract_mongodb_int32_grid_edit_preserves_integer_width",
                "value_contract_mongodb_date_grid_edit_preserves_millisecond_instant",
                "value_contract_mongodb_decimal128_grid_edit_preserves_wide_precision",
                "value_contract_mongodb_nested_document_edit_preserves_extended_bson_and_row_identity",
                "value_contract_mongodb_off_page_type_change_during_census_refuses_edit",
                "value_contract_mongodb_browse_uses_one_find_for_schema_and_page",
                "value_contract_mongodb_census_is_not_a_snapshot_for_already_read_documents",
                "value_contract_mongodb_run_find_merges_page_types_and_exports_materialized_values",
                "value_contract_mysql_unsigned_integer_grid_edits_refuse_coercion_and_preserve_u64",
                "value_contract_mysql_spatial_grid_refusal_preserves_native_bytes",
                "value_contract_mysql_bit_parser_edits_preserve_native_values",
                "value_contract_mongodb_collection_wide_mixed_metadata_refuses_edit",
            },
        )
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        fast = workflow.split("  fast:\n", 1)[1].split("  gtk-safety:\n", 1)[0]
        for required in ["run-test-layer.py app-server", "/var/run/docker.sock:/var/run/docker.sock"]:
            self.assertIn(required, fast)
        self.assertIn(
            "cargo test --locked -p tablepro-app --features duckdb --lib value_contract_duckdb",
            workflow,
        )
        ledger = subprocess.check_output(["python3", str(ROOT / "linux/scripts/inventory-ignored-tests.py")], text=True)
        for package, target in [("tablepro-mcp", "mongodb_extended_json"), ("tablepro-policy", "session_postgres")]:
            self.assertIn(f"-p {package} --test {target}", ledger)
        self.assertIn("scripts/run-test-layer.py app-server", ledger)
        self.assertNotIn("tablepro-driver-tests", ledger)
        self.assertNotIn("tablepro-driver-src", ledger)

    def test_full_gate_enforces_function_size_like_preflight(self):
        script = (ROOT / "linux/scripts/ci-local.sh").read_text()
        full = script.split("run_full() {", 1)[1].split("run_integration()", 1)[0]
        self.assertIn("python3 scripts/check-function-size.py", full)

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
