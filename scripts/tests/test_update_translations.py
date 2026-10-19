import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "update-translations.py"
SPEC = importlib.util.spec_from_file_location("update_translations", SCRIPT)
TRANSLATIONS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRANSLATIONS)


class TranslationGenerationTests(unittest.TestCase):
    def test_finds_qualified_calls_and_returns_sorted_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "apps/linux/app/src"
            (source / "z.rs").parent.mkdir(parents=True)
            (source / "z.rs").write_text('crate::tr!("later");\n', encoding="utf-8")
            (source / "a.rs").write_text('tr!("first");\n', encoding="utf-8")
            (source / "unused.rs").write_text('println!("not translated");\n', encoding="utf-8")
            self.assertEqual(
                [path.relative_to(root).as_posix() for path in TRANSLATIONS.translation_sources(root)],
                ["apps/linux/app/src/a.rs", "apps/linux/app/src/z.rs"],
            )

    def test_rewrites_qualified_macro_paths_for_xgettext(self):
        source = 'crate::tr!("one"); app::i18n::tr!("two"); tr!("three");'
        self.assertEqual(TRANSLATIONS.rewrite_qualified_macros(source), 'tr!("one"); tr!("two"); tr!("three");')

    def test_extracts_rust_strings_without_parsing_the_rust_file(self):
        source = r'''crate::tr!("Hello \
            world\u{2026}");
let value = 3;
'''
        self.assertEqual(TRANSLATIONS.extraction_source(source), 'tr("Hello world\\u2026")\n\n\n')
        self.assertEqual(TRANSLATIONS.extraction_source('crate::tr!(\n    "Next line");\n'), '\ntr("Next line")\n')

    def test_normalizes_only_catalog_creation_timestamp(self):
        pot = '"POT-Creation-Date: 2026-10-19 10:20+0200\\n"\nmsgid "A"\n'
        normalized = TRANSLATIONS.normalize_catalog(pot)
        self.assertTrue(normalized.startswith('"POT-Creation-Date: 1970-01-01 00:00+0000'))
        self.assertIn('msgid "A"', normalized)

    def test_reports_pot_and_potfiles_drift_independently(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            catalog = root / "apps/linux/po/tablepro.pot"
            potfiles = root / "apps/linux/po/POTFILES.in"
            catalog.parent.mkdir(parents=True)
            catalog.write_text("current pot\n", encoding="utf-8")
            potfiles.write_text("stale source.rs\n", encoding="utf-8")
            self.assertEqual(
                TRANSLATIONS.stale_outputs(root, "current pot\n", "expected source.rs\n"),
                (potfiles,),
            )
            self.assertEqual(
                TRANSLATIONS.stale_outputs(root, "stale pot\n", "stale source.rs\n"),
                (catalog,),
            )


if __name__ == "__main__":
    unittest.main()
