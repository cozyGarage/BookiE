import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("check_driver_id_branches", ROOT / "scripts/check-driver-id-branches.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class DriverIdBranchTests(unittest.TestCase):
    def counted(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "x.rs"
            path.write_text(source)
            return checker.count(path)

    def test_a_comparison_with_an_engine_id_counts(self):
        self.assertEqual(self.counted('if self.driver_id == "redis" {}\nlet ok = matches!(self.driver_id, "a" | "mongodb");'), 2)

    def test_a_struct_field_initialiser_does_not_count(self):
        self.assertEqual(self.counted('Row {\n    driver_id: "postgres".into(),\n}'), 0)

    def test_test_code_after_the_cfg_marker_does_not_count(self):
        self.assertEqual(self.counted('#[cfg(test)]\nmod t { fn f() { assert!(driver_id == "sqlite"); } }'), 0)

    def test_the_committed_baseline_matches_the_tree(self):
        current = checker.counts()
        baseline = checker.read_baseline()
        self.assertTrue(all(number <= baseline.get(name, 0) for name, number in current.items()))


if __name__ == "__main__":
    unittest.main()
