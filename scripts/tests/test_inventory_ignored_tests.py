from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/inventory-ignored-tests.py"
BUILD_TREE = ROOT / "apps/linux/packaging/arch/src"


class InventoryIgnoredTestsTests(unittest.TestCase):
    def test_a_known_gpui_issue_is_reported_without_being_registered_for_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "apps/gpui"
            (app / "src").mkdir(parents=True)
            (app / "Cargo.toml").write_text(
                '[package]\nname = "bookie-gpui"\nversion = "0.1.0"\nedition = "2024"\n'
            )
            (app / "src/ui_tests.rs").write_text(
                '#[test]\n#[ignore = "known issue: keyboard paging"]\nfn keyboard_paging() {}\n'
            )
            scripts = root / "scripts"
            scripts.mkdir()
            (scripts / "inventory-ignored-tests.py").write_text(SCRIPT.read_text())
            (scripts / "isolated-tests.json").write_text("{}\n")
            (root / "Cargo.toml").write_text('[workspace]\nmembers = ["apps/gpui"]\n')
            subprocess.run(["git", "-C", str(root), "init", "-q"], check=True)
            subprocess.run(["git", "-C", str(root), "add", "Cargo.toml", "apps", "scripts"], check=True)

            result = subprocess.run(
                [sys.executable, str(scripts / "inventory-ignored-tests.py")],
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("| [keyboard_paging](../apps/gpui/src/ui_tests.rs) | GPUI |", result.stdout)

    def test_an_isolated_test_cannot_name_a_missing_workspace_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/core"]\nresolver = "3"\n')
            crate = root / "crates/core"
            (crate / "src").mkdir(parents=True)
            (crate / "Cargo.toml").write_text(
                '[package]\nname = "bookie-core"\nversion = "0.1.0"\nedition = "2024"\n'
            )
            (crate / "src/lib.rs").write_text("")
            scripts = root / "scripts"
            scripts.mkdir()
            inventory = scripts / "inventory-ignored-tests.py"
            inventory.write_text(SCRIPT.read_text())
            (scripts / "isolated-tests.json").write_text(
                '{"gtk": [["tablepro-app", "--lib", "test_name"]]}\n'
            )
            subprocess.run(["git", "-C", str(root), "init", "-q"], check=True)
            subprocess.run(
                ["git", "-C", str(root), "add", "Cargo.toml", "crates", "scripts"], check=True
            )

            result = subprocess.run(
                [sys.executable, str(inventory), "--check"], capture_output=True, text=True, check=False
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unknown Cargo package in isolated-test registry: tablepro-app", result.stderr)

    def test_a_gitignored_build_tree_is_not_scanned(self):
        existed = BUILD_TREE.exists()
        stray = BUILD_TREE / "regression-probe/apps/linux/app/src/stray.rs"
        stray.parent.mkdir(parents=True, exist_ok=True)
        stray.write_text('#[test]\n#[ignore = "requires Docker"]\nfn stray() {}\n')
        try:
            ignored = subprocess.run(["git", "check-ignore", "-q", str(stray)], cwd=ROOT).returncode == 0
            self.assertTrue(ignored, "the probe path must be gitignored for this test to mean anything")
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--check"], cwd=ROOT, capture_output=True, text=True, timeout=120
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        finally:
            shutil.rmtree(BUILD_TREE / "regression-probe", ignore_errors=True)
            if not existed:
                shutil.rmtree(BUILD_TREE, ignore_errors=True)

    def test_a_known_gpui_limitation_has_an_inventory_tier(self):
        result = subprocess.run(
            [sys.executable, str(SCRIPT)], cwd=ROOT, capture_output=True, text=True, timeout=120
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(
            "| [the_cursor_crosses_to_the_next_page_and_back](../apps/gpui/src/ui_tests.rs) | GPUI |",
            result.stdout,
        )


if __name__ == "__main__":
    unittest.main()
