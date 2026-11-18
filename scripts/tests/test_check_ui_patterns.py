import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("check_ui_patterns", ROOT / "scripts/check-ui-patterns.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class UiPatternTests(unittest.TestCase):
    def counted(self, source):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "x.rs"
            path.write_text(source)
            return checker.count(path)

    def test_a_spacing_constant_and_zero_do_not_count(self):
        self.assertEqual(self.counted(".margin_top(spacing::MD)\n.spacing(0)"), {})

    def test_a_spacing_literal_counts_in_every_form(self):
        source = ".margin_top(10)\n.spacing(6)\nset_margin_end: 8,\nBox::new(gtk::Orientation::Vertical, 4)"
        self.assertEqual(self.counted(source), {"literal-spacing": 4})

    def test_thread_local_and_toolbar_count(self):
        self.assertEqual(
            self.counted("thread_local! { }\nlet t = adw::ToolbarView::new();"),
            {"thread_local": 1, "hand-built-toolbar": 1},
        )

    def test_test_code_after_the_cfg_marker_does_not_count(self):
        self.assertEqual(self.counted("#[cfg(test)]\nmod t { fn f() { thread_local! {} } }"), {})

    def test_the_committed_baseline_matches_the_tree(self):
        current = checker.counts()
        baseline = checker.read_baseline()
        self.assertTrue(all(number <= baseline.get(key, 0) for key, number in current.items()))
        self.assertTrue(all(key in current for key in baseline), "lower the baseline for removed patterns")


if __name__ == "__main__":
    unittest.main()
