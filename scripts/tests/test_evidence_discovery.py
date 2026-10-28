import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("evidence_discovery_validator", Path(__file__).parents[1] / "validate-evidence.py")
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)


class EvidenceDiscoveryTests(unittest.TestCase):
    def scan(self, files, strict=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            evidence = root / "docs/evidence"
            evidence.mkdir(parents=True)
            for name, value in files.items():
                path = evidence / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(value if isinstance(value, str) else json.dumps(value))
            output = io.StringIO()
            args = ["validate-evidence", "--strict"] if strict else ["validate-evidence"]
            with (mock.patch.object(VALIDATOR, "ROOT", root),
                  mock.patch.object(VALIDATOR, "EVIDENCE", evidence),
                  mock.patch.object(sys, "argv", args),
                  contextlib.redirect_stdout(output), contextlib.redirect_stderr(output)):
                status = VALIDATOR.main()
            return status, output.getvalue()

    def test_bad_legacy_source_digest_under_validation_name_fails_the_default_scan(self):
        status, output = self.scan({"case/validation.json": {"source_sha256": {"source.rs": "invalid"}}})
        self.assertEqual(status, 1)
        self.assertIn("source.rs", output)

    def test_bad_source_record_under_an_arbitrary_name_warns_or_fails_strictly(self):
        status, output = self.scan({"case/result.json": {"schema_version": 1, "source_sha256": []}})
        self.assertEqual(status, 0)
        self.assertIn("result.json", output)
        status, _ = self.scan({"case/result.json": {"schema_version": 1, "source_sha256": []}}, strict=True)
        self.assertEqual(status, 1)

    def test_invalid_run_report_under_an_arbitrary_name_fails_the_default_scan(self):
        status, _ = self.scan({"case/run.json": {"schema_version": 1, "commit": "invalid",
            "working_tree": "", "tracked_diff_sha256": "a" * 64, "status": "passed", "layers": {}}})
        self.assertEqual(status, 1)

    def test_missing_required_run_report_field_is_not_treated_as_generic_evidence(self):
        for field in ("commit", "working_tree", "tracked_diff_sha256", "status", "layers"):
            with self.subTest(field=field):
                report = {"schema_version": 1, "commit": "a" * 40, "working_tree": "",
                          "tracked_diff_sha256": "b" * 64, "status": "passed", "layers": {}}
                report.pop(field)
                status, _ = self.scan({"case/run.json": report})
                self.assertEqual(status, 1)

    def test_malformed_named_evidence_is_reported(self):
        self.assertEqual(self.scan({"case/validation.json": "{"})[0], 1)
        self.assertEqual(self.scan({"case/manifest.json": "{"})[0], 1)

    def test_unrelated_json_and_unsupported_versions_do_not_become_evidence(self):
        files = {"case/validation.json": [1, 2], "case/other.json": {"schema_version": 1, "artifacts": []},
                 "case/future.json": {"schema_version": 2, "source_sha256": []},
                 "case/old.json": {"note": "not an evidence record"}}
        status, output = self.scan(files, strict=True)
        self.assertEqual(status, 0)
        self.assertIn("Validated 0 schema-v1", output)
        self.assertIn("0 legacy packets", output)

    def test_valid_execution_report_is_counted_without_filename_requirements(self):
        status, output = self.scan({"case/run.json": {"schema_version": 1, "commit": "a" * 40,
            "working_tree": "", "tracked_diff_sha256": "b" * 64, "status": "passed", "layers": {}}})
        self.assertEqual(status, 0)
        self.assertIn("Validated 1 schema-v1", output)

    def test_provenance_review_does_not_certify_an_unrecorded_run_snapshot(self):
        record = {"source_sha256": {}, "source_commits": {},
                  "provenance_review": {"status": "run_snapshot_unrecorded"}}
        status, output = self.scan({"case/validation.json": record}, strict=True)
        self.assertEqual(status, 1)
        self.assertIn("validation.json", output)


if __name__ == "__main__":
    unittest.main()
