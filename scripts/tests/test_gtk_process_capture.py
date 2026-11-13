import ast
import importlib.util
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch


PROCESS_PATH = Path(__file__).resolve().parents[2] / "apps/linux/app/tests/gtk_process.py"
PROCESS_SPEC = importlib.util.spec_from_file_location("gtk_process", PROCESS_PATH)
gtk_process = importlib.util.module_from_spec(PROCESS_SPEC)
PROCESS_SPEC.loader.exec_module(gtk_process)
TRIM_PATH = Path(__file__).resolve().parents[2] / "apps/linux/app/tests/gtk_allocator_trim.py"
TRIM_SPEC = importlib.util.spec_from_file_location("gtk_allocator_trim", TRIM_PATH)
gtk_allocator_trim = importlib.util.module_from_spec(TRIM_SPEC)
TRIM_SPEC.loader.exec_module(gtk_allocator_trim)


class GtkProcessCaptureTests(unittest.TestCase):
    def harness(self, marker):
        source = Path(__file__).resolve().parents[2] / "apps/linux/app/tests/gtk_safety.py"
        tree = ast.parse(source.read_text())
        names = {"start_application", "stop_application", "read_application_stderr"}
        functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]

        def wait_for_window(**_):
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                if marker.exists():
                    return
                time.sleep(0.01)
            raise AssertionError("the child did not finish writing its diagnostics")

        namespace = dict(os=os, subprocess=subprocess, signal=signal, tempfile=tempfile, gtk_process=gtk_process,
                         gtk_allocator_trim=gtk_allocator_trim, _owned_application=None,
                         wait_for_node=wait_for_window, CONNECTION_NAME="fixture", pyatspi=SimpleNamespace(ROLE_FRAME=1))
        exec(compile(ast.Module(body=functions, type_ignores=[]), str(source), "exec"), namespace)
        return namespace

    def application(self, root, exit_code):
        binary = root / "application"
        binary.write_text(
            f"#!{sys.executable}\n"
            "import os\nfrom pathlib import Path\n"
            "payload = b'x' * (256 * 1024)\n"
            "while payload:\n    payload = payload[os.write(2, payload):]\n"
            "Path(os.environ['CAPTURE_MARKER']).touch()\n"
            f"raise SystemExit({exit_code})\n"
        )
        binary.chmod(0o700)
        return binary

    def test_large_stderr_does_not_block_application_start_and_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "ready"
            harness = self.harness(marker)
            process = harness["start_application"](
                self.application(root, 0), {**os.environ, "CAPTURE_MARKER": str(marker)}, restored="empty"
            )
            self.assertIs(harness["_owned_application"].process, process)
            process.wait(timeout=3)
            self.assertEqual(harness["stop_application"](process), "x" * (256 * 1024))
            self.assertTrue(process.stderr.closed)
            self.assertIsNone(harness["_owned_application"])

    def test_nonzero_exit_keeps_diagnostics_and_closes_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "ready"
            harness = self.harness(marker)
            process = harness["start_application"](
                self.application(root, 7), {**os.environ, "CAPTURE_MARKER": str(marker)}, restored="empty"
            )
            process.wait(timeout=3)
            with self.assertRaisesRegex(AssertionError, "application exited with 7: x"):
                harness["stop_application"](process)
            self.assertTrue(process.stderr.closed)

    def test_failed_spawn_closes_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            harness = self.harness(Path(directory) / "ready")
            capture = tempfile.TemporaryFile(mode="w+", encoding="utf-8")
            with patch.object(tempfile, "TemporaryFile", return_value=capture):
                with self.assertRaises(FileNotFoundError):
                    harness["start_application"]("/missing/bookie-fixture", os.environ.copy())
            self.assertTrue(capture.closed)

    def test_missing_window_preserves_diagnostics_and_closes_capture(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "ready"
            harness = self.harness(marker)
            wait_for_window = harness["wait_for_node"]

            def missing_window(**kwargs):
                wait_for_window(**kwargs)
                raise AssertionError("missing window")

            harness["wait_for_node"] = missing_window
            processes = []
            spawn = subprocess.Popen

            def record_process(*args, **kwargs):
                process = spawn(*args, **kwargs)
                processes.append(process)
                return process

            with patch.object(subprocess, "Popen", side_effect=record_process):
                with self.assertRaisesRegex(AssertionError, "expected window: x"):
                    harness["start_application"](
                        self.application(root, 0), {**os.environ, "CAPTURE_MARKER": str(marker)}, restored="empty"
                    )
            self.assertEqual(len(processes), 1)
            self.assertIsNotNone(processes[0].poll())
            self.assertTrue(processes[0].stderr.closed)

    def test_allocator_trim_profile_handles_signal_without_terminating_application(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            ready = root / "ready"
            result = root / "malloc-trim.txt"
            harness = self.harness(ready)
            environment = {
                **os.environ,
                "CAPTURE_MARKER": str(ready),
                "BOOKIE_PROFILE_MALLOC_TRIM_RESULT": str(result),
                "XDG_RUNTIME_DIR": str(root),
            }
            environment.pop("BOOKIE_GTK_APP_LD_PRELOAD", None)
            environment.pop("BOOKIE_GTK_APP_HEAPTRACK_OUTPUT", None)
            binary = root / "application"
            binary.write_text(
                f"#!{sys.executable}\n"
                "import os\nfrom pathlib import Path\nimport time\n"
                "Path(os.environ['CAPTURE_MARKER']).touch()\n"
                "time.sleep(30)\n"
            )
            binary.chmod(0o700)
            process = harness["start_application"](binary, environment, restored="empty")
            self.addCleanup(harness["stop_application"], process)
            os.kill(process.pid, signal.SIGUSR2)
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline and not result.exists():
                time.sleep(0.01)
            self.assertTrue(result.exists())
            self.assertRegex(result.read_text(), r"malloc_trim=[01]\n")
            self.assertIsNone(process.poll())


if __name__ == "__main__":
    unittest.main()
