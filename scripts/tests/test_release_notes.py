import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("release_notes", ROOT / "scripts/release-notes.py")
release_notes = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_notes)


class ReleaseNotesTests(unittest.TestCase):
    def test_cut_adds_a_link_for_the_new_release(self):
        text = "## [Unreleased]\n\n## [0.2.0] - 2026-10-01\n\n- Existing change\n\n[Unreleased]: old\n"

        result = release_notes.cut(text, "0.3.0", "2026-10-21")

        self.assertIn("[0.3.0]: https://github.com/cozyGarage/BookiE/releases/tag/linux-v0.3.0", result)
        self.assertIn(
            "[Unreleased]: https://github.com/cozyGarage/BookiE/compare/linux-v0.3.0...main",
            result,
        )

    def test_parse_accepts_notes_and_none(self):
        self.assertEqual(release_notes.parse_notes("fix\n\nRelease-Note: Fixed: Export keeps NULLs")[0], [("Fixed", "Export keeps NULLs")])
        self.assertEqual(release_notes.parse_notes("ci\n\nRelease-Note: none"), ([], []))

    def test_parse_accepts_a_note_after_escaped_line_breaks(self):
        message = r"feat(experiment): update the grid\n\nValidation passed.\n\nRelease-Note: none"

        self.assertEqual(release_notes.parse_notes(message), ([], []))

    def test_parse_rejects_missing_unknown_empty_and_mixed(self):
        for message in ("no trailer", "Release-Note: Fix: x", "Release-Note: Fixed:", "Release-Note: none\nRelease-Note: Added: x"):
            self.assertTrue(release_notes.parse_notes(message)[1], message)

    def test_parse_names_the_allowed_categories_for_each_malformed_shape(self):
        shapes = (
            "Release-Note: Improved: x",
            "Release-Note: none.",
            "Release-Note: Fixedx",
            "Release-Note: <Added|Changed|Fixed>: <what the user sees>",
            "Release-Note: Export keeps NULLs",
        )
        for message in shapes:
            errors = release_notes.parse_notes(message)[1]
            self.assertEqual(len(errors), 1, message)
            self.assertIn("Added, Changed, Deprecated, Removed, Fixed, Security", errors[0], message)

    def test_fragments_group_by_category_and_reject_bad_names_and_bodies(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "README.md").write_text("docs")
            (root / "a.fixed.md").write_text("Export keeps NULLs\n")
            (root / "b.added.md").write_text("Pager\n")
            grouped, errors = release_notes.read_fragments(root)
            self.assertEqual(errors, [])
            self.assertEqual(release_notes.render_fragments(grouped), "### Added\n\n- Pager\n\n### Fixed\n\n- Export keeps NULLs\n")
            for name, body in (("c.improved.md", "x"), ("d.Fixed.md", "x"), ("e.txt", "x"), ("f.fixed.md", ""), ("g.fixed.md", "one\ntwo")):
                (root / name).write_text(body)
            self.assertEqual(len(release_notes.read_fragments(root)[1]), 5)

    def test_releasing_fragments_adds_a_version_section_and_refuses_none(self):
        text = "## [Unreleased]\n\n## [0.2.0] - 2026-10-01\n\n[Unreleased]: old\n"
        grouped = {category: [] for category in release_notes.CATEGORIES}
        with self.assertRaises(SystemExit):
            release_notes.cut_fragments(text, "0.3.0", "2026-11-12", grouped)
        grouped["Fixed"].append("Export keeps NULLs")

        result = release_notes.cut_fragments(text, "0.3.0", "2026-11-12", grouped)

        self.assertIn("## [0.3.0] - 2026-11-12\n\n### Fixed\n\n- Export keeps NULLs\n", result)
        self.assertEqual(release_notes.section(result, "0.3.0"), "### Fixed\n\n- Export keeps NULLs\n")

    def test_trailers_pattern_finds_only_note_lines(self):
        body = "## Change\nText\n\nRelease-Note: Fixed: A\nRelease-Note: Added: B\n\n- [ ] Validation"

        self.assertEqual(release_notes.TRAILER.findall(body), ["Fixed: A", "Added: B"])

    def test_a_wrapped_note_keeps_its_whole_sentence(self):
        body = "## Release note\n\nRelease-Note: Changed: Transaction age labels show translated minute and\nhour units.\n\n## Validation\n- ok"

        self.assertEqual(release_notes.note_values(body), ["Changed: Transaction age labels show translated minute and hour units."])
        self.assertEqual(release_notes.parse_notes(body)[0], [("Changed", "Transaction age labels show translated minute and hour units.")])

    def test_a_note_ends_at_a_blank_line_a_heading_or_the_next_note(self):
        body = "Release-Note: Added: One\n## Next\nRelease-Note: Fixed: Two\nRelease-Note: none\n\ntail text"

        self.assertEqual(release_notes.note_values(body), ["Added: One", "Fixed: Two", "none"])

    def test_assemble_groups_by_category_and_adds_pull_numbers(self):
        commits = [
            ("fix(core): keep NULLs (#12)", "Release-Note: Fixed: Export keeps NULLs"),
            ("feat(app): add pager (#13)", "Release-Note: Added: Pager\nRelease-Note: Changed: Wider footer"),
            ("ci: cache (#14)", "Release-Note: none"),
        ]

        result = release_notes.assemble(commits)

        self.assertEqual(result, "### Added\n\n- Pager (#13)\n\n### Changed\n\n- Wider footer (#13)\n\n### Fixed\n\n- Export keeps NULLs (#12)\n")

    def test_assemble_skips_direct_pushes_and_merge_commits_without_a_note(self):
        commits = [
            ("docs: reconcile evidence", "no trailer"),
            ("Merge remote-tracking branch 'fork/linux' into topic", ""),
            ("docs: add a note when wanted", "Release-Note: Changed: Wider footer"),
            ("fix(app): keep NULLs (#9)", "Release-Note: Fixed: Export keeps NULLs"),
        ]

        result = release_notes.assemble(commits)

        self.assertIn("- Wider footer", result)
        self.assertIn("- Export keeps NULLs (#9)", result)
        self.assertNotIn("evidence", result)

    def test_assemble_refuses_a_commit_without_a_note_unless_allowed(self):
        commits = [("fix: x (#1)", "no trailer"), ("fix: y (#2)", "Release-Note: Fixed: y")]

        with self.assertRaises(SystemExit):
            release_notes.assemble(commits)
        self.assertIn("- y (#2)", release_notes.assemble(commits, allow_missing=True))


if __name__ == "__main__":
    unittest.main()
