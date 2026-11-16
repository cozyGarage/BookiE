import hashlib
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VALIDATOR = ROOT / "scripts/validate_release_inputs.py"


class ReleaseInputValidationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bookie-release-inputs-")
        self.addCleanup(self.temporary.cleanup)
        self.inputs = Path(self.temporary.name)
        self.commit = "1" * 40
        self.version = "0.2.7"
        self.source = self.inputs / f"bookie-{self.version}-{self.commit}.tar.gz"
        self.vendor = self.inputs / f"bookie-vendor-{self.version}-{self.commit}.tar.gz"
        self.metadata = self.inputs / f"bookie-{self.version}-{self.commit}-inputs.json"
        self._write_archive(
            self.source,
            f"BookiE-{self.commit}",
            {
                "Cargo.toml": b"[workspace.package]\nversion = \"0.2.7\"\n",
                "apps/linux/app/Cargo.toml": b"[package]\n",
                "crates/agentd/Cargo.toml": b"[package]\n",
                "crates/ssh/Cargo.toml": b"[package]\n",
                "apps/linux/packaging/arch/PKGBUILD": b"recipe\n",
            },
        )
        self._write_archive(
            self.vendor,
            f"BookiE-{self.commit}",
            {".cargo/config.toml": b"[source.crates-io]\n", "cargo-vendor/": None},
        )
        self._write_metadata()

    def _write_archive(self, path, prefix, members):
        with tarfile.open(path, "w:gz") as archive:
            for name, content in members.items():
                info = tarfile.TarInfo(f"{prefix}/{name}")
                if content is None:
                    info.type = tarfile.DIRTYPE
                    archive.addfile(info)
                    continue
                info.size = len(content)
                archive.addfile(info, io.BytesIO(content))

    def _write_metadata(self, **updates):
        metadata = {
            "commit": self.commit,
            "package_version": self.version,
            "source_version": self.version,
            "release_date": "2026-11-15",
            "target": "x86_64-unknown-linux-gnu",
            "cargo": "cargo 1.98.0",
            "rustc": "rustc 1.98.0",
            "workspace_roots": ["bookie-app:kerberos", "bookie-agentd:kerberos", "bookie-ssh:bookie-askpass"],
            "native_build_inputs": {"depends": ["gtk4"], "makedepends": ["rust"], "checkdepends": ["namcap"]},
            "source_archive_sha256": hashlib.sha256(self.source.read_bytes()).hexdigest(),
            "vendor_archive_sha256": hashlib.sha256(self.vendor.read_bytes()).hexdigest(),
            "arch_recipe_sha256": hashlib.sha256(b"recipe\n").hexdigest(),
        }
        metadata.update(updates)
        self.metadata.write_text(json.dumps(metadata))

    def _validate(self, commit=None, version=None):
        return subprocess.run(
            [
                sys.executable,
                str(VALIDATOR),
                str(self.inputs),
                commit or self.commit,
                version or self.version,
            ],
            capture_output=True,
            text=True,
        )

    def test_valid_inputs_return_the_verified_archive_paths(self):
        result = self._validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout),
            {"metadata": str(self.metadata), "source_archive": str(self.source), "vendor_archive": str(self.vendor)},
        )

    def test_other_commit_and_version_are_rejected(self):
        for commit, version, diagnostic in (
            ("2" * 40, self.version, "commit does not match"),
            (self.commit, "0.2.8", "version does not match"),
        ):
            with self.subTest(commit=commit, version=version):
                result = self._validate(commit, version)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)

    def test_invalid_release_metadata_is_rejected(self):
        for updates, diagnostic in (
            ({"release_date": "2026-02-30"}, "invalid release date"),
            ({"target": "aarch64-unknown-linux-gnu"}, "unsupported target"),
            ({"source_archive_sha256": "0" * 64}, "checksum does not match"),
            ({"workspace_roots": []}, "workspace feature provenance"),
            ({"native_build_inputs": {"depends": ["gtk4"]}}, "native build provenance"),
            ({"native_build_inputs": {"depends": [], "makedepends": ["rust"], "checkdepends": ["namcap"]}}, "native build provenance"),
        ):
            with self.subTest(updates=updates):
                self._write_metadata(**updates)
                result = self._validate()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)

    def test_missing_vendor_input_is_rejected(self):
        self.vendor.unlink()
        result = self._validate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("required release input is missing", result.stderr)

    def test_unsafe_vendor_archive_paths_and_links_are_rejected(self):
        for member_name, member_type, diagnostic in (
            (f"BookiE-{self.commit}/../escape", tarfile.REGTYPE, "unsafe path"),
            (f"BookiE-{self.commit}/cargo-vendor/escape", tarfile.SYMTYPE, "contains a link"),
            (f"BookiE-{self.commit}/cargo-vendor/device", tarfile.FIFOTYPE, "unsupported file type"),
        ):
            with self.subTest(member_name=member_name):
                with tarfile.open(self.vendor, "w:gz") as archive:
                    for name, content in (
                        (f"BookiE-{self.commit}/.cargo/config.toml", b"[source.crates-io]\n"),
                        (f"BookiE-{self.commit}/cargo-vendor/", None),
                    ):
                        info = tarfile.TarInfo(name)
                        if content is None:
                            info.type = tarfile.DIRTYPE
                            archive.addfile(info)
                        else:
                            info.size = len(content)
                            archive.addfile(info, io.BytesIO(content))
                    unsafe = tarfile.TarInfo(member_name)
                    unsafe.type = member_type
                    unsafe.linkname = "/etc/passwd"
                    archive.addfile(unsafe)
                self._write_metadata()
                result = self._validate()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(diagnostic, result.stderr)


if __name__ == "__main__":
    unittest.main()
