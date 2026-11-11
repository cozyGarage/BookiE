import hashlib
import importlib.util
import io
import json
import os
import platform
from contextlib import redirect_stdout
from unittest.mock import patch
from pathlib import Path
import tarfile
import tempfile
import unittest
from datetime import datetime, timedelta, timezone

SPEC = importlib.util.spec_from_file_location('registry', Path(__file__).resolve().parents[1] / 'ci-bundle-registry.py')
registry = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(registry)


class RegistryExtractionTests(unittest.TestCase):
    def test_prune_deletes_only_expired_bookie_bundles_and_keeps_current_revision(self):
        now = datetime.now(timezone.utc)
        old = (now - timedelta(days=20)).isoformat()
        recent = (now - timedelta(days=2)).isoformat()
        packages = [
            {'type': 'generic', 'name': 'bookie-test-bundles', 'version': 'a' * 40 + '-direct', 'created_at': old},
            {'type': 'generic', 'name': 'bookie-test-bundles', 'version': 'b' * 40 + '-direct', 'created_at': recent},
            {'type': 'generic', 'name': 'bookie-test-bundles', 'version': 'c' * 40 + '-direct', 'created_at': old},
            {'type': 'generic', 'name': 'other-package', 'version': 'd' * 40 + '-direct', 'created_at': old},
            {'type': 'generic', 'name': 'bookie-test-bundles', 'version': 'invalid', 'created_at': old},
        ]

        class Response:
            def __init__(self, status=200, body=b''):
                self.status = status
                self.body = body

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, *_):
                return self.body

        requests = []

        def request(url, method='GET', data=None, size=None):
            requests.append((method, url))
            return Response(200, json.dumps(packages).encode()) if method == 'GET' else Response(204)

        with patch.dict('os.environ', {'BOOKIE_REGISTRY_API_URL': 'https://forgejo.example/api/v1/packages/trung',
                                       'BOOKIE_REGISTRY_TOKEN': 'test-token'}), \
                patch.object(registry, 'request', side_effect=request):
            registry.prune('c' * 40, 14)

        deleted = [url for method, url in requests if method == 'DELETE']
        self.assertEqual(deleted, [
            'https://forgejo.example/api/v1/packages/trung/generic/bookie-test-bundles/' + 'a' * 40 + '-direct'
        ])

    def test_archive_only_bundle_verifies_without_duplicate_executables(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / 'nextest.tar.zst'
            archive.write_bytes(b'nextest archive')
            index = {'schema_version': 1, 'source_commit': 'a' * 40, 'build_group': 'unit',
                     'architecture': platform.machine(), 'archive_only': True,
                     'tests': [{'path': 'tests/not-copied', 'kind': 'lib'}], 'binaries': {},
                     'files': {'nextest.tar.zst': {
                         'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
                         'size': archive.stat().st_size}}}
            (root / 'index.json').write_text(json.dumps(index))
            self.assertTrue(registry.bundle.verify(root, 'a' * 40, 'unit')['archive_only'])

    def test_bundle_verification_checks_the_requested_group(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            executable = root / 'tests' / 'sample'
            executable.parent.mkdir()
            executable.write_bytes(b'test executable')
            index = {'schema_version': 1, 'source_commit': 'a' * 40, 'build_group': 'gpui',
                     'architecture': platform.machine(), 'tests': [{'path': 'tests/sample'}],
                     'binaries': {}, 'files': {'tests/sample': {
                         'sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
                         'size': executable.stat().st_size}}}
            (root / 'index.json').write_text(json.dumps(index))
            self.assertEqual(registry.bundle.verify(root, 'a' * 40, 'gpui')['build_group'], 'gpui')
            with self.assertRaisesRegex(ValueError, 'build group mismatch'):
                registry.bundle.verify(root, 'a' * 40, 'default')

    def test_registry_http_errors_identify_the_failed_request(self):
        error = registry.urllib.error.HTTPError(
            'https://registry.example/bundle/bundle.tar.gz', 500, 'Internal Server Error', {}, None)
        with patch.dict('os.environ', {'BOOKIE_REGISTRY_TOKEN': 'test-token'}), \
                patch.object(registry.urllib.request, 'urlopen', side_effect=error), \
                self.assertRaisesRegex(registry.urllib.error.HTTPError, 'PUT /bundle/bundle.tar.gz returned HTTP 500') as raised:
            registry.request('https://registry.example/bundle/bundle.tar.gz', 'PUT')
        self.assertEqual(raised.exception.code, 500)

    def test_download_progress_is_quiet_until_the_interval_then_reports_rate(self):
        output = io.StringIO()
        with patch.object(registry.time, 'monotonic', return_value=9), redirect_stdout(output):
            last_report = registry.report_download_progress('direct', 1024 ** 2, 4 * 1024 ** 2, 0, 0)
        self.assertEqual(last_report, 0)
        self.assertEqual(output.getvalue(), '')

        with patch.object(registry.time, 'monotonic', return_value=10), redirect_stdout(output):
            last_report = registry.report_download_progress('direct', 1024 ** 2, 4 * 1024 ** 2, 0, last_report)
        self.assertEqual(last_report, 10)
        self.assertIn('bundle download progress: direct 1.0/4.0 MiB (0.1 MiB/s)', output.getvalue())

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

            with patch.object(registry.bundle, 'verify', return_value={'build_group': 'default'}), \
                    patch.object(registry, 'existing_manifest', return_value=None), \
                    patch.object(registry, 'request', side_effect=request), \
                    patch.object(registry, 'UPLOAD_CHUNK_SIZE', 128, create=True):
                registry.publish(root, 'a' * 40, 'default', 'https://registry.example/bundle/')

            manifest = json.loads(uploaded['manifest.json'])
            parts = [name for name in uploaded if name.startswith('bundle.tar.gz.part-')]
            self.assertGreater(len(parts), 1, f'expected chunked upload, got {sorted(uploaded)}')
            self.assertTrue(all(len(uploaded[name]) <= 128 for name in parts))
            self.assertEqual(manifest['schema_version'], 2)
            self.assertEqual(requests[0], ('DELETE', 'bundle'))
            self.assertEqual(requests[-1], ('PUT', 'manifest.json'))
            destination = root / 'destination'
            output = io.StringIO()
            with patch.object(registry.bundle, 'verify', return_value={'build_group': 'default'}), \
                    patch.object(registry, 'request', side_effect=request), redirect_stdout(output):
                registry.fetch(destination, 'a' * 40, 'default', 'https://registry.example/bundle/')
            self.assertEqual((destination / 'program').read_bytes(), bytes(range(256)) * 8)
            self.assertIn('bundle download progress: default', output.getvalue())
            self.assertIn('bundle response reads:', output.getvalue())
            self.assertIn('local write calls:', output.getvalue())
            self.assertIn('bundle receive/write total:', output.getvalue())
            self.assertIn('bundle checksum:', output.getvalue())
            self.assertIn('bundle extract/verify:', output.getvalue())

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
