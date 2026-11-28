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


    def test_code_after_a_test_module_still_counts(self):
        source = "#[cfg(test)]\nmod t {\n    fn f() { thread_local! {} }\n}\nfn g() { thread_local! {} }\n"
        self.assertEqual(self.counted(source), {"thread_local": 1})

    def test_an_indented_test_item_skips_only_that_item(self):
        source = "impl X {\n    #[cfg(test)]\n    fn probe() { thread_local! {} }\n    fn real() { thread_local! {} }\n}\n"
        self.assertEqual(self.counted(source), {"thread_local": 1})

    def test_gpui_colour_text_size_empty_label_info_and_shadow_count(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "apps" / "gpui" / "src" / "view.rs"
            path.parent.mkdir(parents=True)
            path.write_text(
                ".bg(rgba(0x00000088))\n.text_color(hsla(0., 0., 0.7, 0.45))\n.text_xs()\n.text_sm()\n"
                'ghost("toggle", "", theme)\n.text_color(rgb(theme.info))\n.shadow_lg()\n.px(px(8.0 + 14.0 * depth))\n'
                'Button::new("menu")\n'
            )
            found = checker.count(path)
        self.assertEqual(found.get("gpui-raw-colour"), 2)
        self.assertEqual(found.get("gpui-text-size-class"), 2)
        self.assertEqual(found.get("gpui-empty-label"), 1)
        self.assertEqual(found.get("gpui-info-colour"), 1)
        self.assertEqual(found.get("gpui-shadow"), 1)
        self.assertEqual(found.get("gpui-literal-px"), 1)
        self.assertEqual(found.get("gpui-button-new"), 1)

    def test_a_gpui_click_without_an_accessible_name_counts(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "apps" / "gpui" / "src" / "view.rs"
            path.parent.mkdir(parents=True)
            path.write_text(
                'let a = icon_button("close", "x", "Close", theme).on_click(listener);\n'
                'let b = div()\n    .id("row")\n    .aria_label("Open")\n    .on_click(listener);\n'
                'let c = div()\n    .id("bare")\n    .child(icon("x"))\n    .on_click(listener);\n'
            )
            found = checker.count(path)
        self.assertEqual(found.get("gpui-unnamed-click"), 1)


if __name__ == "__main__":
    unittest.main()
