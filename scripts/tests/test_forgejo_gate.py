from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HEAD = "a" * 40
GIT = """#!/usr/bin/env bash
case "$1" in
  remote) echo https://example.invalid/fixture/repo.git ;;
  rev-parse|ls-remote) printf '%s\\n' "$GATE_TEST_HEAD" ;;
  diff) echo Cargo.toml ;;
  fetch) exit 0 ;;
  *) exit 91 ;;
esac
"""
CURL = """import fcntl, json, os, pathlib, sys, time
fixture = pathlib.Path(os.environ['GATE_TEST_FIXTURE'])
data = json.loads(fixture.read_text())
active_file = os.environ.get('GATE_TEST_ACTIVE_FILE')
if active_file:
    state = pathlib.Path(active_file)
    with state.open('r+') as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        current, maximum = map(int, (stream.read() or '0 0').split())
        current += 1
        stream.seek(0)
        stream.truncate()
        stream.write(f'{current} {max(current, maximum)}')
        stream.flush()
        fcntl.flock(stream, fcntl.LOCK_UN)
    time.sleep(float(os.environ.get('GATE_TEST_CURL_DELAY', '0')))
    with state.open('r+') as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        current, maximum = map(int, stream.read().split())
        stream.seek(0)
        stream.truncate()
        stream.write(f'{current - 1} {maximum}')
        stream.flush()
        fcntl.flock(stream, fcntl.LOCK_UN)
if any('fixture-token' in arg for arg in sys.argv):
    raise SystemExit('gate token passed in curl arguments')
config = pathlib.Path(sys.argv[sys.argv.index('--config') + 1]).read_text()
if 'header = "Authorization: token fixture-token"' not in config:
    raise SystemExit('gate authorization header missing from curl config')
if '/actions/jobs/' in sys.argv[-1] and '/logs' in sys.argv[-1]:
    job_id = int(sys.argv[-1].split('/actions/jobs/', 1)[1].split('/', 1)[0])
    job = next(job for job in data['tasks'] if job.get('id') == job_id)
    if job.get('status') != 'failure':
        raise SystemExit('job logs requested for nonfailure job')
    print(data['logs'])
elif '/actions/runs/' in sys.argv[-1] and '/jobs' in sys.argv[-1]:
    print(json.dumps(data['tasks']))
else:
    state = fixture.with_suffix('.polls')
    polls = int(state.read_text()) if state.exists() else 0
    state.write_text(str(polls + 1))
    run = dict(data['run'], status=data.get('initial_status', 'failure') if polls == 0 else 'success')
    print(json.dumps({'workflow_runs': [run]}))
"""


def gate(tasks, initial_status='failure'):
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        tools = root / 'bin'
        tools.mkdir()
        for name, source in [('git', GIT), ('curl', f'#!{sys.executable}\n' + CURL), ('sleep', '#!/bin/sh\nexit 0\n')]:
            path = tools / name
            path.write_text(source)
            path.chmod(0o755)
        fixture = root / 'fixture.json'
        fixture.write_text(json.dumps({'run': {'id': 928, 'index_in_repo': 927, 'commit_sha': HEAD, 'workflow_id': 'ci.yml'}, 'initial_status': initial_status, 'tasks': tasks, 'logs': 'compile failed: fixture error'}))
        token = root / 'token'
        token.write_text('fixture-token')
        env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ['PATH'],
                   GATE_TEST_HEAD=HEAD, GATE_TEST_FIXTURE=str(fixture), FORGEJO_TOKEN_FILE=str(token),
                   FORGEJO_GATE_LOCK=str(root / 'gate.lock'), FORGEJO_GATE_TIMEOUT_MINUTES='1')
        return subprocess.run(['bash', str(ROOT / 'scripts/forgejo-gate.sh'), 'subject'],
                              cwd=root, env=env, capture_output=True, text=True, timeout=10)


class ForgejoGateTests(unittest.TestCase):
    def test_run_list_requests_use_first_page(self):
        script = (ROOT / "scripts/forgejo-gate.sh").read_text()
        self.assertIn('"$api/actions/runs?page=1&limit=1"', script)
        self.assertIn('"$api/actions/runs?page=1&limit=50"', script)

    def test_a_matching_retry_waits_when_api_id_differs_from_run_number(self):
        result = gate([{'run_number': 927, 'head_sha': HEAD, 'status': 'running', 'name': 'widgets'}])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('927', result.stdout)
        self.assertIn('928', result.stdout)

    def test_failure_reporting_uses_jobs_for_the_failed_run(self):
        result = gate([
            {'id': 101, 'status': 'failure', 'name': 'expected-job'},
            {'id': 102, 'status': 'cancelled', 'name': 'cancelled-job'},
            {'status': 'blocked', 'name': 'downstream-job'},
            {'status': 'success', 'name': 'passed-job'},
        ])
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn('expected-job', result.stdout)
        self.assertIn('cancelled-job', result.stdout)
        self.assertNotIn('job logs requested for nonfailure job', result.stdout + result.stderr)
        self.assertIn('compile failed: fixture error', result.stdout)
        self.assertNotIn('downstream-job', result.stdout)
        self.assertNotIn('passed-job', result.stdout)

    def test_cancelled_jobs_are_reported_without_fetching_logs(self):
        result = gate([{'id': 101, 'status': 'cancelled', 'name': 'cancelled-job'}], initial_status='cancelled')
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn('cancelled cancelled-job (job 101)', result.stdout)
        self.assertNotIn('job logs requested for nonfailure job', result.stdout + result.stderr)

    def test_at_most_two_gates_run_at_once_on_this_host(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            active = root / 'active'
            active.write_text('0 0')
            lock_dir = root / 'locks'
            processes = []
            for index in range(3):
                run_root = root / str(index)
                tools = run_root / 'bin'
                tools.mkdir(parents=True)
                for name, source in [('git', GIT), ('curl', f'#!{sys.executable}\n' + CURL), ('sleep', '#!/bin/sh\nexit 0\n')]:
                    path = tools / name
                    path.write_text(source)
                    path.chmod(0o755)
                fixture = run_root / 'fixture.json'
                fixture.write_text(json.dumps({'run': {'id': 928, 'index_in_repo': 927, 'commit_sha': HEAD, 'workflow_id': 'ci.yml'}, 'initial_status': 'success', 'tasks': [], 'logs': ''}))
                token = run_root / 'token'
                token.write_text('fixture-token')
                env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ['PATH'],
                           GATE_TEST_HEAD=HEAD, GATE_TEST_FIXTURE=str(fixture), FORGEJO_TOKEN_FILE=str(token),
                           FORGEJO_GATE_LOCK=str(run_root / 'gate.lock'), FORGEJO_GATE_LOCK_DIR=str(lock_dir),
                           FORGEJO_GATE_TIMEOUT_MINUTES='1', GATE_TEST_ACTIVE_FILE=str(active), GATE_TEST_CURL_DELAY='0.2')
                processes.append(subprocess.Popen(['bash', str(ROOT / 'scripts/forgejo-gate.sh'), 'subject'],
                                                  cwd=run_root, env=env, stdout=subprocess.PIPE,
                                                  stderr=subprocess.PIPE, text=True))
            for process in processes:
                stdout, stderr = process.communicate(timeout=10)
                self.assertEqual(process.returncode, 0, stdout + stderr)
            self.assertEqual(active.read_text().split()[1], '2')


if __name__ == '__main__':
    unittest.main()
