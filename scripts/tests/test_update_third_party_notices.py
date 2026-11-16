import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "update-third-party-notices.py"
SPEC = importlib.util.spec_from_file_location("update_third_party_notices", SCRIPT)
NOTICES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NOTICES)


class ThirdPartyNoticeTests(unittest.TestCase):
    def test_generates_sorted_attributed_notices_from_all_dependency_graphs(self):
        with tempfile.TemporaryDirectory() as directory:
            root, config, manifests = self.setup_root(Path(directory))
            inventory = {
                "licenses": [
                    {
                        "id": "MIT",
                        "text": "MIT terms\n",
                        "used_by": [
                            {"crate": {"name": "zeta", "version": "2.0"}},
                            {"crate": {"name": "alpha", "version": "1.0"}},
                        ],
                    },
                    {
                        "id": "Apache-2.0",
                        "text": "Apache terms",
                        "used_by": [{"crate": {"name": "omega", "version": "3.0"}}],
                    },
                ]
            }

            def generate(command, **kwargs):
                return NOTICES.subprocess.CompletedProcess(command, 0, json.dumps(inventory), "")

            with self.patched_inputs(root, config, manifests), patch.object(NOTICES.subprocess, "run", side_effect=generate) as run:
                result = NOTICES.generate()

            self.assertEqual(run.call_count, 3)
            self.assertEqual(run.call_args_list[0].args[0][-2:], ["--features", "kerberos"])
            self.assertEqual(run.call_args_list[1].args[0][-2:], ["--features", "kerberos"])
            self.assertNotIn("--features", run.call_args_list[2].args[0])
            self.assertLess(result.index("## Apache-2.0"), result.index("## MIT"))
            self.assertLess(result.index("`alpha 1.0`"), result.index("`zeta 2.0`"))
            self.assertIn("MIT terms", result)

    def test_rejects_license_allowlist_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root, config, manifests = self.setup_root(Path(directory), deny_allow=["MIT", "BSD-3-Clause"])
            with self.patched_inputs(root, config, manifests), patch.object(NOTICES.subprocess, "run") as run:
                with self.assertRaisesRegex(RuntimeError, "differ from deny.toml"):
                    NOTICES.generate()
            run.assert_not_called()

    def test_normalizes_license_line_endings_before_grouping(self):
        with tempfile.TemporaryDirectory() as directory:
            root, config, manifests = self.setup_root(Path(directory))
            inventory = {
                "licenses": [
                    {
                        "id": "MIT",
                        "text": "MIT terms\r\ncontinued\r\n",
                        "used_by": [{"crate": {"name": "windows", "version": "1.0"}}],
                    },
                    {
                        "id": "MIT",
                        "text": "MIT terms\ncontinued\n",
                        "used_by": [{"crate": {"name": "unix", "version": "1.0"}}],
                    },
                ]
            }
            result = NOTICES.subprocess.CompletedProcess(["cargo-about"], 0, json.dumps(inventory), "")
            with (
                self.patched_inputs(root, config, manifests),
                patch.object(NOTICES.subprocess, "run", return_value=result),
            ):
                generated = NOTICES.generate()

            self.assertEqual(generated.count("## MIT"), 1)
            self.assertIn("- `unix 1.0`", generated)
            self.assertIn("- `windows 1.0`", generated)
            self.assertIn("MIT terms\ncontinued", generated)

    def test_reports_cargo_about_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root, config, manifests = self.setup_root(Path(directory))
            result = NOTICES.subprocess.CompletedProcess(["cargo-about"], 1, "", "offline dependency missing")
            with self.patched_inputs(root, config, manifests), patch.object(NOTICES.subprocess, "run", return_value=result):
                with self.assertRaisesRegex(RuntimeError, "offline dependency missing"):
                    NOTICES.generate()

    def test_rejects_empty_and_incomplete_license_inventories(self):
        for licenses, message in (([], "no license inventory"), ([{"id": "MIT"}], "incomplete license data")):
            with self.subTest(message=message), tempfile.TemporaryDirectory() as directory:
                root, config, manifests = self.setup_root(Path(directory))
                result = NOTICES.subprocess.CompletedProcess(["cargo-about"], 0, json.dumps({"licenses": licenses}), "")
                with self.patched_inputs(root, config, manifests), patch.object(NOTICES.subprocess, "run", return_value=result):
                    with self.assertRaisesRegex(RuntimeError, message):
                        NOTICES.generate()

    def test_rejects_license_entries_without_package_attribution(self):
        with tempfile.TemporaryDirectory() as directory:
            root, config, manifests = self.setup_root(Path(directory))
            inventory = {"licenses": [{"id": "MIT", "text": "MIT terms", "used_by": []}]}
            result = NOTICES.subprocess.CompletedProcess(["cargo-about"], 0, json.dumps(inventory), "")
            with self.patched_inputs(root, config, manifests), patch.object(NOTICES.subprocess, "run", return_value=result):
                with self.assertRaisesRegex(RuntimeError, "has no attributed packages"):
                    NOTICES.generate()

    def test_check_mode_accepts_current_notice_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "notices.md"
            output.write_text("current\n", encoding="utf-8")
            with (
                patch.object(NOTICES, "ROOT", root),
                patch.object(NOTICES, "OUTPUT", output),
                patch.object(NOTICES, "generate", return_value="current\n"),
                patch.object(sys, "argv", [str(SCRIPT), "--check"]),
            ):
                self.assertEqual(NOTICES.main(), 0)

    def check_with_cache(self, argv, generated, marker_exists=False, digest="inputs-1"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "notices.md"
            output.write_text("current\n", encoding="utf-8")
            cache = root / "cache"
            cache.mkdir()
            marker = cache / f"notices-check-{digest}.ok"
            if marker_exists:
                marker.write_text("ok\n")
            with (
                patch.object(NOTICES, "ROOT", root),
                patch.object(NOTICES, "OUTPUT", output),
                patch.object(NOTICES, "cache_directory", return_value=cache),
                patch.object(NOTICES, "input_digest", return_value=digest),
                patch.object(NOTICES, "generate", side_effect=generated) as generate,
                patch.object(sys, "argv", [str(SCRIPT), *argv]),
                contextlib.redirect_stdout(io.StringIO()),
                contextlib.redirect_stderr(io.StringIO()),
            ):
                result = NOTICES.main()
            return result, generate.call_count, marker.exists()

    def test_cached_check_skips_generation_when_the_inputs_already_passed(self):
        result, generated, _ = self.check_with_cache(
            ["--check", "--cached"], AssertionError("must not generate"), marker_exists=True
        )
        self.assertEqual((result, generated), (0, 0))

    def test_cached_check_records_a_pass_and_never_a_failure(self):
        result, generated, recorded = self.check_with_cache(["--check", "--cached"], ["current\n"])
        self.assertEqual((result, generated, recorded), (0, 1, True))
        result, generated, recorded = self.check_with_cache(["--check", "--cached"], ["stale\n"])
        self.assertEqual((result, generated, recorded), (1, 1, False))

    def test_a_check_without_the_cache_flag_ignores_a_recorded_pass(self):
        result, generated, _ = self.check_with_cache(["--check"], ["stale\n"], marker_exists=True)
        self.assertEqual((result, generated), (1, 1))

    def test_the_input_digest_follows_every_file_the_notices_depend_on(self):
        import os
        import subprocess

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            for name, text in {
                "Cargo.lock": "lock 1",
                "Cargo.toml": "manifest 1",
                "about.toml": "about 1",
                "deny.toml": "deny 1",
                "notices.md": "notices 1",
            }.items():
                (root / name).write_text(text)
            (root / "crates").mkdir()
            (root / "crates" / "Cargo.toml").write_text("member 1")
            with (
                patch.object(NOTICES, "ROOT", root),
                patch.object(NOTICES, "CONFIG", root / "about.toml"),
                patch.object(NOTICES, "OUTPUT", root / "notices.md"),
                patch.object(NOTICES, "__file__", str(root / "script.py"), create=True),
                patch.dict(os.environ, {"CARGO_ABOUT": "true"}),
            ):
                (root / "script.py").write_text("script 1")
                baseline = NOTICES.input_digest()
                self.assertEqual(NOTICES.input_digest(), baseline)
                for name in ["Cargo.lock", "crates/Cargo.toml", "about.toml", "deny.toml", "notices.md"]:
                    original = (root / name).read_text()
                    (root / name).write_text(original + " changed")
                    self.assertNotEqual(NOTICES.input_digest(), baseline, name)
                    (root / name).write_text(original)
                self.assertEqual(NOTICES.input_digest(), baseline)

    def test_check_mode_reports_stale_notice_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "notices.md"
            output.write_text("old\n", encoding="utf-8")
            error = io.StringIO()
            with (
                patch.object(NOTICES, "ROOT", root),
                patch.object(NOTICES, "OUTPUT", output),
                patch.object(NOTICES, "generate", return_value="new\n"),
                patch.object(sys, "argv", [str(SCRIPT), "--check"]),
                contextlib.redirect_stderr(error),
            ):
                self.assertEqual(NOTICES.main(), 1)
            self.assertIn("notices.md is stale", error.getvalue())
            self.assertIn("@@", error.getvalue())

    def test_update_mode_writes_notice_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "notices.md"
            with (
                patch.object(NOTICES, "ROOT", root),
                patch.object(NOTICES, "OUTPUT", output),
                patch.object(NOTICES, "generate", return_value="new\n"),
                patch.object(sys, "argv", [str(SCRIPT)]),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                self.assertEqual(NOTICES.main(), 0)
            self.assertEqual(output.read_text(encoding="utf-8"), "new\n")

    def test_generator_errors_return_failure(self):
        error = io.StringIO()
        with (
            patch.object(NOTICES, "generate", side_effect=RuntimeError("bad license data")),
            patch.object(sys, "argv", [str(SCRIPT)]),
            contextlib.redirect_stderr(error),
        ):
            self.assertEqual(NOTICES.main(), 1)
        self.assertIn("bad license data", error.getvalue())

    def setup_root(self, root, deny_allow=None):
        config = root / "about.toml"
        deny = root / "deny.toml"
        config.write_text('accepted = ["MIT", "Apache-2.0"]\n', encoding="utf-8")
        accepted = deny_allow or ["MIT", "Apache-2.0"]
        deny.write_text("[licenses]\nallow = [" + ", ".join(f'"{item}"' for item in accepted) + "]\n", encoding="utf-8")
        manifests = []
        for name in ("app", "agentd", "ssh"):
            manifest = root / f"{name}/Cargo.toml"
            manifest.parent.mkdir()
            manifest.write_text("[package]\nname = \"fixture\"\nversion = \"1.0.0\"\n", encoding="utf-8")
            manifests.append((manifest, ["kerberos"] if name != "ssh" else []))
        return root, config, manifests

    def patched_inputs(self, root, config, manifests):
        from contextlib import ExitStack

        stack = ExitStack()
        stack.enter_context(patch.object(NOTICES, "ROOT", root))
        stack.enter_context(patch.object(NOTICES, "CONFIG", config))
        stack.enter_context(patch.object(NOTICES, "MANIFESTS", manifests))
        return stack


if __name__ == "__main__":
    unittest.main()
