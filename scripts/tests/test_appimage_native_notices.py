import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "update-appimage-native-notices.py"
SPEC = importlib.util.spec_from_file_location("appimage_native_notices", SCRIPT)
NOTICES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NOTICES)


class AppImageNativeNoticeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.appdir = self.root / "appdir"
        self.artifact = self.appdir / "usr/lib/libfixture.so.1"
        self.source = self.root / "usr/lib/x86_64-linux-gnu/libfixture.so.1"
        self.copyright = self.root / "usr/share/doc/fixture/copyright"
        self.artifact.parent.mkdir(parents=True)
        self.source.parent.mkdir(parents=True)
        self.copyright.parent.mkdir(parents=True)
        elf = b"\x7fELF" + b"fixture-library"
        self.artifact.write_bytes(elf)
        self.source.write_bytes(elf)
        self.copyright_text = (
            b"Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n"
            b"License: GPL-3+\n See /usr/share/common-licenses/GPL-3.\n"
        )
        self.copyright.write_bytes(self.copyright_text)
        self.common_license = self.root / "usr/share/common-licenses/GPL-3"
        self.common_license.parent.mkdir(parents=True)
        self.common_license_text = b"GNU GENERAL PUBLIC LICENSE\nVersion 3\n"
        self.common_license.write_bytes(self.common_license_text)

    def command(self, args, **_kwargs):
        if args[:2] == ["readelf", "-n"]:
            return NOTICES.subprocess.CompletedProcess(args, 0, "Build ID: abcd1234\n", "")
        if args[:2] == ["dpkg-query", "-S"]:
            return NOTICES.subprocess.CompletedProcess(
                args, 0, "fixture:amd64: /usr/lib/x86_64-linux-gnu/libfixture.so.1\n", ""
            )
        if args[:2] == ["dpkg-query", "-W"]:
            return NOTICES.subprocess.CompletedProcess(args, 0, "1.0-1\n", "")
        if args[:2] == ["dpkg-query", "-L"]:
            return NOTICES.subprocess.CompletedProcess(
                args,
                0,
                "/usr/lib/x86_64-linux-gnu/libfixture.so.1\n/usr/share/doc/fixture/copyright\n",
                "",
            )
        raise AssertionError(args)

    def test_copies_exact_package_copyright_and_records_artifact_provenance_deterministically(self):
        with patch.object(NOTICES.subprocess, "run", side_effect=self.command):
            count = NOTICES.generate(self.appdir, self.root)
            bundle = self.appdir / NOTICES.OUTPUT
            first = {path.relative_to(bundle): path.read_bytes() for path in bundle.rglob("*") if path.is_file()}
            self.assertEqual(NOTICES.generate(self.appdir, self.root), count)
            second = {path.relative_to(bundle): path.read_bytes() for path in bundle.rglob("*") if path.is_file()}

        self.assertEqual(count, (1, 1))
        self.assertEqual(first, second)
        manifest = json.loads(first[Path("manifest.json")])
        package = manifest["packages"][0]
        self.assertEqual(package["binary_package"], "fixture:amd64")
        self.assertEqual(package["version"], "1.0-1")
        self.assertEqual(package["bundled_elfs"][0]["match"], "build-id")
        self.assertEqual(package["bundled_elfs"][0]["source_path"], "/usr/lib/x86_64-linux-gnu/libfixture.so.1")
        self.assertEqual(package["notice_texts"][0]["sha256"], hashlib.sha256(self.copyright_text).hexdigest())
        notice = Path("licenses/fixture_amd64/fixture/copyright")
        self.assertEqual(first[notice], self.copyright_text)
        common_license = Path("licenses/fixture_amd64/common-licenses/GPL-3")
        self.assertEqual(first[common_license], self.common_license_text)
        self.assertEqual(package["notice_texts"][1]["kind"], "common-license")
        self.assertEqual(package["notice_texts"][1]["source_path"], "/usr/share/common-licenses/GPL-3")
        self.assertEqual(package["notice_texts"][1]["sha256"], hashlib.sha256(self.common_license_text).hexdigest())

    def test_fails_closed_when_an_artifact_library_has_no_package_provenance(self):
        def unowned(args, **kwargs):
            if args[:2] == ["dpkg-query", "-S"]:
                return NOTICES.subprocess.CompletedProcess(args, 1, "", "no path found")
            return self.command(args, **kwargs)

        with patch.object(NOTICES.subprocess, "run", side_effect=unowned):
            with self.assertRaisesRegex(RuntimeError, "no matching Debian package file"):
                NOTICES.generate(self.appdir, self.root)

    def test_fails_closed_when_the_owner_has_no_copyright_text(self):
        def no_copyright(args, **kwargs):
            if args[:2] == ["dpkg-query", "-L"]:
                return NOTICES.subprocess.CompletedProcess(args, 0, "/usr/lib/x86_64-linux-gnu/libfixture.so.1\n", "")
            return self.command(args, **kwargs)

        with patch.object(NOTICES.subprocess, "run", side_effect=no_copyright):
            with self.assertRaisesRegex(RuntimeError, "no packaged copyright file"):
                NOTICES.generate(self.appdir, self.root)

    def test_follows_packaged_doc_directory_symlink_to_copyright(self):
        package_doc = self.root / "usr/share/doc/fixture"
        self.copyright.unlink()
        package_doc.rmdir()
        source_doc = self.root / "usr/share/doc/gcc-base"
        source_doc.mkdir()
        (source_doc / "copyright").write_bytes(self.copyright_text)
        package_doc.symlink_to("gcc-base", target_is_directory=True)

        def shared_doc_directory(args, **kwargs):
            if args[:2] == ["dpkg-query", "-L"]:
                return NOTICES.subprocess.CompletedProcess(
                    args,
                    0,
                    "/usr/lib/x86_64-linux-gnu/libfixture.so.1\n/usr/share/doc/fixture\n",
                    "",
                )
            return self.command(args, **kwargs)

        with patch.object(NOTICES.subprocess, "run", side_effect=shared_doc_directory):
            self.assertEqual(NOTICES.generate(self.appdir, self.root), (1, 1))

        bundle = self.appdir / NOTICES.OUTPUT
        manifest = json.loads((bundle / "manifest.json").read_text())
        package = manifest["packages"][0]
        self.assertEqual(package["notice_texts"][0]["source_path"], "/usr/share/doc/fixture/copyright")
        notice = bundle / "licenses/fixture_amd64/fixture/copyright"
        self.assertEqual(notice.read_bytes(), self.copyright_text)

    def test_fails_closed_when_a_referenced_common_license_is_missing(self):
        self.common_license.unlink()
        with patch.object(NOTICES.subprocess, "run", side_effect=self.command):
            with self.assertRaisesRegex(RuntimeError, "references missing common license text"):
                NOTICES.generate(self.appdir, self.root)

    def test_uses_exact_hash_when_a_library_has_no_build_id(self):
        def no_build_id(args, **kwargs):
            if args[:2] == ["readelf", "-n"]:
                return NOTICES.subprocess.CompletedProcess(args, 0, "", "")
            return self.command(args, **kwargs)

        with patch.object(NOTICES.subprocess, "run", side_effect=no_build_id):
            count = NOTICES.generate(self.appdir, self.root)
        self.assertEqual(count, (1, 1))
        manifest = json.loads((self.appdir / NOTICES.OUTPUT / "manifest.json").read_text())
        self.assertEqual(manifest["packages"][0]["bundled_elfs"][0]["match"], "sha256")

    def test_release_build_embeds_and_verifies_notices_in_the_published_image(self):
        root = SCRIPT.parents[1]
        build = (root / "scripts/build-appimage.sh").read_text()
        release = (root / ".github/workflows/release.yml").read_text()

        generate = build.index("update-appimage-native-notices.py")
        package = build.index("--output appimage")
        verify = build.index('diff -r')
        self.assertLess(generate, package)
        self.assertLess(package, verify)
        self.assertIn('"scripts/update-appimage-native-notices.py"', release)


if __name__ == "__main__":
    unittest.main()
