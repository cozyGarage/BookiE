import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class StreamProfileScriptTests(unittest.TestCase):
    def test_custom_output_directory_is_created_for_both_profiles(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            stubs = base / "bin"
            stubs.mkdir()
            self.write_stub(stubs, "cargo", "exit 0\n")
            self.write_stub(stubs, "install", 'for dest do :; done\nmkdir -p "$(dirname "$dest")"\n: > "$dest"\n')
            self.write_stub(stubs, "sha256sum", 'printf "binary-hash  %s\\n" "$1"\n')
            self.write_stub(stubs, "docker", 'printf "postgres-image\\n"\n')
            self.write_stub(stubs, "git", 'printf "source-revision\\n"\n')
            env = {
                **os.environ,
                "PATH": f"{stubs}:{os.environ['PATH']}",
                "CARGO_TARGET_DIR": str(base / "target"),
                "TABLEPRO_PROFILE_REPETITIONS": "0",
            }

            for script in ("profile-stream-pin-retention.sh", "profile-stream-export.sh"):
                output = base / script / "nested" / "profile.jsonl"
                result = subprocess.run(
                    ["bash", f"scripts/{script}"],
                    cwd=ROOT,
                    env={**env, "TABLEPRO_PROFILE_OUT": str(output)},
                    capture_output=True,
                    text=True,
                    check=False,
                )

                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(output.read_text().splitlines()[0])["profile_mode"], "metadata")

    @staticmethod
    def write_stub(directory, name, body):
        path = directory / name
        path.write_text(f"#!/bin/sh\n{body}")
        path.chmod(0o755)


if __name__ == "__main__":
    unittest.main()
