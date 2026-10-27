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


if __name__ == "__main__":
    unittest.main()
