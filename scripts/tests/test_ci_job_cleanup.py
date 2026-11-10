import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = (ROOT / ".forgejo/workflows/ci.yml", ROOT / ".forgejo/workflows/nightly.yml")
CLEANUP = ROOT / "scripts/cleanup-ci-job.sh"


def runner_jobs(path):
    jobs = {}
    active = False
    name = None
    block = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line == "jobs:":
            active = True
            continue
        if not active:
            continue
        header = re.fullmatch(r"  ([a-z0-9_-]+):", line)
        if header:
            if name is not None and any("runs-on:" in item for item in block):
                jobs[name] = "\n".join(block)
            name = header.group(1)
            block = [line]
            continue
        if name is not None:
            block.append(line)
    if name is not None and any("runs-on:" in item for item in block):
        jobs[name] = "\n".join(block)
    return jobs


class CiJobCleanupTests(unittest.TestCase):
    def test_gtk_server_fixtures_have_job_scoped_cleanup_labels(self):
        jobs = runner_jobs(WORKFLOWS[0])
        for job in ("e2e", "gtk-server"):
            self.assertIn("BOOKIE_TEST_RUN_ID:", jobs[job])
            self.assertIn("${{ matrix.server }}", jobs[job])
        for server in ("postgres", "mysql", "mysql-approval", "mongodb", "clickhouse"):
            script = (ROOT / f"scripts/test-gtk-{server}.sh").read_text(encoding="utf-8")
            self.assertIn('com.bookie.test-run=${BOOKIE_TEST_RUN_ID:-local-$$}', script)

    def test_every_runner_job_finishes_with_the_finalizer(self):
        for workflow in WORKFLOWS:
            with self.subTest(workflow=workflow.name):
                jobs = runner_jobs(workflow)
                self.assertTrue(jobs)
                for name, block in jobs.items():
                    with self.subTest(job=name):
                        self.assertIn("      - name: Finalize runner job", block)
                        self.assertIn("        if: always()", block)
                        self.assertIn("        shell: bash", block)
                        self.assertIn("bash src/scripts/cleanup-ci-job.sh", block)
                        step_headers = re.findall(r"(?m)^      - (?:name:|uses:|run:)", block)
                        self.assertEqual(step_headers[-1], "      - name:")

    def test_checkout_uses_the_documented_disk_reserve(self):
        for workflow in WORKFLOWS:
            with self.subTest(workflow=workflow.name):
                for name, block in runner_jobs(workflow).items():
                    if "/usr/local/libexec/bookie-ci-cache checkout" in block:
                        with self.subTest(job=name):
                            self.assertIn(
                                "shell: /usr/local/libexec/bookie-ci-cache checkout --min-free-gib 30 {0}",
                                block,
                            )

    def test_finalizer_removes_job_files_and_keeps_shared_targets(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary) / "workspace"
            workspace.mkdir()
            job_tmp = Path(temporary) / "job-tmp"
            job_tmp.mkdir()
            directories = {"src", "gtk-artifact", "floor-artifact", "floor-package", "test-bundles", "mutants-core-1"}
            for name in (
                "src",
                "gtk-artifact",
                "floor-artifact",
                "floor-package",
                "floor-package-path",
                "test-bundles",
                "coverage.json",
                "coverage-postgres.json",
                "mutants-core-1",
                "soak-1-1.log",
            ):
                path = workspace / name
                path.mkdir() if name in directories else path.touch()
            keep = workspace / "unrelated"
            keep.write_text("preserve", encoding="utf-8")
            target = Path(temporary) / "shared-target"
            target.mkdir()
            (target / "cache").write_text("preserve", encoding="utf-8")
            result = subprocess.run(
                ["bash", str(CLEANUP)],
                cwd=workspace,
                env={
                    **os.environ,
                    "GITHUB_RUN_ID": "123",
                    "GITHUB_WORKSPACE": str(workspace),
                    "CARGO_TARGET_DIR": str(target),
                    "TMPDIR": str(job_tmp),
                },
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(keep.exists())
            self.assertTrue((target / "cache").exists())
            for name in (
                "src",
                "gtk-artifact",
                "floor-artifact",
                "floor-package",
                "floor-package-path",
                "test-bundles",
                "coverage.json",
                "coverage-postgres.json",
                "mutants-core-1",
                "soak-1-1.log",
            ):
                self.assertFalse((workspace / name).exists())

    def test_finalizer_runs_scoped_container_and_cache_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary) / "workspace"
            scripts = workspace / "src/scripts"
            scripts.mkdir(parents=True)
            job_tmp = Path(temporary) / "job-tmp"
            job_tmp.mkdir()
            (scripts / "cleanup-testcontainers.py").touch()
            (scripts / "ci-cache.py").touch()
            bin_dir = Path(temporary) / "bin"
            bin_dir.mkdir()
            calls = Path(temporary) / "calls"
            python = bin_dir / "python3"
            python.write_text(
                "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> \"$CLEANUP_CALLS\"\n",
                encoding="utf-8",
            )
            python.chmod(0o755)
            result = subprocess.run(
                ["bash", str(CLEANUP)],
                cwd=workspace,
                env={
                    **os.environ,
                    "PATH": f"{bin_dir}:{os.environ['PATH']}",
                    "GITHUB_RUN_ID": "123",
                    "GITHUB_WORKSPACE": str(workspace),
                    "CARGO_TARGET_DIR": "/var/cache/ci/target/unit",
                    "TMPDIR": str(job_tmp),
                    "BOOKIE_TEST_RUN_ID": "forgejo-123-1-postgres",
                    "CLEANUP_CALLS": str(calls),
                },
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                calls.read_text(encoding="utf-8").splitlines(),
                [
                    f"{scripts / 'cleanup-testcontainers.py'} forgejo-123-1-postgres",
                    f"{scripts / 'ci-cache.py'} cleanup --min-free-gib 30",
                ],
            )

    def test_finalizer_removes_the_driver_tls_compose_project(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary) / "workspace"
            scripts = workspace / "src/scripts"
            scripts.mkdir(parents=True)
            job_tmp = Path(temporary) / "job-tmp"
            job_tmp.mkdir()
            fixture = workspace / "src/tests/fixtures/driver-tls"
            fixture.mkdir(parents=True)
            compose_file = fixture / "docker-compose.yml"
            compose_file.touch()
            (scripts / "cleanup-testcontainers.py").touch()
            bin_dir = Path(temporary) / "bin"
            bin_dir.mkdir()
            calls = Path(temporary) / "calls"
            python = bin_dir / "python3"
            python.write_text(
                "#!/usr/bin/env bash\nprintf 'python %s\\n' \"$*\" >> \"$CLEANUP_CALLS\"\n",
                encoding="utf-8",
            )
            python.chmod(0o755)
            docker = bin_dir / "docker"
            docker.write_text(
                "#!/usr/bin/env bash\nprintf 'docker %s\\n' \"$*\" >> \"$CLEANUP_CALLS\"\n",
                encoding="utf-8",
            )
            docker.chmod(0o755)
            run_id = "forgejo-123-1-driver-tls"
            result = subprocess.run(
                ["bash", str(CLEANUP)],
                cwd=workspace,
                env={
                    **os.environ,
                    "PATH": f"{bin_dir}:{os.environ['PATH']}",
                    "GITHUB_RUN_ID": "123",
                    "GITHUB_JOB": "driver-tls",
                    "GITHUB_WORKSPACE": str(workspace),
                    "CARGO_TARGET_DIR": str(Path(temporary) / "target"),
                    "TMPDIR": str(job_tmp),
                    "BOOKIE_TEST_RUN_ID": run_id,
                    "CLEANUP_CALLS": str(calls),
                },
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                calls.read_text(encoding="utf-8").splitlines(),
                [
                    f"python {scripts / 'cleanup-testcontainers.py'} {run_id}",
                    f"docker compose --project-name {run_id} --project-directory {fixture} "
                    f"-f {compose_file} down --volumes --remove-orphans",
                ],
            )
            self.assertFalse((workspace / "src").exists())

    def test_pg_dump_uses_and_removes_a_run_scoped_temp_directory(self):
        job = runner_jobs(WORKFLOWS[0])["pg-dump"]
        self.assertIn("TMPDIR: ${{ github.workspace }}/.tmp-pg-dump-${{ github.run_id }}-${{ github.run_attempt }}", job)
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary) / "workspace"
            workspace.mkdir()
            job_tmp = workspace / ".tmp-pg-dump-123-2"
            job_tmp.mkdir()
            (job_tmp / "temporary-data").touch()
            result = subprocess.run(
                ["bash", str(CLEANUP)],
                cwd=workspace,
                env={
                    **os.environ,
                    "GITHUB_RUN_ID": "123",
                    "GITHUB_RUN_ATTEMPT": "2",
                    "GITHUB_WORKSPACE": str(workspace),
                    "CARGO_TARGET_DIR": str(Path(temporary) / "target"),
                    "TMPDIR": str(job_tmp),
                },
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(job_tmp.exists())

    def test_driver_finalizer_removes_only_its_scoped_scratch_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            bin_dir = Path(temporary) / "bin"
            bin_dir.mkdir()
            calls = Path(temporary) / "calls"
            rm = bin_dir / "rm"
            rm.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['RM_CALLS'], 'a', encoding='utf-8') as stream:\n"
                "    stream.write(json.dumps(sys.argv[1:]) + '\\n')\n",
                encoding="utf-8",
            )
            rm.chmod(0o755)
            scratch = "/var/cache/ci/tmp/bookie-driver-123-2-mysql"
            result = subprocess.run(
                ["bash", str(CLEANUP)],
                env={
                    **os.environ,
                    "PATH": f"{bin_dir}:{os.environ['PATH']}",
                    "GITHUB_RUN_ID": "123",
                    "GITHUB_JOB": "driver",
                    "TMPDIR": scratch,
                    "RM_CALLS": str(calls),
                },
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            rm_calls = [json.loads(line) for line in calls.read_text(encoding="utf-8").splitlines()]
            self.assertIn(["-rf", "--", scratch], rm_calls)

            calls.unlink()
            result = subprocess.run(
                ["bash", str(CLEANUP)],
                env={
                    **os.environ,
                    "PATH": f"{bin_dir}:{os.environ['PATH']}",
                    "GITHUB_RUN_ID": "123",
                    "GITHUB_JOB": "unit",
                    "TMPDIR": scratch,
                    "RM_CALLS": str(calls),
                },
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            rm_calls = [json.loads(line) for line in calls.read_text(encoding="utf-8").splitlines()]
            self.assertNotIn(["-rf", "--", scratch], rm_calls)

    def test_driver_tls_workflow_assigns_a_run_scoped_fixture_project(self):
        job = runner_jobs(WORKFLOWS[0])["driver-tls"]
        run_id = "BOOKIE_TEST_RUN_ID: forgejo-${{ github.run_id }}-${{ github.run_attempt }}-driver-tls"
        self.assertIn(run_id, job)
        script = (ROOT / "scripts/test-driver-tls.sh").read_text(encoding="utf-8")
        self.assertIn('--project-name "${BOOKIE_TEST_RUN_ID:-driver-tls}"', script)

    def test_nightly_outputs_do_not_use_tmp(self):
        self.assertNotIn("/tmp/", WORKFLOWS[1].read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
