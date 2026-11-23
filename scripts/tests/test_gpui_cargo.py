import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "gpui-cargo.sh"


class GpuiCargoTests(unittest.TestCase):
    def run_wrapper(self, env, args=None):
        with tempfile.TemporaryDirectory() as temporary:
            bin_dir = Path(temporary) / "bin"
            bin_dir.mkdir()
            cargo = bin_dir / "cargo"
            cargo.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' \"$CARGO_TARGET_DIR\" \"$RUSTC_WRAPPER\" "
                "\"$CMAKE_C_COMPILER_LAUNCHER\" \"$CMAKE_CXX_COMPILER_LAUNCHER\"\n"
                "printf 'arg=%s\\n' \"$@\"\n"
            )
            cargo.chmod(0o755)
            sccache = bin_dir / "sccache"
            sccache.write_text("#!/usr/bin/env bash\nexit 0\n")
            sccache.chmod(0o755)
            process_env = {
                key: value
                for key, value in os.environ.items()
                if key not in {
                    "CARGO_TARGET_DIR",
                    "RUSTC_WRAPPER",
                    "CMAKE_C_COMPILER_LAUNCHER",
                    "CMAKE_CXX_COMPILER_LAUNCHER",
                }
            }
            process_env.update(env)
            process_env["PATH"] = f"{bin_dir}:{os.environ['PATH']}"
            result = subprocess.run(
                ["bash", str(SCRIPT), *(args or ["test", "--locked"])],
                cwd=ROOT,
                env=process_env,
                check=True,
                capture_output=True,
                text=True,
            )
            return result.stdout.splitlines(), str(sccache)

    def test_gpui_cargo_shares_the_repo_target_and_enables_optional_compiler_caches(self):
        output, sccache = self.run_wrapper({})
        self.assertEqual(output[:4], [str(ROOT / "target"), sccache, sccache, sccache])
        self.assertIn(f"arg=--manifest-path", output)
        self.assertIn(f"arg={ROOT / 'apps/gpui/Cargo.toml'}", output)

    def test_gpui_cargo_respects_caller_target_and_compiler_cache_settings(self):
        output, _ = self.run_wrapper({
            "CARGO_TARGET_DIR": "/tmp/bookie-target",
            "RUSTC_WRAPPER": "/usr/bin/custom-rust-cache",
            "CMAKE_C_COMPILER_LAUNCHER": "/usr/bin/custom-c-cache",
            "CMAKE_CXX_COMPILER_LAUNCHER": "/usr/bin/custom-cxx-cache",
        })
        self.assertEqual(output[:4], [
            "/tmp/bookie-target",
            "/usr/bin/custom-rust-cache",
            "/usr/bin/custom-c-cache",
            "/usr/bin/custom-cxx-cache",
        ])

    def test_manifest_path_stays_before_test_harness_separator(self):
        output, _ = self.run_wrapper(
            {},
            [
                "test",
                "--locked",
                "--bin",
                "bookie-gpui",
                "filter::case",
                "--",
                "--exact",
                "--test-threads=1",
            ],
        )
        args = [line.removeprefix("arg=") for line in output if line.startswith("arg=")]
        self.assertLess(args.index("--manifest-path"), args.index("--"))
        self.assertEqual(args[-2:], ["--exact", "--test-threads=1"])



if __name__ == "__main__":
    unittest.main()
