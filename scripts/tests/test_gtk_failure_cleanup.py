import ast
import json
import os
from pathlib import Path
import shutil
import tempfile
from unittest import TestCase, main
from unittest.mock import patch


class GtkFailureCleanupTests(TestCase):
    def test_snapshot_failure_keeps_original_error_and_stops_process(self):
        source = Path(__file__).resolve().parents[2] / "apps/linux/app/tests/gtk_safety.py"
        tree = ast.parse(source.read_text())
        node = next(item for item in tree.body if isinstance(item, ast.FunctionDef) and item.name == "run_scenario")
        stopped = []
        def snapshot():
            raise ValueError("accessibility unavailable")
        def scenario(_database, _base):
            raise AssertionError("original scenario failure")
        scenario.requires_editor = False
        namespace = dict(Path=Path, tempfile=tempfile, os=os, json=json, shutil=shutil,
                         write_fixture=lambda *_a, **_kw: (None, {}),
                         start_application=lambda *_a: object(),
                         stop_application=lambda process: stopped.append(process) or "",
                         accessible_snapshot=snapshot)
        exec(compile(ast.Module(body=[node], type_ignores=[]), str(source), "exec"), namespace)
        with tempfile.TemporaryDirectory() as artifacts, patch.dict(os.environ, {"BOOKIE_GTK_ARTIFACT_DIR": artifacts}):
            with self.assertRaisesRegex(AssertionError, "original scenario failure"):
                namespace["run_scenario"]("binary", scenario)
        self.assertEqual(len(stopped), 1)
        with tempfile.NamedTemporaryFile() as artifact_file, patch.dict(
            os.environ, {"BOOKIE_GTK_ARTIFACT_DIR": artifact_file.name}
        ):
            with self.assertRaisesRegex(AssertionError, "original scenario failure"):
                namespace["run_scenario"]("binary", scenario)
        self.assertEqual(len(stopped), 2)


if __name__ == "__main__":
    main()
