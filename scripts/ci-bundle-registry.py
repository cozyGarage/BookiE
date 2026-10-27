#!/usr/bin/env python3
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import tarfile
import tempfile
import urllib.error
import urllib.request
import urllib.parse
import zipfile

MAX_ARCHIVE = 8 * 1024 ** 3
MAX_EXTRACTED = 24 * 1024 ** 3
SPEC = importlib.util.spec_from_file_location('bundle', Path(__file__).with_name('ci-test-bundle.py'))
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)


def request(url, method='GET', data=None, size=None):
    headers = {'Authorization': 'token ' + os.environ['BOOKIE_REGISTRY_TOKEN']}
    if size is not None:
        headers.update({'Content-Type': 'application/octet-stream', 'Content-Length': str(size)})
    return urllib.request.urlopen(urllib.request.Request(url, data=data, method=method, headers=headers), timeout=180)


def extract(archive, destination):
    with tarfile.open(archive, 'r:gz') as source:
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
    if (manifest.get('schema_version') != 1 or manifest.get('source_commit') != revision
            or manifest.get('build_group') != group or not 0 < manifest['size'] <= MAX_ARCHIVE
            or not re.fullmatch('[0-9a-f]{64}', manifest['sha256'])
            or not re.fullmatch('[0-9a-f]{64}', manifest['index_sha256'])):
        raise ValueError('existing registry bundle manifest is invalid')
    return manifest


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
        manifest = {'schema_version': 1, 'source_commit': revision, 'build_group': group,
                    'sha256': bundle.digest(archive), 'size': size, 'index_sha256': bundle.digest(directory / 'index.json')}
        with archive.open('rb') as content, request(url + 'bundle.tar.gz', 'PUT', content, size) as response:
            if response.status != 201:
                raise ValueError('registry did not accept the bundle')
        data = json.dumps(manifest, sort_keys=True).encode()
        with request(url + 'manifest.json', 'PUT', data, len(data)) as response:
            if response.status != 201:
                raise ValueError('registry did not accept the bundle manifest')
        print(json.dumps(manifest, sort_keys=True))


def fetch(directory, revision, group, url):
    if directory.exists():
        index = bundle.verify(directory, revision)
        if index['build_group'] != group:
            raise ValueError('cached test bundle build group mismatch')
        return
    with request(url + 'manifest.json') as response:
        manifest = json.loads(response.read(65537))
    if (manifest.get('schema_version') != 1 or manifest.get('source_commit') != revision
            or manifest.get('build_group') != group or not 0 < manifest['size'] <= MAX_ARCHIVE
            or not re.fullmatch('[0-9a-f]{64}', manifest['sha256'])):
        raise ValueError('registry bundle identity or size is invalid')
    directory.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=directory.parent) as temporary:
        temporary = Path(temporary)
        archive = temporary / 'bundle.tar.gz'
        with request(url + 'bundle.tar.gz') as response, archive.open('wb') as output:
            remaining = manifest['size']
            while remaining:
                block = response.read(min(1024 * 1024, remaining))
                if not block:
                    raise ValueError('registry returned a truncated bundle')
                output.write(block)
                remaining -= len(block)
            if response.read(1):
                raise ValueError('registry bundle exceeds its declared size')
        if bundle.digest(archive) != manifest['sha256']:
            raise ValueError('registry bundle digest mismatch')
        staging = temporary / 'verified'
        staging.mkdir()
        extract(archive, staging)
        if bundle.digest(staging / 'index.json') != manifest['index_sha256']:
            raise ValueError('registry bundle index digest mismatch')
        index = bundle.verify(staging, revision)
        if index['build_group'] != group:
            raise ValueError('registry bundle build group mismatch')
        shutil.move(staging, directory)


class ArtifactRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, stream, code, message, headers, url):
        result = super().redirect_request(request, stream, code, message, headers, url)
        if result is not None and urllib.parse.urlsplit(url).netloc != urllib.parse.urlsplit(request.full_url).netloc:
            result.remove_header('Authorization')
        return result


def github_fetch(directory, revision, group):
    token = os.environ['BOOKIE_GITHUB_ARTIFACT_TOKEN']
    def api(path):
        req = urllib.request.Request('https://api.github.com/repos/cozyGarage/BookiE/' + path,
                                     headers={'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json'})
        return urllib.request.build_opener(ArtifactRedirect()).open(req, timeout=180)
    with api('actions/runs?head_sha=' + revision + '&status=success&per_page=20') as response:
        runs = json.load(response)['workflow_runs']
    expected = 'bookie-tests-' + revision + '-' + group
    for run in runs:
        if run['head_sha'] != revision or run['event'] == 'pull_request':
            continue
        with api('actions/runs/' + str(run['id']) + '/artifacts?per_page=100') as response:
            artifacts = json.load(response)['artifacts']
        for artifact in artifacts:
            if artifact['name'] != expected or artifact['expired']:
                continue
            if not 0 < artifact['size_in_bytes'] <= MAX_ARCHIVE:
                raise ValueError('GitHub artifact exceeds download limit')
            directory.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.TemporaryDirectory(dir=directory.parent) as temporary:
                temporary = Path(temporary)
                archive = temporary / 'bundle.zip'
                with api('actions/artifacts/' + str(artifact['id']) + '/zip') as response, archive.open('wb') as output:
                    total = 0
                    while block := response.read(1024 * 1024):
                        total += len(block)
                        if total > MAX_ARCHIVE:
                            raise ValueError('GitHub artifact exceeds download limit')
                        output.write(block)
                expected_digest = artifact.get('digest', '')
                if expected_digest != 'sha256:' + bundle.digest(archive):
                    raise ValueError('GitHub artifact digest is unavailable or mismatched')
                staging = temporary / 'verified'
                staging.mkdir()
                with zipfile.ZipFile(archive) as source:
                    members = source.infolist()
                    if len(members) > 5000 or sum(member.file_size for member in members) > MAX_EXTRACTED:
                        raise ValueError('GitHub artifact exceeds extraction limits')
                    for member in members:
                        path = (staging / member.filename).resolve()
                        if not path.is_relative_to(staging.resolve()) or (member.external_attr >> 16) & 0o170000 == 0o120000:
                            raise ValueError('unsafe GitHub artifact member')
                    source.extractall(staging)
                index = bundle.verify(staging, revision)
                if index['build_group'] != group:
                    raise ValueError('GitHub artifact build group mismatch')
                for entry in index['tests']:
                    (staging / entry['path']).chmod(0o755)
                for name in index['binaries'].values():
                    (staging / name).chmod(0o755)
                shutil.move(staging, directory)
                return
    raise ValueError('no successful GitHub build supplies the exact requested artifact')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('operation', choices=['publish', 'fetch', 'github-fetch'])
    parser.add_argument('directory', type=Path)
    parser.add_argument('revision')
    parser.add_argument('group', choices=['default', 'duckdb'])
    args = parser.parse_args()
    if not re.fullmatch('[0-9a-f]{40}', args.revision):
        parser.error('a full source commit is required')
    if args.operation == 'github-fetch':
        github_fetch(args.directory, args.revision, args.group)
        return
    base = os.environ['BOOKIE_REGISTRY_URL'].rstrip('/')
    url = base + '/' + args.revision + '-' + args.group + '/'
    (publish if args.operation == 'publish' else fetch)(args.directory, args.revision, args.group, url)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError, urllib.error.HTTPError) as error:
        raise SystemExit(str(error))
