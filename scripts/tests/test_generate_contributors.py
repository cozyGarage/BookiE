import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "generate_contributors", ROOT / "scripts/generate-contributors.py"
)
tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tool)


class GenerateContributorsTests(unittest.TestCase):
    def test_normalize_omits_bots_unless_requested(self):
        raw = [
            {"login": "alice", "type": "User"},
            {"login": "github-actions[bot]", "type": "Bot"},
            {"login": "dependabot[bot]", "type": "User"},
            {"login": "bob", "type": "User"},
        ]
        people = tool.normalize_contributors(raw, include_bots=False)
        self.assertEqual([person["login"] for person in people], ["alice", "bob"])
        with_bots = tool.normalize_contributors(raw, include_bots=True)
        self.assertEqual(
            [person["login"] for person in with_bots],
            ["alice", "github-actions[bot]", "dependabot[bot]", "bob"],
        )

    def test_avatar_grid_uses_stable_github_png_urls(self):
        grid = tool.avatar_grid([{"login": "alice"}, {"login": "bob"}], size=64)
        self.assertIn('href="https://github.com/alice"', grid)
        self.assertIn('src="https://github.com/alice.png?size=64"', grid)
        self.assertIn('alt="bob"', grid)

    def test_replace_marked_section_rewrites_between_markers(self):
        text = "before\n<!-- contributors:start -->\nold\n<!-- contributors:end -->\nafter\n"
        updated = tool.replace_marked_section(text, "<!-- contributors:start -->\nnew\n<!-- contributors:end -->")
        self.assertEqual(updated, "before\n<!-- contributors:start -->\nnew\n<!-- contributors:end -->\nafter\n")

    def test_ensure_readme_section_inserts_before_contributing(self):
        text = "## Packaging\n\ntext\n\n## Contributing\n\nRead the guide.\n"
        body = tool.readme_section([{"login": "alice"}])
        updated = tool.ensure_readme_section(text, body)
        self.assertIn("## Contributors", updated)
        self.assertLess(updated.index("## Contributors"), updated.index("## Contributing"))
        self.assertIn("CONTRIBUTORS.md", updated)

    def test_fetch_via_gh_reads_paginated_slurp_pages(self):
        result = subprocess.CompletedProcess(
            args=[],
            returncode=0,
            stdout=json.dumps(
                [
                    [{"login": "alice", "type": "User"}],
                    [{"login": "bob", "type": "User"}],
                ]
            ),
            stderr="",
        )
        people, error = tool.fetch_via_gh("cozyGarage", "BookiE", runner=Mock(return_value=result))
        self.assertIsNone(error)
        self.assertEqual([person["login"] for person in people], ["alice", "bob"])

    def test_contributors_document_mentions_refresh_command(self):
        document = tool.contributors_document([{"login": "alice"}], "cozyGarage/BookiE")
        self.assertIn("python3 scripts/generate-contributors.py", document)
        self.assertIn("GITHUB_TOKEN", document)
        self.assertIn(tool.START_MARKER, document)

    def test_write_paths_round_trip_in_temp_tree(self):
        people = [{"login": "alice", "type": "User"}]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            readme = root / "README.md"
            contributors = root / "CONTRIBUTORS.md"
            readme.write_text("## Contributing\n\nHi.\n", encoding="utf-8")
            document = tool.contributors_document(people, "cozyGarage/BookiE")
            section = tool.readme_section(people)
            updated = tool.ensure_readme_section(readme.read_text(encoding="utf-8"), section)
            contributors.write_text(document, encoding="utf-8")
            readme.write_text(updated if updated.endswith("\n") else updated + "\n", encoding="utf-8")
            self.assertIn("alice.png?size=64", contributors.read_text(encoding="utf-8"))
            self.assertIn("<!-- contributors:start -->", readme.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
