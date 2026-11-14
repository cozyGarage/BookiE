import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class PostgresReleaseStartupTests(unittest.TestCase):
    def test_server_certificate_covers_local_and_container_hostnames(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary) / "postgres-release"
            fixture.mkdir()
            generator = fixture / "generate-materials.sh"
            shutil.copy(ROOT / "tests/fixtures/postgres-release/generate-materials.sh", generator)
            subprocess.run(["bash", str(generator)], check=True, capture_output=True, text=True)
            certificate = subprocess.run(
                ["openssl", "x509", "-in", str(fixture / "materials/server.crt"), "-noout", "-ext", "subjectAltName"],
                check=True,
                capture_output=True,
                text=True,
            )
            self.assertIn("DNS:localhost", certificate.stdout)
            self.assertIn("DNS:host.docker.internal", certificate.stdout)
            self.assertNotIn("IP Address:127.0.0.1", certificate.stdout)

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

    def test_fixture_hosts_route_container_and_local_ssh(self):
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
    Path(os.environ["CAPTURE_SSH_HOST"]).write_text(os.environ["BOOKIE_FIXTURE_SSH_HOST"])
    sys.exit(42)
''')
            docker.chmod(0o755)
            getent = bin_dir / "getent"
            getent.write_text('''#!/usr/bin/env python3
import sys
hosts = {"localhost": "127.0.0.1", "host.docker.internal": "172.17.0.1"}
print(hosts[sys.argv[-1]], "STREAM", sys.argv[-1])
''')
            getent.chmod(0o755)
            captured = root / "bind-address"
            captured_ssh = root / "ssh-host"
            cases = (
                ("host.docker.internal", "172.17.0.1", "host.docker.internal"),
                (None, "127.0.0.1", "127.0.0.1"),
            )

            for fixture_host, expected_bind, expected_ssh in cases:
                with self.subTest(fixture_host=fixture_host):
                    env = dict(
                        os.environ,
                        PATH=f"{bin_dir}:{os.environ['PATH']}",
                        CAPTURE_BIND_ADDRESS=str(captured),
                        CAPTURE_SSH_HOST=str(captured_ssh),
                        PG_FIXTURE_ROOT=str(fixture),
                    )
                    env.pop("BOOKIE_FIXTURE_HOST", None)
                    env.pop("BOOKIE_FIXTURE_SSH_HOST", None)
                    env.pop("BOOKIE_FIXTURE_BIND_ADDRESS", None)
                    if fixture_host is not None:
                        env["BOOKIE_FIXTURE_HOST"] = fixture_host
                    result = subprocess.run(
                        ["bash", str(root / "scripts/test-postgres-release.sh")],
                        env=env,
                        text=True,
                        capture_output=True,
                    )

                    self.assertEqual(result.returncode, 42, result.stderr)
                    self.assertEqual(captured.read_text(), expected_bind)
                    self.assertEqual(captured_ssh.read_text(), expected_ssh)

    def test_container_fixture_ports_are_bound_to_the_configured_address(self):
        compose = (ROOT / "tests/fixtures/postgres-release/docker-compose.yml").read_text()
        self.assertEqual(compose.count('${BOOKIE_FIXTURE_BIND_ADDRESS:-127.0.0.1}:'), 3)

        script = (ROOT / "scripts/test-postgres-release.sh").read_text()
        self.assertIn('fixture_host="${BOOKIE_FIXTURE_HOST:-localhost}"', script)
        self.assertIn('BOOKIE_GTK_POSTGRES_MTLS_HOST="$BOOKIE_FIXTURE_PROXY_HOST"', script)
        self.assertIn('BOOKIE_GTK_POSTGRES_SSH_AUDIT_HOST="$BOOKIE_FIXTURE_SSH_HOST"', script)

        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        job = workflow.split("  pg-release:\n", 1)[1].split("  distro-floor-scenarios:\n", 1)[0]
        self.assertIn("options: --add-host=host.docker.internal:host-gateway", job)
        self.assertIn("BOOKIE_FIXTURE_HOST: host.docker.internal", job)


if __name__ == "__main__":
    unittest.main()
