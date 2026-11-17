import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "apps/linux/app/tests"))

import keyboard_paths


class KeyboardPathRegistryTests(unittest.TestCase):
    def test_an_unlisted_clickable_is_reported(self):
        names = ["Run", "Open safety_items", "Open orders"]
        self.assertEqual(keyboard_paths.unregistered(names, keyboard_paths.SQL_EDITOR), ["Open orders"])

    def test_every_entry_names_its_kind_of_path(self):
        self.assertEqual(keyboard_paths.malformed(keyboard_paths.SQL_EDITOR), [])
        self.assertEqual(keyboard_paths.malformed({"Run": "press it"}), ["Run"])

    def test_sidebar_row_accessible_name_has_a_keyboard_path(self):
        name = "safety_items Show columns Open safety_items"
        self.assertEqual(keyboard_paths.unregistered([name], keyboard_paths.SQL_EDITOR), [])


if __name__ == "__main__":
    unittest.main()
