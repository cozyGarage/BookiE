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
                "\"$CC\" \"$CXX\" "
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
                    "CC",
                    "CXX",
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

    def test_gpui_cargo_uses_stable_git_common_target_and_optional_compiler_caches(self):
        output, sccache = self.run_wrapper({})
        common_dir = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "--path-format=absolute", "--git-common-dir"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        self.assertEqual(
            output[:6],
            [str(Path(common_dir).parent / "target"), sccache, f"{sccache} cc", f"{sccache} c++", sccache, sccache],
        )
        self.assertIn(f"arg=--manifest-path", output)
        self.assertIn(f"arg={ROOT / 'apps/gpui/Cargo.toml'}", output)

    def test_gpui_cargo_respects_caller_target_and_compiler_cache_settings(self):
        output, _ = self.run_wrapper({
            "CARGO_TARGET_DIR": "/tmp/bookie-target",
            "RUSTC_WRAPPER": "/usr/bin/custom-rust-cache",
            "CC": "/usr/bin/custom-c",
            "CXX": "/usr/bin/custom-cxx",
            "CMAKE_C_COMPILER_LAUNCHER": "/usr/bin/custom-c-cache",
            "CMAKE_CXX_COMPILER_LAUNCHER": "/usr/bin/custom-cxx-cache",
        })
        self.assertEqual(output[:6], [
            "/tmp/bookie-target",
            "/usr/bin/custom-rust-cache",
            "/usr/bin/custom-c",
            "/usr/bin/custom-cxx",
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

    def test_target_dir_prints_the_shared_or_explicit_target_directory(self):
        output, _ = self.run_wrapper({}, ["target-dir"])
        common_dir = subprocess.run(
            ["git", "-C", str(ROOT), "rev-parse", "--path-format=absolute", "--git-common-dir"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        self.assertEqual(output, [str(Path(common_dir).parent / "target")])

        output, _ = self.run_wrapper({"CARGO_TARGET_DIR": "/tmp/bookie-target"}, ["target-dir"])
        self.assertEqual(output, ["/tmp/bookie-target"])

    def test_cargo_deny_gets_manifest_path_before_its_subcommand(self):
        output, _ = self.run_wrapper({}, ["deny", "--config", "deny.toml", "check", "advisories", "licenses"])
        args = [line.removeprefix("arg=") for line in output if line.startswith("arg=")]
        self.assertEqual(args, [
            "deny",
            "--manifest-path",
            str(ROOT / "apps/gpui/Cargo.toml"),
            "--config",
            "deny.toml",
            "check",
            "advisories",
            "licenses",
        ])



if __name__ == "__main__":
    unittest.main()
