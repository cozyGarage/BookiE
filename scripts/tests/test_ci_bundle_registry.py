import hashlib
import importlib.util
import io
import json
import os
import platform
from contextlib import redirect_stdout
from pathlib import Path
import tarfile
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('registry', Path(__file__).resolve().parents[1] / 'ci-bundle-registry.py')
registry = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(registry)


class RegistryExtractionTests(unittest.TestCase):
    def test_forgejo_list_reads_all_pages_using_link_and_total_count(self):
        requests = []

        class Response:
            def __init__(self, body, headers=None):
                self.body = body
                self.headers = headers or {}

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, *_):
                return json.dumps(self.body).encode()

        def request(url, **_):
            requests.append(url)
            page = int(registry.urllib.parse.parse_qs(registry.urllib.parse.urlsplit(url).query)['page'][0])
            if page == 1:
                return Response([{'name': str(index)} for index in range(50)], {
                    'Link': '<https://forgejo.example/branches?page=2>; rel="next"',
                    'X-Total-Count': '51',
                })
            return Response([{'name': 'last'}], {'X-Total-Count': '51'})

        with patch.object(registry, 'request', side_effect=request):
            branches = registry.forgejo_list('https://forgejo.example/api', 'branches', {}, 'repo-token')

        self.assertEqual(len(branches), 51)
        self.assertIn('page=2', requests[1])

    def test_forgejo_list_rejects_invalid_total_count(self):
        class Response:
            headers = {'X-Total-Count': 'invalid'}

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, *_):
                return b'[]'

        with patch.object(registry, 'request', return_value=Response()):
            with self.assertRaisesRegex(ValueError, 'invalid branches reference count'):
                registry.forgejo_list('https://forgejo.example/api', 'branches', {}, 'repo-token')

    def test_discover_protected_revisions_from_branches_open_prs_and_active_runs(self):
        responses = {
            '/branches?limit=50&page=1': [{'commit': {'id': 'a' * 40}}],
            '/pulls?limit=50&page=1&state=open': [{'state': 'open', 'head': {'sha': 'b' * 40}}],
            '/actions/runs?limit=50&page=1&status=waiting&status=running': {
                'total_count': 1, 'workflow_runs': [{'status': 'running', 'commit_sha': 'c' * 40}]
            },
        }
        requests = []

        class Response:
            def __init__(self, body):
                self.body = body

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, *_):
                return json.dumps(self.body).encode()

        def request(url, method='GET', data=None, size=None, token=None):
            requests.append((url, token))
            parsed = registry.urllib.parse.urlsplit(url)
            key = parsed.path.removeprefix('/api/v1/repos/trung/bookie') + '?' + parsed.query
            return Response(responses[key])

        with patch.dict('os.environ', {
                'BOOKIE_FORGEJO_API_URL': 'https://forgejo.example/api/v1/repos/trung/bookie',
                'BOOKIE_FORGEJO_TOKEN': 'repo-token'}), patch.object(registry, 'request', side_effect=request):
            revisions = registry.discover_protected_revisions()

        self.assertEqual(revisions, {'a' * 40, 'b' * 40, 'c' * 40})
        self.assertEqual({token for _, token in requests}, {'repo-token'})

    def test_discovery_ignores_closed_pull_requests_and_finished_runs(self):
        responses = {
            '/branches?limit=50&page=1': [],
            '/pulls?limit=50&page=1&state=open': [{'state': 'closed', 'head': {'sha': 'b' * 40}}],
            '/actions/runs?limit=50&page=1&status=waiting&status=running': {
                'total_count': 1, 'workflow_runs': [{'status': 'completed', 'commit_sha': 'c' * 40}]
            },
        }

        class Response:
            def __init__(self, body):
                self.body = body
                self.headers = {}

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, *_):
                return json.dumps(self.body).encode()

        def request(url, **_):
            parsed = registry.urllib.parse.urlsplit(url)
            key = parsed.path.removeprefix('/api/v1/repos/trung/bookie') + '?' + parsed.query
            return Response(responses[key])

        with patch.dict('os.environ', {
                'BOOKIE_FORGEJO_API_URL': 'https://forgejo.example/api/v1/repos/trung/bookie',
                'BOOKIE_FORGEJO_TOKEN': 'repo-token'}), patch.object(registry, 'request', side_effect=request):
            revisions = registry.discover_protected_revisions()

        self.assertEqual(revisions, set())

    def test_discovery_rejects_invalid_branch_revision(self):
        responses = {
            '/branches?limit=50&page=1': [{'commit': {'id': 'not-a-commit'}}],
            '/pulls?limit=50&page=1&state=open': [],
            '/actions/runs?limit=50&page=1&status=waiting&status=running': {
                'total_count': 0, 'workflow_runs': []
            },
        }

        class Response:
            def __init__(self, body):
                self.body = body
                self.headers = {}

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, *_):
                return json.dumps(self.body).encode()

        def request(url, **_):
            parsed = registry.urllib.parse.urlsplit(url)
            key = parsed.path.removeprefix('/api/v1/repos/trung/bookie') + '?' + parsed.query
            return Response(responses[key])

        with patch.dict('os.environ', {
                'BOOKIE_FORGEJO_API_URL': 'https://forgejo.example/api/v1/repos/trung/bookie',
                'BOOKIE_FORGEJO_TOKEN': 'repo-token'}), patch.object(registry, 'request', side_effect=request):
            with self.assertRaisesRegex(ValueError, 'invalid protected source revision'):
                registry.discover_protected_revisions()

    def test_discovery_rejects_incomplete_action_run_revisions(self):
        class Response:
            def __init__(self, body):
                self.body = body

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, *_):
                return json.dumps(self.body).encode()

        def request(url, **_):
            return Response({'total_count': 1} if '/actions/runs?' in url else [])

        with patch.dict('os.environ', {
                'BOOKIE_FORGEJO_API_URL': 'https://forgejo.example/api/v1/repos/trung/bookie',
                'BOOKIE_FORGEJO_TOKEN': 'repo-token'}), \
                patch.object(registry, 'request', side_effect=request):
            with self.assertRaisesRegex(ValueError, 'incomplete actions/runs reference inventory'):
                registry.discover_protected_revisions()

    def test_prune_dry_run_lists_expired_bundles_without_deleting(self):
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

        output = io.StringIO()
        with patch.dict('os.environ', {'BOOKIE_REGISTRY_API_URL': 'https://forgejo.example/api/v1/packages/trung',
                                       'BOOKIE_REGISTRY_TOKEN': 'test-token'}), \
                patch.object(registry, 'request', side_effect=request), redirect_stdout(output):
            registry.prune('c' * 40, 14, set(), dry_run=True)

        deleted = [url for method, url in requests if method == 'DELETE']
        self.assertEqual(deleted, [])
        self.assertIn('expired candidate ' + 'a' * 40 + '-direct', output.getvalue())
        self.assertNotIn('expired candidate ' + 'c' * 40, output.getvalue())

    def test_prune_does_not_delete_old_protected_revisions(self):
        old = (datetime.now(timezone.utc) - timedelta(days=20)).isoformat()
        packages = [
            {'type': 'generic', 'name': 'bookie-test-bundles', 'version': 'a' * 40 + '-direct', 'created_at': old},
            {'type': 'generic', 'name': 'bookie-test-bundles', 'version': 'b' * 40 + '-direct', 'created_at': old},
        ]
        requests = []

        class Response:
            def __init__(self, status=200):
                self.status = status

            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, *_):
                return json.dumps(packages).encode()

        def request(url, method='GET', data=None, size=None):
            requests.append((method, url))
            return Response(204 if method == 'DELETE' else 200)

        with patch.dict('os.environ', {'BOOKIE_REGISTRY_API_URL': 'https://forgejo.example/api/v1/packages/trung',
                                       'BOOKIE_REGISTRY_TOKEN': 'test-token'}), \
                patch.object(registry, 'request', side_effect=request):
            registry.prune('c' * 40, 14, {'b' * 40}, dry_run=False, references_complete=True)

        deleted = [url for method, url in requests if method == 'DELETE']
        self.assertEqual(deleted, [
            'https://forgejo.example/api/v1/packages/trung/generic/bookie-test-bundles/' + 'a' * 40 + '-direct'
        ])

    def test_prune_delete_requires_an_explicit_protected_revision(self):
        with self.assertRaisesRegex(ValueError, 'protected revisions and complete reference discovery'):
            registry.prune('c' * 40, 14, set(), dry_run=False)

    def test_prune_delete_requires_complete_reference_discovery(self):
        with self.assertRaisesRegex(ValueError, 'protected revisions and complete reference discovery'):
            registry.prune('c' * 40, 14, {'b' * 40}, dry_run=False)

    def test_prune_reads_all_pages_before_deleting_versions(self):
        recent = (datetime.now(timezone.utc) - timedelta(days=2)).isoformat()
        old = (datetime.now(timezone.utc) - timedelta(days=20)).isoformat()
        packages = [
            {'type': 'generic', 'name': 'bookie-test-bundles', 'version': f'{number:040x}-direct',
             'created_at': recent}
            for number in range(50)
        ] + [{'type': 'generic', 'name': 'bookie-test-bundles', 'version': 'a' * 40 + '-direct',
              'created_at': old}]
        requests = []

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

        def request(url, method='GET', data=None, size=None):
            requests.append((method, url))
            if method == 'DELETE':
                return Response(204)
            page = int(registry.urllib.parse.parse_qs(registry.urllib.parse.urlsplit(url).query)['page'][0])
            selection = packages[(page - 1) * 50:page * 50]
            return Response(204) if method == 'DELETE' else Response(body=json.dumps(selection).encode())

        with patch.dict('os.environ', {'BOOKIE_REGISTRY_API_URL': 'https://forgejo.example/api/v1/packages/trung',
                                       'BOOKIE_REGISTRY_TOKEN': 'test-token'}), \
                patch.object(registry, 'request', side_effect=request):
            registry.prune('c' * 40, 14, {'d' * 40}, dry_run=False, references_complete=True)

        self.assertEqual([method for method, _ in requests[:2]], ['GET', 'GET'])
        self.assertEqual([method for method, _ in requests[2:]], ['DELETE'])
        self.assertTrue(requests[2][1].endswith('/' + 'a' * 40 + '-direct'))

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
                patch.object(registry.urllib.request, 'build_opener') as opener, \
                self.assertRaisesRegex(registry.urllib.error.HTTPError, 'PUT /bundle/bundle.tar.gz returned HTTP 500') as raised:
            opener.return_value.open.side_effect = error
            registry.request('https://registry.example/bundle/bundle.tar.gz', 'PUT')
        self.assertEqual(raised.exception.code, 500)

    def test_registry_redirect_drops_credentials_on_origin_change(self):
        request = registry.urllib.request.Request(
            'https://forgejo.example/api', headers={'Authorization': 'token test-token'})
        redirect = registry.ArtifactRedirect()
        same_origin = redirect.redirect_request(request, None, 302, 'Found', {}, 'https://forgejo.example/next')
        downgrade = redirect.redirect_request(request, None, 302, 'Found', {}, 'http://forgejo.example/next')
        other_host = redirect.redirect_request(request, None, 302, 'Found', {}, 'https://other.example/next')
        self.assertEqual(same_origin.get_header('Authorization'), 'token test-token')
        self.assertIsNone(downgrade.get_header('Authorization'))
        self.assertIsNone(other_host.get_header('Authorization'))

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
                patch.object(registry.urllib.request, 'build_opener') as opener:
            opener.return_value.open.side_effect = error
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
