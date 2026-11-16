import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/update-aur-recipe.py"
RECIPE = """pkgname=bookie
pkgver=0.2.6
pkgrel=3
depends=('gtk4')
_tag="linux-v${pkgver}"
_commit=3b3d4ce1d1dd161df3196ae2f46b18bcefc066c5
source=("${pkgname}::git+https://example.test/bookie.git#tag=${_tag}")
"""


class UpdateAurRecipeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bookie-aur-recipe-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "--quiet")
        self.git("config", "user.name", "Release Test")
        self.git("config", "user.email", "release@example.test")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "tag.gpgsign", "false")
        (self.repo / "file.txt").write_text("release\n")
        self.git("add", "file.txt")
        self.git("commit", "--quiet", "-m", "release")
        self.git("tag", "-a", "-m", "release", "linux-v0.2.7")
        self.commit = self.git("rev-parse", "HEAD^{commit}").strip()
        self.recipe = self.root / "PKGBUILD"
        self.recipe.write_text(RECIPE)

    def git(self, *arguments):
        return subprocess.run(
            ["git", "-C", str(self.repo), *arguments], check=True, capture_output=True, text=True
        ).stdout

    def run_script(self, *arguments):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--repo", str(self.repo), "--recipe", str(self.recipe), *arguments],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_it_pins_the_version_release_and_commit_of_the_tag_and_changes_nothing_else(self):
        result = self.run_script("linux-v0.2.7")
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = (
            RECIPE.replace("pkgver=0.2.6", "pkgver=0.2.7")
            .replace("pkgrel=3", "pkgrel=1")
            .replace("3b3d4ce1d1dd161df3196ae2f46b18bcefc066c5", self.commit)
        )
        self.assertEqual(self.recipe.read_text(), expected)

    def test_a_second_run_changes_nothing_and_check_accepts_it(self):
        self.assertEqual(self.run_script("linux-v0.2.7").returncode, 0)
        first = self.recipe.read_text()
        self.assertEqual(self.run_script("linux-v0.2.7").returncode, 0)
        self.assertEqual(self.recipe.read_text(), first)
        self.assertEqual(self.run_script("linux-v0.2.7", "--check").returncode, 0)

    def test_check_reports_a_stale_recipe_without_writing_it(self):
        result = self.run_script("linux-v0.2.7", "--check")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.recipe.read_text(), RECIPE)

    def test_output_leaves_the_recipe_untouched(self):
        target = self.root / "out" / "PKGBUILD"
        target.parent.mkdir()
        self.assertEqual(self.run_script("linux-v0.2.7", "--output", str(target)).returncode, 0)
        self.assertEqual(self.recipe.read_text(), RECIPE)
        self.assertIn(f"_commit={self.commit}", target.read_text())

    def test_it_refuses_tags_that_are_not_releases_or_do_not_exist(self):
        for tag in ["v0.2.7", "linux-v0.2", "linux-v0.2.7-rc1", "linux-v0.2.7; rm -rf /", "linux-v9.9.9"]:
            result = self.run_script(tag)
            self.assertNotEqual(result.returncode, 0, tag)
            self.assertEqual(self.recipe.read_text(), RECIPE, tag)

    def test_it_refuses_a_recipe_that_does_not_define_each_field_once(self):
        for broken in [RECIPE.replace("pkgrel=3\n", ""), RECIPE + "_commit=" + "0" * 40 + "\n"]:
            self.recipe.write_text(broken)
            self.assertEqual(self.run_script("linux-v0.2.7").returncode, 2)
            self.assertEqual(self.recipe.read_text(), broken)


if __name__ == "__main__":
    unittest.main()
