import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class PostgresReleaseStartupTests(unittest.TestCase):
    def test_private_umask_keeps_seed_readable_without_exposing_keys(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            shutil.copy(ROOT / "scripts/test-postgres-release.sh", root / "scripts")
            fixture = root / "tests/fixtures/postgres-release"
            seed = fixture / "seed"
            seed.mkdir(parents=True, mode=0o700)
            sql = seed / "01-seed.sql"
            sql.write_text("SELECT 1;\n")
            sql.chmod(0o600)
            materials = fixture / "materials"
            materials.mkdir(mode=0o700)
            key = materials / "server.key"
            key.write_text("fixture-private-material")
            key.chmod(0o600)
            (fixture / "generate-materials.sh").write_text("exit 0\n")
            binary = root / "bin"
            binary.mkdir()
            docker = binary / "docker"
            docker.write_text('''#!/usr/bin/env python3
import os, sys
from pathlib import Path
if "up" in sys.argv:
    seed = Path(os.environ["PG_FIXTURE_ROOT"]) / "seed"
    readable = seed.stat().st_mode & 0o005 == 0o005 and (seed / "01-seed.sql").stat().st_mode & 0o004
    sys.exit(42 if readable else 43)
''')
            docker.chmod(0o755)
            result = subprocess.run(
                ["bash", str(root / "scripts/test-postgres-release.sh")],
                env=dict(os.environ, PATH=f"{binary}:{os.environ['PATH']}", PG_FIXTURE_ROOT=str(fixture)),
                umask=0o077, text=True, capture_output=True,
            )
            self.assertEqual(result.returncode, 42, result.stderr)
            self.assertEqual(seed.stat().st_mode & 0o777, 0o755)
            self.assertEqual(sql.stat().st_mode & 0o777, 0o644)
            self.assertEqual(materials.stat().st_mode & 0o777, 0o700)
            self.assertEqual(key.stat().st_mode & 0o777, 0o600)

    def test_container_job_uses_the_resolved_docker_host_for_fixture_ports(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            shutil.copy(ROOT / "scripts/test-postgres-release.sh", root / "scripts")
            fixture = root / "tests/fixtures/postgres-release"
            seed = fixture / "seed"
            seed.mkdir(parents=True)
            (seed / "01-seed.sql").write_text("SELECT 1;\n")
            (fixture / "materials").mkdir()
            (fixture / "generate-materials.sh").write_text("exit 0\n")
            bin_dir = root / "bin"
            bin_dir.mkdir()
            docker = bin_dir / "docker"
            docker.write_text('''#!/usr/bin/env python3
import os, sys
from pathlib import Path
if "up" in sys.argv:
    Path(os.environ["CAPTURE_BIND_ADDRESS"]).write_text(os.environ["BOOKIE_FIXTURE_BIND_ADDRESS"])
    sys.exit(42)
''')
            docker.chmod(0o755)
            getent = bin_dir / "getent"
            getent.write_text('#!/bin/sh\nprintf "172.17.0.1 STREAM host.docker.internal\\n"\n')
            getent.chmod(0o755)
            captured = root / "bind-address"
            result = subprocess.run(
                ["bash", str(root / "scripts/test-postgres-release.sh")],
                env=dict(
                    os.environ,
                    PATH=f"{bin_dir}:{os.environ['PATH']}",
                    BOOKIE_FIXTURE_HOST="host.docker.internal",
                    CAPTURE_BIND_ADDRESS=str(captured),
                    PG_FIXTURE_ROOT=str(fixture),
                ),
                text=True,
                capture_output=True,
            )

            self.assertEqual(result.returncode, 42, result.stderr)
            self.assertEqual(captured.read_text(), "172.17.0.1")

    def test_container_fixture_ports_are_bound_to_the_configured_address(self):
        compose = (ROOT / "tests/fixtures/postgres-release/docker-compose.yml").read_text()
        self.assertEqual(compose.count('${BOOKIE_FIXTURE_BIND_ADDRESS:-127.0.0.1}:'), 3)

        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        job = workflow.split("  pg-release:\n", 1)[1].split("  distro-floor-scenarios:\n", 1)[0]
        self.assertIn("options: --add-host=host.docker.internal:host-gateway", job)
        self.assertIn("BOOKIE_FIXTURE_HOST: host.docker.internal", job)


if __name__ == "__main__":
    unittest.main()
