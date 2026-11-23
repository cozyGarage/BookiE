#!/usr/bin/env python3
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import urllib.parse
from datetime import datetime, timedelta, timezone

MAX_ARCHIVE = 8 * 1024 ** 3
MAX_EXTRACTED = 24 * 1024 ** 3
UPLOAD_CHUNK_SIZE = 64 * 1024 ** 2
PROGRESS_INTERVAL_SECONDS = 10
# shortcut: the two-day lookback covers the 90-minute job limit plus queue recovery; revisit if either grows.
ACTION_RUN_LOOKBACK_DAYS = 2
TERMINAL_JOB_STATUSES = {'success', 'failure', 'cancelled', 'skipped'}
BUNDLE_GROUPS = {'default', 'direct', 'unit', 'sandbox', 'gpui', 'workspace'} | set(
    json.loads((Path(__file__).parent / 'ci-test-bundle-groups.json').read_text())['groups']
)
SPEC = importlib.util.spec_from_file_location('bundle', Path(__file__).with_name('ci-test-bundle.py'))
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)


def request(url, method='GET', data=None, size=None, token=None):
    token = token if token is not None else os.environ['BOOKIE_REGISTRY_TOKEN']
    headers = {'Authorization': 'token ' + token}
    if size is not None:
        headers.update({'Content-Type': 'application/octet-stream', 'Content-Length': str(size)})
    for attempt in range(3 if method == 'GET' else 1):
        try:
            return urllib.request.build_opener(ArtifactRedirect()).open(
                urllib.request.Request(url, data=data, method=method, headers=headers), timeout=180)
        except urllib.error.HTTPError as error:
            if method == 'GET' and error.code in (429, 500, 502, 503, 504) and attempt < 2:
                error.close()
                time.sleep(2 ** attempt)
                continue
            path = urllib.parse.urlsplit(url).path
            error.msg = f'registry {method} {path} returned HTTP {error.code}'
            error.close()
            raise
        except (urllib.error.URLError, TimeoutError, ConnectionError):
            if method != 'GET' or attempt == 2:
                raise
            time.sleep(2 ** attempt)


def upload_bytes(url, data):
    for attempt in range(3):
        try:
            with request(url, 'PUT', data, len(data)) as response:
                if response.status != 201:
                    raise ValueError('registry did not accept a bundle file')
                return
        except urllib.error.HTTPError as error:
            if error.code not in (409, 429, 500, 502, 503, 504):
                raise
            failure = error
        except (urllib.error.URLError, TimeoutError, ConnectionError) as error:
            failure = error
        try:
            with request(url) as response:
                received = response.read(len(data) + 1)
            if received == data:
                return
            raise ValueError('registry already contains different bundle bytes')
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise
        if attempt == 2:
            raise failure
        time.sleep(2 ** attempt)



def extract(archive, destination):
    with tarfile.open(archive, 'r:*') as source:
        members = source.getmembers()
        if len(members) > 5000 or sum(member.size for member in members) > MAX_EXTRACTED:
            raise ValueError('test bundle exceeds extraction limits')
        for member in members:
            if not (member.isfile() or member.isdir()):
                raise ValueError('test bundle contains a link or special file')
            if not (destination / member.name).resolve().is_relative_to(destination.resolve()):
                raise ValueError('test bundle path escapes its directory')
        source.extractall(destination, members=members, filter='data')


def existing_manifest(url, revision, group):
    try:
        with request(url + 'manifest.json') as response:
            manifest = json.loads(response.read(65537))
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise
    verify_manifest(manifest, revision, group)
    return manifest


def verify_manifest(manifest, revision, group):
    if (not isinstance(manifest, dict) or type(manifest.get('schema_version')) is not int
            or manifest.get('schema_version') not in (1, 2) or manifest.get('source_commit') != revision
            or manifest.get('build_group') != group or type(manifest.get('size')) is not int
            or not 0 < manifest['size'] <= MAX_ARCHIVE
            or not re.fullmatch('[0-9a-f]{64}', manifest.get('sha256', ''))
            or not re.fullmatch('[0-9a-f]{64}', manifest.get('index_sha256', ''))):
        raise ValueError('registry bundle identity or size is invalid')
    if manifest['schema_version'] == 1:
        return
    chunks = manifest.get('chunks')
    if not isinstance(chunks, list) or not chunks or len(chunks) > MAX_ARCHIVE // UPLOAD_CHUNK_SIZE + 1:
        raise ValueError('registry bundle chunks are invalid')
    if any(not isinstance(chunk, dict) or chunk.get('name') != f'bundle.tar.gz.part-{number:04d}'
           or type(chunk.get('size')) is not int or not 0 < chunk['size'] <= UPLOAD_CHUNK_SIZE
           or not re.fullmatch('[0-9a-f]{64}', chunk.get('sha256', ''))
           for number, chunk in enumerate(chunks)):
        raise ValueError('registry bundle chunks are invalid')
    if sum(chunk['size'] for chunk in chunks) != manifest['size']:
        raise ValueError('registry bundle chunk sizes do not match the archive')


def report_download_progress(group, downloaded, total, started, last_report):
    now = time.monotonic()
    if downloaded < total and now - last_report < PROGRESS_INTERVAL_SECONDS:
        return last_report
    elapsed = now - started
    rate = downloaded / 1024 ** 2 / elapsed if elapsed else 0
    print(f"bundle download progress: {group} {downloaded / 1024 ** 2:.1f}/"
          f"{total / 1024 ** 2:.1f} MiB ({rate:.1f} MiB/s)", flush=True)
    return now


def publish(directory, revision, group, url):
    index = bundle.verify(directory, revision)
    if index['build_group'] != group:
        raise ValueError('test bundle build group mismatch')
    existing = existing_manifest(url, revision, group)
    if existing:
        print(json.dumps(existing, sort_keys=True))
        return
    with tempfile.TemporaryDirectory() as temporary:
        archive = Path(temporary) / 'bundle.tar.gz'
        with tarfile.open(archive, 'w:gz', compresslevel=1) as output:
            for path in sorted(directory.rglob('*')):
                if path.is_symlink():
                    raise ValueError('cannot publish a bundle containing links')
                output.add(path, arcname=str(path.relative_to(directory)), recursive=False)
        size = archive.stat().st_size
        if size > MAX_ARCHIVE:
            raise ValueError('test bundle exceeds upload size limit')
        try:
            with request(url.rstrip('/'), 'DELETE'):
                pass
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise
        manifest = {'schema_version': 2, 'source_commit': revision, 'build_group': group,
                    'sha256': bundle.digest(archive), 'size': size,
                    'index_sha256': bundle.digest(directory / 'index.json'), 'chunks': []}
        with archive.open('rb') as content:
            number = 0
            while chunk := content.read(UPLOAD_CHUNK_SIZE):
                name = f'bundle.tar.gz.part-{number:04d}'
                chunk_sha256 = hashlib.sha256(chunk).hexdigest()
                upload_bytes(url + name, chunk)
                manifest['chunks'].append({'name': name, 'size': len(chunk), 'sha256': chunk_sha256})
                number += 1
        data = json.dumps(manifest, sort_keys=True).encode()
        upload_bytes(url + 'manifest.json', data)
        print(json.dumps(manifest, sort_keys=True))


def fetch(directory, revision, group, url):
    if directory.exists():
        index = bundle.verify(directory, revision)
        if index['build_group'] != group:
            raise ValueError('cached test bundle build group mismatch')
        os.utime(directory, None)
        return
    with request(url + 'manifest.json') as response:
        manifest = json.loads(response.read(65537))
    verify_manifest(manifest, revision, group)
    print(f"downloading {manifest['size'] / 1024**3:.2f} GiB {group} test bundle", flush=True)
    directory.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=directory.parent) as temporary:
        temporary = Path(temporary)
        archive = temporary / 'bundle.tar.gz'
        parts = manifest.get('chunks', [{'name': 'bundle.tar.gz', 'size': manifest['size'], 'sha256': manifest['sha256']}])
        transfer_started = time.monotonic()
        last_progress = transfer_started
        downloaded = 0
        response_seconds = 0.0
        write_seconds = 0.0
        with archive.open('wb') as output:
            for part in parts:
                digest = hashlib.sha256()
                with request(url + part['name']) as response:
                    remaining = part['size']
                    while remaining:
                        read_started = time.monotonic()
                        block = response.read(min(1024 * 1024, remaining))
                        response_seconds += time.monotonic() - read_started
                        if not block:
                            raise ValueError('registry returned a truncated bundle chunk')
                        write_started = time.monotonic()
                        output.write(block)
                        write_seconds += time.monotonic() - write_started
                        digest.update(block)
                        remaining -= len(block)
                        downloaded += len(block)
                        last_progress = report_download_progress(
                            group, downloaded, manifest['size'], transfer_started, last_progress)
                    if response.read(1):
                        raise ValueError('registry bundle chunk exceeds its declared size')
                if digest.hexdigest() != part['sha256']:
                    raise ValueError('registry bundle chunk digest mismatch')
        transfer_seconds = time.monotonic() - transfer_started
        transfer_mib = manifest['size'] / 1024**2
        transfer_rate = transfer_mib / transfer_seconds if transfer_seconds else 0
        print(f"bundle response reads: {response_seconds:.1f}s; local write calls: {write_seconds:.1f}s", flush=True)
        print(f"bundle receive/write total: {transfer_mib:.1f} MiB in {transfer_seconds:.1f}s ({transfer_rate:.1f} MiB/s)", flush=True)
        checksum_started = time.monotonic()
        if bundle.digest(archive) != manifest['sha256']:
            raise ValueError('registry bundle digest mismatch')
        print(f"bundle checksum: {time.monotonic() - checksum_started:.1f}s", flush=True)
        staging = temporary / 'verified'
        staging.mkdir()
        extract_started = time.monotonic()
        extract(archive, staging)
        if bundle.digest(staging / 'index.json') != manifest['index_sha256']:
            raise ValueError('registry bundle index digest mismatch')
        index = bundle.verify(staging, revision)
        if index['build_group'] != group:
            raise ValueError('registry bundle build group mismatch')
        print(f"bundle extract/verify: {time.monotonic() - extract_started:.1f}s", flush=True)
        shutil.move(staging, directory)
        os.utime(directory, None)
    print('test bundle ready', flush=True)


class ArtifactRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, stream, code, message, headers, url):
        result = super().redirect_request(request, stream, code, message, headers, url)
        if result is not None:
            source = urllib.parse.urlsplit(request.full_url)
            target = urllib.parse.urlsplit(url)
            if (target.scheme, target.netloc) != (source.scheme, source.netloc):
                result.remove_header('Authorization')
        return result


class UnstableReferenceInventory(ValueError):
    pass


def forgejo_list(api, endpoint, parameters, token, key=None):
    for attempt in range(3):
        try:
            return forgejo_list_snapshot(api, endpoint, parameters, token, key)
        except UnstableReferenceInventory:
            if attempt == 2:
                raise
            time.sleep(0.25 * (attempt + 1))


def forgejo_list_snapshot(api, endpoint, parameters, token, key):
    items = []
    page = 1
    while True:
        query = urllib.parse.urlencode({'limit': 50, 'page': page, **parameters}, doseq=True)
        with request(f'{api}/{endpoint}?{query}', token=token) as response:
            data = json.load(response)
            headers = getattr(response, 'headers', {})
        page_items = data if key is None else data.get(key) if isinstance(data, dict) else None
        if not isinstance(page_items, list):
            raise ValueError(f'Forgejo returned an incomplete {endpoint} reference inventory')
        items.extend(page_items)
        total = data.get('total_count') if isinstance(data, dict) else None
        if key is not None and type(total) is not int:
            raise ValueError(f'Forgejo returned an incomplete {endpoint} reference inventory')
        if total is None and hasattr(headers, 'get') and headers.get('X-Total-Count'):
            try:
                total = int(headers['X-Total-Count'])
            except ValueError as error:
                raise ValueError(f'Forgejo returned an invalid {endpoint} reference count') from error
        links = headers.get('Link', '') if hasattr(headers, 'get') else ''
        next_page = 'rel="next"' in links
        if type(total) is int:
            if len(items) > total:
                raise UnstableReferenceInventory(
                    f'Forgejo returned an inconsistent {endpoint} reference count')
            if next_page:
                page += 1
                continue
            if links:
                if not page_items and len(items) < total:
                    raise UnstableReferenceInventory(
                        f'Forgejo returned an incomplete {endpoint} reference inventory')
                return items
            if len(items) == total:
                return items
            if not page_items:
                raise UnstableReferenceInventory(
                    f'Forgejo returned an incomplete {endpoint} reference inventory')
        elif not next_page and len(page_items) < 50:
            return items
        page += 1


def discover_protected_revisions():
    api = os.environ['BOOKIE_FORGEJO_API_URL'].rstrip('/')
    token = os.environ['BOOKIE_FORGEJO_TOKEN']
    revisions = set()
    branches = forgejo_list(api, 'branches', {}, token)
    pulls = forgejo_list(api, 'pulls', {'state': 'open'}, token)
    runs = forgejo_list(api, 'actions/runs', {'status': ['waiting', 'running']}, token, 'workflow_runs')
    runs.extend(forgejo_list(
        api, 'actions/runs', {'status': ['failure', 'cancelled']}, token, 'workflow_runs'))
    for branch in branches:
        revisions.add(branch['commit']['id'])
    for pull in pulls:
        if pull['state'] == 'open':
            revisions.add(pull['head']['sha'])
    for run in runs:
        status = run['status']
        if status in ('waiting', 'running'):
            revisions.add(run['commit_sha'])
            continue
        if status not in ('failure', 'cancelled'):
            continue
        created = run.get('created')
        if not isinstance(created, str):
            raise ValueError('Forgejo returned an incomplete actions/runs reference inventory')
        try:
            created_at = datetime.fromisoformat(created.replace('Z', '+00:00'))
        except ValueError as error:
            raise ValueError('Forgejo returned an invalid actions/runs creation time') from error
        if created_at.tzinfo is None:
            raise ValueError('Forgejo returned an invalid actions/runs creation time')
        if created_at < datetime.now(timezone.utc) - timedelta(days=ACTION_RUN_LOOKBACK_DAYS):
            continue
        run_id = run.get('id')
        if type(run_id) is not int:
            raise ValueError('Forgejo returned an incomplete actions/runs reference inventory')
        with request(f'{api}/actions/runs/{run_id}/jobs', token=token) as response:
            jobs = json.load(response)
        if not isinstance(jobs, list):
            raise ValueError('Forgejo returned an incomplete actions/jobs reference inventory')
        if any(not isinstance(job, dict) or job.get('status') not in TERMINAL_JOB_STATUSES for job in jobs):
            revisions.add(run['commit_sha'])
    if any(not isinstance(revision, str) or not re.fullmatch(r'[0-9a-f]{40}', revision)
           for revision in revisions):
        raise ValueError('Forgejo returned an invalid protected source revision')
    return revisions


def prune(revision, retention_days, protected_revisions, dry_run=True, references_complete=False):
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('a full source commit is required')
    if not 1 <= retention_days <= 90:
        raise ValueError('package retention must be between 1 and 90 days')
    if any(not re.fullmatch(r'[0-9a-f]{40}', item) for item in protected_revisions):
        raise ValueError('protected revisions must be full source commits')
    if not dry_run and (not protected_revisions or not references_complete):
        raise ValueError('deletion requires protected revisions and complete reference discovery')
    api = os.environ['BOOKIE_REGISTRY_API_URL'].rstrip('/')
    package_name = 'bookie-test-bundles'
    protected_revisions = set(protected_revisions) | {revision}
    cutoff = datetime.now(timezone.utc) - timedelta(days=retention_days)
    page = 1
    packages = []
    while True:
        query = urllib.parse.urlencode({'type': 'generic', 'q': package_name, 'limit': 50, 'page': page})
        with request(f'{api}?{query}') as response:
            page_packages = json.load(response)
            headers = getattr(response, 'headers', {})
        if not isinstance(page_packages, list):
            raise ValueError('registry returned an incomplete bundle inventory')
        packages.extend(page_packages)
        next_page = 'rel="next"' in headers.get('Link', '') if hasattr(headers, 'get') else False
        total = headers.get('X-Total-Count') if hasattr(headers, 'get') else None
        if total is not None:
            try:
                total = int(total)
            except ValueError as error:
                raise ValueError('registry returned an invalid bundle inventory count') from error
            if len(packages) > total:
                raise ValueError('registry returned an inconsistent bundle inventory count')
            if len(packages) >= total:
                break
            if not page_packages:
                raise ValueError('registry returned an incomplete bundle inventory')
        elif not next_page and len(page_packages) < 50:
            break
        page += 1
    expired = []
    for package in packages:
        version = package.get('version', '')
        created = package.get('created_at', '')
        if (package.get('type') != 'generic' or package.get('name') != package_name
                or not re.fullmatch(r'[0-9a-f]{40}-[a-z0-9-]+', version)
                or version.startswith(revision + '-') or not created):
            continue
        try:
            created_at = datetime.fromisoformat(created.replace('Z', '+00:00'))
        except ValueError:
            continue
        if created_at.tzinfo is None or created_at >= cutoff or version.split('-', 1)[0] in protected_revisions:
            continue
        expired.append((version, created))
    for version, created in expired:
        if dry_run:
            print(f'expired candidate {version} created {created}', flush=True)
            continue
        path = '/'.join(urllib.parse.quote(segment, safe='') for segment in ('generic', package_name, version))
        with request(f'{api}/{path}', 'DELETE') as response:
            if response.status != 204:
                raise ValueError('registry did not delete an expired test bundle')
    action = 'would remove' if dry_run else 'removed'
    print(f'{action} {len(expired)} expired test bundle versions; retained {retention_days} days', flush=True)


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest='operation', required=True)
    for operation in ('publish', 'fetch'):
        command = commands.add_parser(operation)
        command.add_argument('directory', type=Path)
        command.add_argument('revision')
        command.add_argument('group', choices=sorted(BUNDLE_GROUPS))
    prune_command = commands.add_parser('prune')
    prune_command.add_argument('revision')
    prune_command.add_argument('--retention-days', type=int, default=14)
    prune_command.add_argument('--discover-references', action='store_true')
    prune_command.add_argument('--delete', action='store_true')
    prune_command.add_argument('--dry-run', action='store_true')
    args = parser.parse_args()
    if args.operation == 'prune':
        if args.delete and args.dry_run:
            parser.error('--delete and --dry-run cannot be combined')
        protected = discover_protected_revisions() if args.discover_references else set()
        if args.delete and not args.discover_references:
            parser.error('--delete requires --discover-references')
        prune(args.revision, args.retention_days, protected, not args.delete, args.discover_references)
        return
    if not re.fullmatch('[0-9a-f]{40}', args.revision):
        parser.error('a full source commit is required')
    base = os.environ['BOOKIE_REGISTRY_URL'].rstrip('/')
    url = base + '/' + args.revision + '-' + args.group + '/'
    (publish if args.operation == 'publish' else fetch)(args.directory, args.revision, args.group, url)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError, urllib.error.HTTPError) as error:
        raise SystemExit(str(error))
