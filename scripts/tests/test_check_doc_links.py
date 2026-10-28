import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("check_doc_links", ROOT / "scripts/check-doc-links.py")
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


class DocLinkTests(unittest.TestCase):
    def broken(self, files):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, text in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            return [(str(p.relative_to(root)), target) for p, target in checker.broken_links(root, [root / n for n in files if n.endswith(".md")])]

    def test_a_link_to_an_existing_file_is_fine(self):
        self.assertEqual(self.broken({"a.md": "[b](b.md)", "b.md": "x"}), [])

    def test_a_link_to_a_missing_file_is_reported_with_its_target(self):
        self.assertEqual(self.broken({"a.md": "[b](gone.md)"}), [("a.md", "gone.md")])

    def test_a_link_label_can_contain_nested_brackets(self):
        files = {"a.md": "[date[] inside [label]](gone.md) [ok](ok.md)", "ok.md": "x"}
        self.assertEqual(self.broken(files), [("a.md", "gone.md")])

    def test_a_valid_heading_anchor_and_query_are_fine(self):
        self.assertEqual(self.broken({"a.md": "[b](b.md?view=raw#top)", "b.md": "# Top"}), [])

    def test_a_missing_heading_anchor_is_reported(self):
        self.assertEqual(self.broken({"a.md": "[b](b.md#gone)", "b.md": "# Here"}), [("a.md", "b.md#gone")])

    def test_duplicate_headings_get_numbered_anchors(self):
        self.assertEqual(self.broken({"a.md": "[b](b.md#section-1)", "b.md": "# Section\n# Section"}), [])

    def test_anchors_on_non_markdown_files_are_not_checked(self):
        self.assertEqual(self.broken({"a.md": "[b](b.rs#top)", "b.rs": "fn main() {}"}), [])

    def test_a_missing_heading_anchor_is_reported_even_when_the_file_exists(self):
        self.assertEqual(self.broken({"a.md": "[b](b.md#top)", "b.md": "x"}), [("a.md", "b.md#top")])
        self.assertEqual(self.broken({"a.md": "[b](gone.md#top)"}), [("a.md", "gone.md")])

    def test_urls_and_pure_anchors_are_not_files(self):
        text = "# Here\n[u](https://example.com/x) [m](mailto:a@b.c) [h](#here) [t](tel:123) [f](ftp://host/file)"
        self.assertEqual(self.broken({"a.md": text}), [])

    def test_a_missing_pure_anchor_is_reported(self):
        self.assertEqual(self.broken({"a.md": "# Here\n[missing](#gone)"}), [("a.md", "#gone")])

    def test_explicit_html_anchor_accepts_attributes_and_ignores_fenced_examples(self):
        files = {
            "a.md": "[manual](b.md#manual) [example](b.md#fake)",
            "b.md": "<A class='target' title='a > b' id='manual'></A>\n```html\n<a id='fake'>\n```",
        }
        self.assertEqual(self.broken(files), [("a.md", "b.md#fake")])

    def test_heading_level_indentation_closing_hashes_and_link_text_form_anchors(self):
        files = {
            "a.md": "[heading](b.md#linked-heading)",
            "b.md": "   ### [Linked heading](other.md) ###",
            "other.md": "# Linked heading target",
        }
        self.assertEqual(self.broken(files), [])

    def test_heading_code_spelling_preserves_underscores(self):
        self.assertEqual(
            self.broken({"a.md": "[section](#sql-server-sql_variant-metadata-refusal)\n## SQL Server `sql_variant` metadata refusal"}),
            [],
        )

    def test_links_resolve_relative_to_the_linking_file(self):
        files = {"docs/a.md": "[b](../crates/x.rs)", "crates/x.rs": "fn main() {}"}
        self.assertEqual(self.broken(files), [])

    def test_links_inside_code_fences_are_not_checked(self):
        self.assertEqual(self.broken({"a.md": "```\n[b](gone.md)\n```\n"}), [])

    def test_links_into_generated_target_output_are_plain_text(self):
        self.assertEqual(self.broken({"a.md": "[r](../target/quality/x/report.json)"}), [])

    def test_markdown_in_generated_package_output_is_not_indexed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            generated = root / "apps/linux/packaging/out/pkg/usr/share/doc/app/LICENSE.md"
            source = root / "docs/source.md"
            generated.parent.mkdir(parents=True)
            source.parent.mkdir(parents=True)
            generated.write_text("[missing](../not-shipped.md)")
            source.write_text("source")
            files = checker.markdown_files(root)
            self.assertIn(source, files)
            self.assertNotIn(generated, files)

    def test_ignored_arch_vendor_markdown_is_excluded_but_new_docs_are_checked(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".gitignore").write_text("apps/linux/packaging/arch/src/\n")
            tracked = root / "docs/tracked.md"
            tracked.parent.mkdir(parents=True)
            tracked.write_text("[target](target.md)\n")
            (root / "docs/target.md").write_text("target\n")
            for args in [
                ["init", "-q"],
                ["config", "user.name", "Fixture"],
                ["config", "user.email", "fixture@example.invalid"],
                ["add", ".gitignore", "docs/tracked.md", "docs/target.md"],
                ["commit", "-qm", "fixture"],
            ]:
                subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True)
            new_doc = root / "docs/new.md"
            new_doc.write_text("[missing](missing.md)\n")
            vendor_doc = root / "apps/linux/packaging/arch/src/TablePro/cargo-vendor/crate/README.md"
            vendor_doc.parent.mkdir(parents=True)
            vendor_doc.write_text("[third party link](missing-vendor.md)\n")

            files = checker.markdown_files(root)
            self.assertEqual(set(files), {tracked, root / "docs/target.md", new_doc})
            self.assertEqual(checker.broken_links(root, files), [(new_doc, "missing.md")])

    def test_markdown_files_remain_discoverable_without_git_metadata(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "docs/source.md"
            source.parent.mkdir(parents=True)
            source.write_text("[missing](missing.md)\n")
            self.assertEqual(checker.markdown_files(root), [source])

    def test_flatpak_cache_links_are_skipped_while_source_errors_are_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            generated = root / ".flatpak-builder/build/dependency/README.md"
            source = root / "docs/source.md"
            generated.parent.mkdir(parents=True)
            source.parent.mkdir(parents=True)
            generated.write_text("[missing](missing-generated.md)")
            source.write_text("[missing](missing-source.md)")
            self.assertEqual(
                checker.broken_links(root, checker.markdown_files(root)),
                [(source, "missing-source.md")],
            )

    def test_a_directory_target_counts_as_existing(self):
        self.assertEqual(self.broken({"a.md": "[d](dir/)", "dir/f.txt": "x"}), [])


if __name__ == "__main__":
    unittest.main()
