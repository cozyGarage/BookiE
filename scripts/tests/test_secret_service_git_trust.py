import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class SecretServiceGitTrustTests(unittest.TestCase):
    def test_nested_tests_use_private_home_and_checkout_only_git_trust(self):
        with tempfile.TemporaryDirectory() as temporary:
            bin_dir = Path(temporary) / "bin"
            bin_dir.mkdir()
            self.write_command(bin_dir, "dbus-run-session", "shift; exec \"$@\"")
            self.write_command(bin_dir, "gnome-keyring-daemon", "exit 0")
            self.write_command(bin_dir, "secret-tool", "exit 0")
            python = bin_dir / "python3"
            python.write_text(
                "#!/usr/bin/python3\n"
                "import os, subprocess, sys\n"
                "assert os.environ['HOME'] != os.environ['ORIGINAL_HOME']\n"
                "assert os.environ['GIT_CONFIG_NOSYSTEM'] == '1'\n"
                "config = os.environ['GIT_CONFIG_GLOBAL']\n"
                "trusted = subprocess.check_output(['git', 'config', '--file', config, '--get-all', 'safe.directory'], text=True).splitlines()\n"
                "assert trusted == [os.environ['EXPECTED_REPO']]\n"
                "env = os.environ | {'GIT_TEST_ASSUME_DIFFERENT_OWNER': '1'}\n"
                "subprocess.run(['git', 'rev-parse', 'HEAD'], check=True, env=env, capture_output=True)\n"
                "with open(os.environ['RUN_COUNT'], 'a', encoding='utf-8') as count: count.write('run\\n')\n"
            )
            python.chmod(0o755)
            count = Path(temporary) / "runs"
            environment = os.environ.copy()
            for key in ("BOOKIE_SECRET_DBUS_ACTIVE", "GIT_CONFIG_GLOBAL", "GIT_CONFIG_NOSYSTEM", "GIT_TEST_ASSUME_DIFFERENT_OWNER"):
                environment.pop(key, None)
            environment.update(
                {
                    "PATH": f"{bin_dir}:{environment['PATH']}",
                    "HOME": str(Path(temporary) / "original-home"),
                    "ORIGINAL_HOME": str(Path(temporary) / "original-home"),
                    "EXPECTED_REPO": str(ROOT),
                    "RUN_COUNT": str(count),
                }
            )
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/test-secret-service.sh")],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(count.read_text().splitlines(), ["run", "run"])

    def write_command(self, directory, name, body):
        command = directory / name
        command.write_text(f"#!/usr/bin/env bash\n{body}\n")
        command.chmod(0o755)


if __name__ == "__main__":
    unittest.main()
