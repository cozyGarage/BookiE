import importlib.util
import json
from datetime import date
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("advisory_review", Path(__file__).resolve().parents[1] / "check-advisory-review.py")
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class AdvisoryReviewTests(unittest.TestCase):
    def test_future_and_current_review_dates_pass(self):
        for review in ("2026-11-22", "2026-12-01"):
            self.assertEqual(check.problems([{"id": "RUSTSEC-X", "reason": f"needed; review-by: {review}"}], date(2026, 11, 22)), [])

    def test_expired_missing_malformed_and_unreasoned_entries_fail(self):
        for entry in (
            {"id": "RUSTSEC-X", "reason": "needed; review-by: 2026-11-21"},
            {"id": "RUSTSEC-X", "reason": "needed"},
            {"id": "RUSTSEC-X", "reason": "needed; review-by: 2026-02-30"},
            {"id": "RUSTSEC-X", "reason": "review-by: 2026-12-01"},
            "RUSTSEC-X",
        ):
            with self.subTest(entry=entry):
                self.assertTrue(check.problems([entry], date(2026, 11, 22)))

    def test_duplicate_or_malformed_review_markers_fail(self):
        for reason in ("needed; review-by: 2026-12-01; review-by: 2026-12-02", "needed; review-by: 2026-12-01-extra"):
            self.assertTrue(check.problems([{"id": "RUSTSEC-X", "reason": reason}], date(2026, 11, 22)))

    def test_supply_chain_layer_runs_the_review_check(self):
        root = Path(__file__).resolve().parents[2]
        layers = json.loads((root / "scripts/test-layers.json").read_text())
        commands = [step["argv"] for step in layers["layers"]["supply-chain"]["steps"]]
        self.assertIn(["python3", "scripts/check-advisory-review.py"], commands)
