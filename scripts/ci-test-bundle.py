#!/usr/bin/env python3
import argparse
import hashlib
import platform
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verify(bundle, revision):
    index = json.loads((bundle / 'index.json').read_text())
    if index.get('schema_version') != 1 or index.get('source_commit') != revision:
        raise ValueError('test bundle identity mismatch')
    if not index.get('tests') or not index.get('files'):
        raise ValueError('empty test bundle')
    for name, expected in index['files'].items():
        path = (bundle / name).resolve()
        if not path.is_relative_to(bundle.resolve()) or not path.is_file():
            raise ValueError('bundle file is missing or outside its directory')
        if path.stat().st_size != expected['size'] or digest(path) != expected['sha256']:
            raise ValueError('bundle file integrity mismatch')
    for entry in index['tests']:
        if entry['path'] not in index['files']:
            raise ValueError('unverified test executable')
    return index


def create(bundle, messages, revision, group, archive=None):
    if bundle.exists():
        raise ValueError('bundle destination already exists')
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'], cwd=ROOT))
    packages = {package['id']: package for package in metadata['packages']}
    bundle.mkdir(parents=True)
    (bundle / 'tests').mkdir()
    (bundle / 'bin').mkdir()
    (bundle / 'schemas').mkdir()
    subprocess.run(['glib-compile-schemas', '--strict', '--targetdir', str(bundle / 'schemas'), str(ROOT / 'apps/linux/data')], check=True)
    index = {'schema_version': 1, 'source_commit': revision, 'build_group': group,
             'build_image': os.environ.get('BOOKIE_BUILD_IMAGE', ''),
             'architecture': platform.machine(), 'rust_version': subprocess.check_output(['rustc', '--version'], text=True).strip(),
             'features': ['kerberos', 'duckdb'] if group == 'duckdb' else ['kerberos'],
             'application_source_commit': revision, 'test_suite_commit': revision, 'ci_recipe_commit': revision,
             'tests': [], 'binaries': {},
             'package_directories': {}, 'files': {}}
    for line in messages.read_text().splitlines():
        message = json.loads(line)
        if message.get('reason') != 'compiler-artifact' or not message.get('executable'):
            continue
        package = packages[message['package_id']]
        target = message['target']
        name = package['name']
        index['package_directories'][name] = str(Path(package['manifest_path']).parent.relative_to(ROOT))
        source = Path(message['executable'])
        destination = ('tests/' + name + '-' + source.name if message['profile']['test'] else 'bin/' + target['name'])
        shutil.copy2(source, bundle / destination)
        if message['profile']['test']:
            kind = 'test' if 'test' in target['kind'] else 'bin' if 'bin' in target['kind'] else 'lib'
            index['tests'].append({'package': name, 'kind': kind, 'name': target['name'], 'path': destination})
        else:
            index['binaries'][target['name']] = destination
    if archive:
        shutil.copy2(archive, bundle / 'nextest.tar.zst')
    if not index['tests']:
        raise ValueError('build produced no test executables')
    for path in sorted(path for path in bundle.rglob('*') if path.is_file()):
        index['files'][str(path.relative_to(bundle))] = {'sha256': digest(path), 'size': path.stat().st_size}
    (bundle / 'index.json').write_text(json.dumps(index, indent=2, sort_keys=True) + '\n')
    verify(bundle, revision)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('operation', choices=['create', 'verify'])
    parser.add_argument('bundle', type=Path)
    parser.add_argument('revision')
    parser.add_argument('--messages', type=Path)
    parser.add_argument('--archive', type=Path)
    parser.add_argument('--group', choices=['default', 'duckdb'], default='default')
    args = parser.parse_args()
    if args.operation == 'create':
        if not args.messages:
            parser.error('create requires --messages')
        create(args.bundle, args.messages, args.revision, args.group, args.archive)
    else:
        index = verify(args.bundle, args.revision)
        print(f"verified {len(index['tests'])} test executables for {args.revision} ({index['build_group']})")


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        raise SystemExit(str(error))
