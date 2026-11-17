import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("paginator_names", ROOT / "apps/linux/app/tests/paginator_names.py")
paginator_names = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(paginator_names)
matches = paginator_names.paginator_name_matches


class PaginatorNameTests(unittest.TestCase):
    def test_the_bare_range_and_the_range_with_a_load_time_both_match(self):
        self.assertTrue(matches("Rows 1 – 3 of 3", "Rows 1 – 3 of 3"))
        self.assertTrue(matches("Rows 1 – 3 of 3 · 5 ms", "Rows 1 – 3 of 3"))
        self.assertTrue(matches("Rows 1 – 100 of 150 · 1.25 s", "Rows 1 – 100 of 150"))
        self.assertTrue(matches("Rows 1 – 3 of 3 · 2m 4s", "Rows 1 – 3 of 3"))
        self.assertTrue(matches("No rows on this page · <1 ms", "No rows on this page"))

    def test_a_different_range_or_state_does_not_match(self):
        self.assertFalse(matches("Rows 1 – 100 of 1500", "Rows 1 – 100 of 150"))
        self.assertFalse(matches("Rows 1 – 100 of 1500 · 5 ms", "Rows 1 – 100 of 150"))
        self.assertFalse(matches("Rows 101 – 150 of 150", "Rows 1 – 150 of 150"))
        self.assertFalse(matches("No rows on this page (result truncated)", "No rows on this page"))
        self.assertFalse(matches("No rows on this page (result truncated) · 5 ms", "No rows on this page"))
        self.assertFalse(matches("", "Rows 1 – 3 of 3"))
        self.assertFalse(matches("Rows 1 – 3 of 3 extra", "Rows 1 – 3 of 3"))

    def test_only_the_load_time_separator_extends_a_match(self):
        self.assertFalse(matches("Rows 1 – 3 of 3, 5 ms", "Rows 1 – 3 of 3"))
        self.assertFalse(matches("Rows 1 – 3 of 3·5 ms", "Rows 1 – 3 of 3"))


if __name__ == "__main__":
    unittest.main()
