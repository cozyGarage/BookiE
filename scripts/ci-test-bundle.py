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
BUNDLE_GROUP_CONFIG = json.loads((ROOT / 'scripts/ci-test-bundle-groups.json').read_text())['groups']
BUNDLE_GROUPS = {'default', 'direct', 'unit', 'sandbox', 'workspace', 'gpui'} | set(BUNDLE_GROUP_CONFIG)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verify(bundle, revision, expected_group=None):
    index = json.loads((bundle / 'index.json').read_text())
    if index.get('schema_version') != 1 or index.get('source_commit') != revision:
        raise ValueError('test bundle identity mismatch')
    if expected_group and index.get('build_group') != expected_group:
        raise ValueError('test bundle build group mismatch')
    if index.get('architecture') != platform.machine():
        raise ValueError('test bundle architecture mismatch')
    if not index.get('tests') or not index.get('files'):
        raise ValueError('empty test bundle')
    for name, expected in index['files'].items():
        path = (bundle / name).resolve()
        if not path.is_relative_to(bundle.resolve()) or not path.is_file():
            raise ValueError('bundle file is missing or outside its directory')
        if path.stat().st_size != expected['size'] or digest(path) != expected['sha256']:
            raise ValueError('bundle file integrity mismatch')
    archive_only = index.get('archive_only', False)
    if type(archive_only) is not bool:
        raise ValueError('test bundle contents are invalid')
    for entry in index['tests']:
        if not archive_only and entry['path'] not in index['files']:
            raise ValueError('unverified test executable')
    if any(path not in index['files'] for path in index.get('binaries', {}).values()):
        raise ValueError('unverified helper executable')
    if (archive_only or (bundle / 'nextest.tar.zst').exists()) and 'nextest.tar.zst' not in index['files']:
        raise ValueError('unverified nextest archive')
    return index


def target_selected(package, kind, kinds, package_kinds):
    return not kinds and not package_kinds or kind in kinds or (package, kind) in package_kinds


def create(bundle, messages, revision, group, archive=None, archive_only=False, kinds=None, package_names=None,
           package_kinds=None):
    if bundle.exists():
        raise ValueError('bundle destination already exists')
    metadata_command = ['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1']
    if group == 'gpui':
        metadata_command.extend(['--manifest-path', str(ROOT / 'apps/gpui/Cargo.toml')])
    metadata = json.loads(subprocess.check_output(metadata_command, cwd=ROOT))
    packages = {package['id']: package for package in metadata['packages']}
    bundle.mkdir(parents=True)
    (bundle / 'tests').mkdir()
    (bundle / 'bin').mkdir()
    (bundle / 'schemas').mkdir()
    subprocess.run(['glib-compile-schemas', '--strict', '--targetdir', str(bundle / 'schemas'), str(ROOT / 'apps/linux/data')], check=True)
    index = {'schema_version': 1, 'source_commit': revision, 'build_group': group,
             'build_image': os.environ.get('BOOKIE_BUILD_RUNTIME_ID') or os.environ.get('BOOKIE_BUILD_IMAGE', ''),
             'architecture': platform.machine(), 'rust_version': subprocess.check_output(['rustc', '--version'], text=True).strip(),
             'features': ['kerberos'] if group != 'gpui' else [],
             'application_source_commit': revision, 'test_suite_commit': revision, 'ci_recipe_commit': revision,
             'tests': [], 'binaries': {}, 'archive_only': archive_only,
             'package_directories': {}, 'files': {}}
    for line in messages.read_text().splitlines():
        message = json.loads(line)
        if message.get('reason') != 'compiler-artifact' or not message.get('executable'):
            continue
        package = packages[message['package_id']]
        target = message['target']
        name = package['name']
        if package_names and name not in package_names:
            continue
        index['package_directories'][name] = str(Path(package['manifest_path']).parent.relative_to(ROOT))
        source = Path(message['executable'])
        if message['profile']['test']:
            kind = 'test' if 'test' in target['kind'] else 'bin' if 'bin' in target['kind'] else 'lib'
            if not target_selected(name, kind, kinds or set(), package_kinds or set()):
                continue
            destination = 'tests/' + name + '-' + source.name
            if not archive_only:
                shutil.copy2(source, bundle / destination)
            index['tests'].append({'package': name, 'kind': kind, 'name': target['name'], 'path': destination})
        else:
            if group != 'default' and name not in BUNDLE_GROUP_CONFIG.get(group, {}).get('bin', []):
                continue
            destination = 'bin/' + target['name']
            shutil.copy2(source, bundle / destination)
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
    parser.add_argument('--group', choices=sorted(BUNDLE_GROUPS), default='default')
    parser.add_argument('--expected-group', choices=sorted(BUNDLE_GROUPS))
    parser.add_argument('--archive-only', action='store_true')
    parser.add_argument('--test-kind', action='append', choices=['lib', 'bin', 'test'])
    parser.add_argument('--package', action='append')
    parser.add_argument('--package-kind', action='append', default=[])
    args = parser.parse_args()
    if args.operation == 'create':
        if not args.messages:
            parser.error('create requires --messages')
        package_kinds = set()
        for value in args.package_kind:
            package, separator, kind = value.partition(':')
            if not separator or not package or kind not in {'lib', 'bin', 'test'}:
                parser.error('--package-kind must be PACKAGE:lib, PACKAGE:bin or PACKAGE:test')
            package_kinds.add((package, kind))
        create(args.bundle, args.messages, args.revision, args.group, args.archive, args.archive_only,
               set(args.test_kind or []), set(args.package or []), package_kinds)
    else:
        index = verify(args.bundle, args.revision, args.expected_group)
        contents = 'Nextest archive' if index.get('archive_only') else f"{len(index['tests'])} test executables"
        print(f"verified {contents} for {args.revision} ({index['build_group']})")


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        raise SystemExit(str(error))
