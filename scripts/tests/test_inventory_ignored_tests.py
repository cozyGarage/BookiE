from pathlib import Path
import shutil
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/inventory-ignored-tests.py"
BUILD_TREE = ROOT / "apps/linux/packaging/arch/src"


class InventoryIgnoredTestsTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
