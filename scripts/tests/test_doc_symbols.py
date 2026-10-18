import tempfile
import unittest
from pathlib import Path

import importlib.util

SCRIPT = Path(__file__).resolve().parents[1] / "check-doc-symbols.py"
SPEC = importlib.util.spec_from_file_location("check_doc_symbols", SCRIPT)
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class DocSymbolTests(unittest.TestCase):
    def test_accepts_a_documented_type_that_exists(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "guide.md").write_text("Uses `Widget`.\n", encoding="utf-8")
            (root / "widget.rs").write_text("pub struct Widget {}\n", encoding="utf-8")
            self.assertEqual(CHECKER.check_references(root, (("guide.md", "widget.rs", "Widget", "struct"),)), [])

    def test_rejects_a_stale_source_symbol(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "guide.md").write_text("Uses `Widget`.\n", encoding="utf-8")
            (root / "widget.rs").write_text("pub struct Gadget {}\n", encoding="utf-8")
            self.assertIn(
                "guide.md: struct Widget is missing from widget.rs",
                CHECKER.check_references(root, (("guide.md", "widget.rs", "Widget", "struct"),)),
            )

    def test_rejects_a_missing_named_source_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "guide.md").write_text("Run `tool.sh`.\n", encoding="utf-8")
            self.assertIn(
                "guide.md: referenced source path is missing: tool.sh",
                CHECKER.check_references(root, (("guide.md", "tool.sh", "tool.sh", "file"),)),
            )


if __name__ == "__main__":
    unittest.main()
