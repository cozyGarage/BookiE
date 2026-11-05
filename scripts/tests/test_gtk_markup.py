import importlib.util
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "check-gtk-markup.py"
SPEC = importlib.util.spec_from_file_location("check_gtk_markup", SCRIPT)
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class GtkMarkupTests(unittest.TestCase):
    def test_dynamic_row_text_requires_plain_text_mode(self):
        source = 'adw::ActionRow::builder().title(connection_name).subtitle("fixed").build();'
        self.assertEqual(len(CHECKER.source_violations(Path("row.rs"), source)), 1)

    def test_escaped_description_is_allowed(self):
        source = 'page.set_description(Some(&escape_markup(message)));'
        self.assertEqual(CHECKER.source_violations(Path("page.rs"), source), [])

    def test_unprotected_description_is_reported(self):
        source = 'page.set_description(Some(&message));'
        self.assertEqual(len(CHECKER.source_violations(Path("page.rs"), source)), 1)

    def test_banner_and_toast_construction_is_checked(self):
        source = 'adw::Banner::new(title); adw::Toast::new(message);'
        self.assertEqual(len(CHECKER.source_violations(Path("widget.rs"), source)), 2)


if __name__ == "__main__":
    unittest.main()
