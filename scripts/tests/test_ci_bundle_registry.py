import importlib.util
import io
import json
from unittest.mock import patch
from pathlib import Path
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('registry', Path(__file__).resolve().parents[1] / 'ci-bundle-registry.py')
registry = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(registry)


class RegistryExtractionTests(unittest.TestCase):
    def test_registry_http_errors_identify_the_failed_request(self):
        error = registry.urllib.error.HTTPError(
            'https://registry.example/bundle/bundle.tar.gz', 500, 'Internal Server Error', {}, None)
        with patch.dict('os.environ', {'BOOKIE_REGISTRY_TOKEN': 'test-token'}), \
                patch.object(registry.urllib.request, 'urlopen', side_effect=error), \
                self.assertRaisesRegex(ValueError, 'PUT /bundle/bundle.tar.gz returned HTTP 500'):
            registry.request('https://registry.example/bundle/bundle.tar.gz', 'PUT')

    def check_archive(self, name, kind=tarfile.REGTYPE):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'bundle.tar'
            with tarfile.open(archive, 'w') as output:
                entry = tarfile.TarInfo(name)
                entry.type = kind
                entry.linkname = '../outside'
                entry.size = 4 if kind == tarfile.REGTYPE else 0
                output.addfile(entry, io.BytesIO(b'test') if entry.size else None)
            destination = root / 'destination'
            destination.mkdir()
            registry.extract(archive, destination)
            return (destination / name).read_bytes()

    def test_github_download_redirect_does_not_forward_the_access_token(self):
        request = registry.urllib.request.Request('https://api.github.com/artifact', headers={'Authorization': 'Bearer private'})
        redirected = registry.ArtifactRedirect().redirect_request(
            request, None, 302, 'Found', {}, 'https://storage.example/artifact.zip')
        self.assertIsNone(redirected.get_header('Authorization'))

    def test_republishing_an_existing_source_bundle_does_not_overwrite_it(self):
        manifest = {'schema_version': 1, 'source_commit': 'a' * 40, 'build_group': 'default',
                    'sha256': 'b' * 64, 'size': 100, 'index_sha256': 'c' * 64}
        with patch.object(registry.bundle, 'verify', return_value={'build_group': 'default'}), \
                patch.object(registry, 'request', return_value=io.BytesIO(json.dumps(manifest).encode())) as request:
            registry.publish(Path('/unused'), 'a' * 40, 'default', 'https://registry.example/bundle/')
        request.assert_called_once_with('https://registry.example/bundle/manifest.json')

    def test_publish_compresses_bundle_before_upload(self):
        class Response:
            status = 201

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'index.json').write_text('{}\n')
            (root / 'program').write_bytes(b'test bundle')
            uploaded = {}

            def request(url, method='GET', data=None, size=None):
                if method == 'PUT' and url.endswith('bundle.tar.gz'):
                    uploaded['archive'] = data.read()
                    self.assertEqual(size, len(uploaded['archive']))
                return Response()

            with patch.object(registry.bundle, 'verify', return_value={'build_group': 'duckdb'}), \
                    patch.object(registry, 'existing_manifest', return_value=None), \
                    patch.object(registry, 'request', side_effect=request):
                registry.publish(root, 'a' * 40, 'duckdb', 'https://registry.example/bundle/')

            self.assertEqual(uploaded['archive'][:2], b'\x1f\x8b')
            archive = root / 'uploaded.tar.gz'
            archive.write_bytes(uploaded['archive'])
            destination = root / 'destination'
            destination.mkdir()
            registry.extract(archive, destination)
            self.assertEqual((destination / 'program').read_bytes(), b'test bundle')

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
