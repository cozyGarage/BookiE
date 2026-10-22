import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "apps/linux/app/tests"))

import keyboard_paths


class KeyboardPathRegistryTests(unittest.TestCase):
    def test_an_unlisted_clickable_is_reported(self):
        names = ["Run", "Open orders", "Brand new button"]
        self.assertEqual(keyboard_paths.unregistered(names, keyboard_paths.SQL_EDITOR), ["Brand new button"])

    def test_every_entry_names_its_kind_of_path(self):
        self.assertEqual(keyboard_paths.malformed(keyboard_paths.SQL_EDITOR), [])
        self.assertEqual(keyboard_paths.malformed({"Run": "press it"}), ["Run"])

    def test_gaps_are_listed_with_their_reason(self):
        self.assertEqual(keyboard_paths.gaps({"Split": "gap: no shortcut"}), ["Split: gap: no shortcut"])


if __name__ == "__main__":
    unittest.main()
