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
                    "GITHUB_WORKSPACE": str(workspace),
                    "CARGO_TARGET_DIR": str(target),
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
                    "GITHUB_WORKSPACE": str(workspace),
                    "CARGO_TARGET_DIR": "/var/cache/ci/target/unit",
                    "TABLEPRO_TEST_RUN_ID": "forgejo-123-1-postgres",
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

    def test_nightly_outputs_do_not_use_tmp(self):
        self.assertNotIn("/tmp/", WORKFLOWS[1].read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
