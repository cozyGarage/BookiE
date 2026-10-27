import importlib.util
import io
import json
import os
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
                self.assertRaisesRegex(registry.urllib.error.HTTPError, 'PUT /bundle/bundle.tar.gz returned HTTP 500') as raised:
            registry.request('https://registry.example/bundle/bundle.tar.gz', 'PUT')
        self.assertEqual(raised.exception.code, 500)

    def test_missing_registry_manifest_remains_a_cache_miss(self):
        error = registry.urllib.error.HTTPError('https://registry.example/bundle/manifest.json', 404, 'Not Found', {}, None)
        with patch.dict('os.environ', {'BOOKIE_REGISTRY_TOKEN': 'test-token'}), \
                patch.object(registry.urllib.request, 'urlopen', side_effect=error):
            self.assertIsNone(registry.existing_manifest('https://registry.example/bundle/', 'a' * 40, 'default'))

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

    def test_fetch_reads_legacy_single_file_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'source'
            source.mkdir()
            (source / 'index.json').write_text('{}\n')
            (source / 'program').write_bytes(b'legacy bundle')
            archive = root / 'bundle.tar.gz'
            with tarfile.open(archive, 'w:gz') as output:
                output.add(source / 'index.json', arcname='index.json')
                output.add(source / 'program', arcname='program')
            manifest = {'schema_version': 1, 'source_commit': 'a' * 40, 'build_group': 'default',
                        'sha256': registry.bundle.digest(archive), 'size': archive.stat().st_size,
                        'index_sha256': registry.bundle.digest(source / 'index.json')}
            files = {'manifest.json': json.dumps(manifest).encode(), 'bundle.tar.gz': archive.read_bytes()}

            def request(url, method='GET', data=None, size=None):
                return io.BytesIO(files[url.rsplit('/', 1)[-1]])

            destination = root / 'downloaded'
            with patch.object(registry, 'request', side_effect=request), \
                    patch.object(registry.bundle, 'verify', return_value={'build_group': 'default'}):
                registry.fetch(destination, 'a' * 40, 'default', 'https://registry.example/bundle/')
            self.assertEqual((destination / 'program').read_bytes(), b'legacy bundle')

    def test_fetch_refreshes_the_recency_of_a_verified_cached_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / 'cached'
            destination.mkdir()
            os.utime(destination, (1, 1))
            with patch.object(registry.bundle, 'verify', return_value={'build_group': 'default'}):
                registry.fetch(destination, 'a' * 40, 'default', 'https://registry.example/bundle/')
            self.assertGreater(destination.stat().st_mtime, 1)

    def test_v2_manifest_rejects_untrusted_chunk_names(self):
        manifest = {'schema_version': 2, 'source_commit': 'a' * 40, 'build_group': 'default', 'size': 1,
                    'sha256': 'b' * 64, 'index_sha256': 'c' * 64,
                    'chunks': [{'name': '../manifest.json', 'size': 1, 'sha256': 'd' * 64}]}
        with self.assertRaisesRegex(ValueError, 'registry bundle chunks are invalid'):
            registry.verify_manifest(manifest, 'a' * 40, 'default')

    def test_publish_splits_and_fetch_reassembles_archive(self):
        class Response:
            def __init__(self, status=201):
                self.status = status

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'index.json').write_text('{}\n')
            (root / 'program').write_bytes(bytes(range(256)) * 8)
            uploaded = {}
            requests = []

            def request(url, method='GET', data=None, size=None):
                name = url.rsplit('/', 1)[-1]
                requests.append((method, name))
                if method == 'DELETE':
                    return Response(204)
                if method == 'PUT':
                    uploaded[name] = data if isinstance(data, bytes) else data.read()
                    self.assertEqual(size, len(uploaded[name]))
                    return Response()
                return io.BytesIO(uploaded[name])

            with patch.object(registry.bundle, 'verify', return_value={'build_group': 'duckdb'}), \
                    patch.object(registry, 'existing_manifest', return_value=None), \
                    patch.object(registry, 'request', side_effect=request), \
                    patch.object(registry, 'UPLOAD_CHUNK_SIZE', 128, create=True):
                registry.publish(root, 'a' * 40, 'duckdb', 'https://registry.example/bundle/')

            manifest = json.loads(uploaded['manifest.json'])
            parts = [name for name in uploaded if name.startswith('bundle.tar.gz.part-')]
            self.assertGreater(len(parts), 1, f'expected chunked upload, got {sorted(uploaded)}')
            self.assertTrue(all(len(uploaded[name]) <= 128 for name in parts))
            self.assertEqual(manifest['schema_version'], 2)
            self.assertEqual(requests[0], ('DELETE', 'bundle'))
            self.assertEqual(requests[-1], ('PUT', 'manifest.json'))
            destination = root / 'destination'
            with patch.object(registry.bundle, 'verify', return_value={'build_group': 'duckdb'}), \
                    patch.object(registry, 'request', side_effect=request):
                registry.fetch(destination, 'a' * 40, 'duckdb', 'https://registry.example/bundle/')
            self.assertEqual((destination / 'program').read_bytes(), bytes(range(256)) * 8)

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
