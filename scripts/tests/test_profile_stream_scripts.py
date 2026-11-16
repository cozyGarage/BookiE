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
            self.write_profile_stubs(stubs)
            env = {
                **os.environ,
                "PATH": f"{stubs}:{os.environ['PATH']}",
                "CARGO_TARGET_DIR": str(base / "target"),
                "BOOKIE_PROFILE_REPETITIONS": "0",
            }

            for script in ("profile-stream-pin-retention.sh", "profile-stream-export.sh"):
                output = base / script / "nested" / "profile.jsonl"
                result = subprocess.run(
                    ["bash", f"scripts/{script}"],
                    cwd=ROOT,
                    env={**env, "BOOKIE_PROFILE_OUT": str(output)},
                    capture_output=True,
                    text=True,
                    check=False,
                )

                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(output.read_text().splitlines()[0])["profile_mode"], "metadata")

    def test_stream_pin_profile_records_json_value_and_wide_row_shape(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            stubs = base / "bin"
            stubs.mkdir()
            self.write_profile_stubs(stubs)
            output = base / "profile.jsonl"
            result = subprocess.run(
                ["bash", "scripts/profile-stream-pin-retention.sh"],
                cwd=ROOT,
                env={
                    **os.environ,
                    "PATH": f"{stubs}:{os.environ['PATH']}",
                    "CARGO_TARGET_DIR": str(base / "target"),
                    "BOOKIE_PROFILE_OUT": str(output),
                    "BOOKIE_PROFILE_REPETITIONS": "0",
                    "BOOKIE_PROFILE_STREAM_VALUE": "json",
                    "BOOKIE_PROFILE_STREAM_WIDTH": "64",
                },
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            metadata = json.loads(output.read_text().splitlines()[0])
            self.assertEqual(metadata["value"], "json")
            self.assertEqual(metadata["width"], 64)

    def test_stream_pin_profile_rejects_invalid_configuration_before_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            stubs = base / "bin"
            stubs.mkdir()
            built = base / "cargo-ran"
            self.write_stub(stubs, "cargo", f": > {built}\n")
            for config in (
                {"BOOKIE_PROFILE_STREAM_VALUE": "xml"},
                {"BOOKIE_PROFILE_STREAM_WIDTH": "65"},
            ):
                result = subprocess.run(
                    ["bash", "scripts/profile-stream-pin-retention.sh"],
                    cwd=ROOT,
                    env={**os.environ, "PATH": f"{stubs}:{os.environ['PATH']}", **config},
                    capture_output=True,
                    text=True,
                    check=False,
                )

                self.assertEqual(result.returncode, 2)
                self.assertFalse(built.exists())

    def write_profile_stubs(self, directory):
        self.write_stub(directory, "cargo", "exit 0\n")
        self.write_stub(directory, "install", 'for dest do :; done\nmkdir -p "$(dirname "$dest")"\n: > "$dest"\n')
        self.write_stub(directory, "sha256sum", 'printf "binary-hash  %s\\n" "$1"\n')
        self.write_stub(directory, "docker", 'printf "postgres-image\\n"\n')
        self.write_stub(directory, "git", 'printf "source-revision\\n"\n')

    @staticmethod
    def write_stub(directory, name, body):
        path = directory / name
        path.write_text(f"#!/bin/sh\n{body}")
        path.chmod(0o755)


if __name__ == "__main__":
    unittest.main()
