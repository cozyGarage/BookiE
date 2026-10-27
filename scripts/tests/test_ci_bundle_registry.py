import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('registry', Path(__file__).resolve().parents[1] / 'ci-bundle-registry.py')
registry = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(registry)


class RegistryExtractionTests(unittest.TestCase):
    def check_archive(self, name, kind=tarfile.REGTYPE):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'bundle.tar.gz'
            with tarfile.open(archive, 'w:gz') as output:
                entry = tarfile.TarInfo(name)
                entry.type = kind
                entry.linkname = '../outside'
                entry.size = 4 if kind == tarfile.REGTYPE else 0
                output.addfile(entry, io.BytesIO(b'test') if entry.size else None)
            destination = root / 'destination'
            destination.mkdir()
            registry.extract(archive, destination)
            return (destination / name).read_bytes()

    def test_regular_payload_is_extracted(self):
        self.assertEqual(self.check_archive('tests/program'), b'test')

    def test_parent_path_is_rejected(self):
        with self.assertRaises(ValueError):
            self.check_archive('../outside')

    def test_absolute_path_is_rejected(self):
        with self.assertRaises(ValueError):
            self.check_archive('/outside')

    def test_symbolic_links_are_rejected(self):
        with self.assertRaises(ValueError):
            self.check_archive('tests/program', tarfile.SYMTYPE)

    def test_hard_links_are_rejected(self):
        with self.assertRaises(ValueError):
            self.check_archive('tests/program', tarfile.LNKTYPE)


if __name__ == '__main__':
    unittest.main()
