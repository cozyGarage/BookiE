import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


class AppImageGlibcFloorTests(unittest.TestCase):
    def load_checker(self):
        spec = importlib.util.spec_from_file_location(
            "appimage_glibc_floor", ROOT / "scripts/check-appimage-glibc-floor.py"
        )
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_reads_only_required_glibc_versions(self):
        checker = self.load_checker()
        report = """Version needs section '.gnu.version_r' contains 1 entry:
  0x0010: Version: 1  File: libc.so.6  Cnt: 2
  0x0020:   Name: GLIBC_2.39  Flags: none  Version: 4
  0x0030:   Name: GLIBC_2.43  Flags: none  Version: 5
Version definition section '.gnu.version_d' contains 1 entry:
  0x0040: Rev: 1  Flags: BASE  Index: 1  Cnt: 1
  0x0050:   Name: GLIBC_2.50
"""
        self.assertEqual(checker.required_glibc_versions(report), [(2, 39), (2, 43)])

    def test_accepts_floor_and_rejects_newer_appimage_dependency(self):
        checker = self.load_checker()
        with tempfile.TemporaryDirectory() as directory:
            elf = Path(directory) / "libgtk.so"
            elf.write_bytes(b"\x7fELFfixture")
            with patch.object(checker.subprocess, "run") as run:
                needs = "Version needs section '.gnu.version_r':\n  Name: GLIBC_2.43\n"
                run.return_value = subprocess.CompletedProcess([], 0, stdout=needs, stderr="")
                checker.check_appdir(Path(directory), (2, 43))
                run.return_value = subprocess.CompletedProcess(
                    [], 0, stdout=needs.replace("2.43", "2.44"), stderr=""
                )
                with self.assertRaisesRegex(ValueError, "2.44.*2.43"):
                    checker.check_appdir(Path(directory), (2, 43))

    def test_rejects_an_appdir_without_elf_files(self):
        checker = self.load_checker()
        with tempfile.TemporaryDirectory() as directory:
            (Path(directory) / "text").write_text("not an ELF")
            with self.assertRaisesRegex(ValueError, "no ELF files"):
                checker.check_appdir(Path(directory), (2, 43))

    def test_build_enforces_the_documented_floor(self):
        floor = (ROOT / "scripts/appimage-glibc-floor.txt").read_text().strip()
        platforms = (ROOT / "docs/platforms.md").read_text()
        build = (ROOT / "scripts/build-appimage.sh").read_text()
        self.assertEqual(floor, "2.43")
        self.assertIn(f"AppImage requires glibc {floor} or newer", platforms)
        self.assertIn("check-appimage-glibc-floor.py", build)
        self.assertIn("APPIMAGE_GLIBC_FLOOR", build)


if __name__ == "__main__":
    unittest.main()
