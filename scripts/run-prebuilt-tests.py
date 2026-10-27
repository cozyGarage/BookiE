#!/usr/bin/env python3
import argparse
import fnmatch
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def load_index(bundle):
    spec = importlib.util.spec_from_file_location('ci_test_bundle', Path(__file__).resolve().with_name('ci-test-bundle.py'))
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    revision = os.environ.get('GITHUB_SHA')
    return verifier.verify(bundle, revision or json.loads((bundle / 'index.json').read_text())['source_commit'])


def commands(bundle, arguments, index=None):
    bundle = bundle.resolve()
    index = index or load_index(bundle)
    if index.get('schema_version') != 1:
        raise ValueError('unsupported test bundle schema')
    revision = os.environ.get('GITHUB_SHA')
    if revision and index.get('source_commit') != revision:
        raise ValueError('test bundle source revision differs from the checkout')
    parser = argparse.ArgumentParser(allow_abbrev=False)
    parser.add_argument('--locked', action='store_true')
    parser.add_argument('-p', '--package', action='append', default=[])
    parser.add_argument('--workspace', action='store_true')
    parser.add_argument('--exclude', action='append', default=[])
    parser.add_argument('--test', action='append', default=[])
    parser.add_argument('--tests', action='store_true')
    parser.add_argument('--lib', action='store_true')
    parser.add_argument('--bins', action='store_true')
    parser.add_argument('filter', nargs='?')
    separator = arguments.index('--') if '--' in arguments else len(arguments)
    try:
        selection = parser.parse_args(arguments[:separator])
    except SystemExit as error:
        raise ValueError('unsupported prebuilt test selection') from error
    test_arguments = arguments[separator + 1:]
    if selection.filter:
        test_arguments = [selection.filter, *test_arguments]
    if not selection.package and not selection.workspace:
        raise ValueError('an explicit package selection is required')
    packages = set(index['package_directories']) if selection.workspace else set(selection.package)
    packages -= set(selection.exclude)
    missing_packages = packages - set(index['package_directories'])
    if missing_packages:
        raise ValueError('a requested prebuilt package is missing')
    all_targets = selection.tests or not (selection.test or selection.lib or selection.bins)
    selected = [entry for entry in index['tests'] if entry['package'] in packages
                and (all_targets or (entry['kind'] == 'test' and any(fnmatch.fnmatchcase(entry['name'], pattern) for pattern in selection.test))
                     or (entry['kind'] == 'lib' and selection.lib)
                     or (entry['kind'] == 'bin' and selection.bins))]
    if not selected:
        raise ValueError('no prebuilt test target matches the selection')
    missing = [pattern for pattern in selection.test if not any(entry['kind'] == 'test' and fnmatch.fnmatchcase(entry['name'], pattern) for entry in selected)]
    if missing:
        raise ValueError('a requested prebuilt test target is missing')
    result = []
    for entry in selected:
        executable = (bundle / entry['path']).resolve()
        if not executable.is_relative_to(bundle) or not executable.is_file():
            raise ValueError('test executable is missing or outside the bundle')
        result.append([str(executable), *test_arguments])
    return result


def environment(bundle, index):
    result = os.environ.copy()
    result['GSETTINGS_SCHEMA_DIR'] = str(bundle.resolve() / 'schemas')
    for name, path in index['binaries'].items():
        if path not in index['files']:
            raise ValueError('unverified helper executable')
        result['CARGO_BIN_EXE_' + name] = str(bundle.resolve() / path)
    return result


def main():
    bundle = Path(os.environ['BOOKIE_TEST_BUNDLE'])
    index = load_index(bundle)
    execution_environment = environment(bundle, index)
    for command in commands(bundle, sys.argv[1:], index):
        entry = next(entry for entry in index['tests'] if (bundle / entry['path']).resolve() == Path(command[0]))
        directory = (ROOT / index['package_directories'][entry['package']]).resolve()
        if not directory.is_relative_to(ROOT.resolve()):
            raise ValueError('package directory is outside the source checkout')
        result = subprocess.run(command, cwd=directory, env=execution_environment)
        if result.returncode:
            return result.returncode
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (ValueError, KeyError, OSError) as error:
        raise SystemExit(str(error))
