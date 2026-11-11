import contextlib
import io
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "packaging_metadata", Path(__file__).resolve().parents[1] / "check-packaging-metadata.py"
)
METADATA = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(METADATA)

FILES = {
    "Cargo.toml": '[workspace.package]\nrust-version = "1.98"\nlicense = "AGPL-3.0-or-later"\n',
    "rust-toolchain.toml": '[toolchain]\nchannel = "1.98"\n',
    "meson.build": (
        "project('bookie', version: '0.2.0', license: 'AGPL-3.0-or-later')\n"
        "dependency('glib-2.0', version: '>=2.88')\n"
        "dependency('gtk4', version: '>=4.24')\n"
        "dependency('libadwaita-1', version: '>=1.10')\n"
        "dependency('gtksourceview-5', version: '>=5.22')\n"
        "application_id = 'com.bookie.linux'\n"
    ),
    "apps/linux/app/src/config.rs": (
        'pub const APP_ID: &str = default_env(option_env!("BOOKIE_APP_ID"), "com.bookie.linux");\n'
        'pub const SCHEMA_ID: &str = "com.bookie.linux";\n'
    ),
    "apps/linux/flatpak/com.bookie.linux.metainfo.xml": (
        "<component><id>com.bookie.linux</id><project_license>AGPL-3.0-or-later</project_license>"
        '<launchable type="desktop-id">com.bookie.linux.desktop</launchable>'
        '<url type="homepage">https://example.test/bookie</url></component>'
    ),
    "apps/linux/flatpak/com.bookie.linux.desktop": "[Desktop Entry]\nIcon=com.bookie.linux\n",
    "apps/linux/flatpak/icons/scalable/com.bookie.linux.svg": "<svg/>",
    "apps/linux/data/com.bookie.linux.gschema.xml": (
        '<schemalist><schema id="com.bookie.linux"/><schema id="com.bookie.linux.Devel"/></schemalist>'
    ),
    "apps/linux/packaging/arch/PKGBUILD": (
        'url="https://example.test/bookie"\n'
        "license=('AGPL-3.0-or-later')\n"
        "depends=('gtk4>=1:4.24' 'glib2>=2.88' 'libadwaita>=1:1.10' 'gtksourceview5>=5.22' 'sqlite')\n"
        "makedepends=('rust>=1:1.98' 'clang')\n"
    ),
    "scripts/build-deb.sh": (
        "cat >control <<EOF\n"
        "Homepage: https://example.test/bookie\n"
        "Depends: libgtk-4-1 (>= 4.24), libadwaita-1-0 (>= 1.10), libgtksourceview-5-0 (>= 5.22), libsecret-1-0\n"
        "EOF\n"
    ),
}
DRIFT = {
    "appstream id": ("apps/linux/flatpak/com.bookie.linux.metainfo.xml", "<id>com.bookie.linux<", "<id>com.bookie.other<"),
    "appstream launchable": ("apps/linux/flatpak/com.bookie.linux.metainfo.xml", "linux.desktop<", "other.desktop<"),
    "meson application id": ("meson.build", "application_id = 'com.bookie.linux'", "application_id = 'com.bookie.other'"),
    "schema id": ("apps/linux/data/com.bookie.linux.gschema.xml", '"com.bookie.linux"', '"com.bookie.other"'),
    "desktop icon": ("apps/linux/flatpak/com.bookie.linux.desktop", "Icon=com.bookie.linux", "Icon=bookie"),
    "meson license": ("meson.build", "license: 'AGPL-3.0-or-later'", "license: 'GPL-3.0-or-later'"),
    "pkgbuild license": ("apps/linux/packaging/arch/PKGBUILD", "license=('AGPL-3.0-or-later')", "license=('MIT')"),
    "appstream license": ("apps/linux/flatpak/com.bookie.linux.metainfo.xml", ">AGPL-3.0-or-later<", ">MIT<"),
    "debian homepage": ("scripts/build-deb.sh", "Homepage: https://example.test/bookie", "Homepage: https://example.test/old"),
    "pkgbuild gtk floor": ("apps/linux/packaging/arch/PKGBUILD", "gtk4>=1:4.24", "gtk4>=1:4.20"),
    "pkgbuild glib floor": ("apps/linux/packaging/arch/PKGBUILD", "glib2>=2.88", "glib2>=2.80"),
    "debian libadwaita floor": ("scripts/build-deb.sh", "libadwaita-1-0 (>= 1.10)", "libadwaita-1-0 (>= 1.6)"),
    "debian gtksourceview floor": ("scripts/build-deb.sh", "libgtksourceview-5-0 (>= 5.22)", "libgtksourceview-5-0 (>= 5.12)"),
    "rust toolchain channel": ("rust-toolchain.toml", 'channel = "1.98"', 'channel = "1.97"'),
    "pkgbuild rust floor": ("apps/linux/packaging/arch/PKGBUILD", "rust>=1:1.98", "rust>=1:1.90"),
}


def write_fixture(root):
    for relative, contents in FILES.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents)


def replace(root, relative, old, new):
    path = root / relative
    contents = path.read_text()
    assert old in contents, old
    path.write_text(contents.replace(old, new))


class PackagingMetadataTests(unittest.TestCase):
    def test_the_repository_packaging_metadata_agrees(self):
        self.assertEqual(METADATA.problems(Path(__file__).resolve().parents[2]), [])

    def test_accepts_matching_metadata_and_unstated_floors(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_fixture(root)
            self.assertEqual(METADATA.problems(root), [])
            replace(root, "apps/linux/packaging/arch/PKGBUILD", "rust>=1:1.98", "rust")
            replace(root, "apps/linux/packaging/arch/PKGBUILD", "gtk4>=1:4.24", "gtk4")
            replace(root, "scripts/build-deb.sh", "libgtk-4-1 (>= 4.24)", "libgtk-4-1")
            self.assertEqual(METADATA.problems(root), [])

    def test_rejects_each_drifting_value(self):
        for target, (relative, old, new) in DRIFT.items():
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                write_fixture(root)
                replace(root, relative, old, new)
                self.assertEqual(len(METADATA.problems(root)), 1)

    def test_rejects_a_renamed_desktop_file_and_a_missing_icon(self):
        for relative in (
            "apps/linux/flatpak/com.bookie.linux.desktop",
            "apps/linux/flatpak/icons/scalable/com.bookie.linux.svg",
        ):
            with self.subTest(path=relative), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                write_fixture(root)
                path = root / relative
                path.rename(path.with_name(f"bookie{path.suffix}"))
                self.assertEqual(len(METADATA.problems(root)), 1)

    def test_rejects_a_schema_id_constant_without_a_schema(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_fixture(root)
            replace(root, "apps/linux/app/src/config.rs", 'SCHEMA_ID: &str = "com.bookie.linux"', 'SCHEMA_ID: &str = "com.bookie.other"')
            self.assertEqual(len(METADATA.problems(root)), 1)

    def test_cli_reports_agreement_drift_and_missing_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_fixture(root)
            with patch.object(METADATA, "ROOT", root), patch("sys.argv", ["check"]):
                with contextlib.redirect_stdout(io.StringIO()) as output:
                    self.assertEqual(METADATA.main(), 0)
                self.assertIn("com.bookie.linux", output.getvalue())
                replace(root, "rust-toolchain.toml", "1.98", "1.97")
                with contextlib.redirect_stderr(io.StringIO()) as errors:
                    self.assertEqual(METADATA.main(), 1)
                self.assertIn("1.97", errors.getvalue())
                (root / "meson.build").unlink()
                with contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(METADATA.main(), 1)


if __name__ == "__main__":
    unittest.main()
