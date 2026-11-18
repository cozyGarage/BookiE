import ast
import json
import os
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[2] / "apps/linux/app/tests/gtk_safety.py"


def load_seed_catalog_hidden_schemas():
    tree = ast.parse(SOURCE.read_text())
    function = next(
        node for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name == "seed_catalog_hidden_schemas"
    )
    namespace = {
        "json": json,
        "os": os,
        "Path": Path,
        "POSTGRES_CONNECTION_ID": "0b6d4a52-3d1a-4f0e-8f6c-5f3f0c2a9e11",
    }
    exec(compile(ast.Module(body=[function], type_ignores=[]), str(SOURCE), "exec"), namespace)
    return namespace["seed_catalog_hidden_schemas"]


def load_postgres_safety_connection_record():
    tree = ast.parse(SOURCE.read_text())
    function = next(
        node for node in tree.body
        if isinstance(node, ast.FunctionDef) and node.name == "postgres_safety_connection_record"
    )
    namespace = {
        "os": os,
        "POSTGRES_CONNECTION_ID": "0b6d4a52-3d1a-4f0e-8f6c-5f3f0c2a9e11",
        "POSTGRES_CONNECTION_NAME": "Safety PostgreSQL",
    }
    exec(compile(ast.Module(body=[function], type_ignores=[]), str(SOURCE), "exec"), namespace)
    return namespace["postgres_safety_connection_record"]


class GtkCatalogProfileStateTests(unittest.TestCase):
    def test_hidden_schemas_are_seeded_for_the_postgres_connection(self):
        seed = load_seed_catalog_hidden_schemas()
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory)
            with patch.dict(
                os.environ,
                {
                    "BOOKIE_GTK_POSTGRES_PORT": "55433",
                    "BOOKIE_GTK_POSTGRES_DB": "catalog_profile",
                    "BOOKIE_GTK_CATALOG_HIDDEN_SCHEMAS_JSON": '["bench_00", "bench_01"]',
                },
                clear=True,
            ):
                seed(config)
            path = config / "workspace_state.json"
            state = json.loads(path.read_text())
            self.assertEqual(
                state["connections"]["0b6d4a52-3d1a-4f0e-8f6c-5f3f0c2a9e11"],
                {"tabs": [], "database": "catalog_profile", "hidden_schemas": ["bench_00", "bench_01"]},
            )
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)

    def test_hidden_schema_seed_requires_postgres_and_a_string_list(self):
        seed = load_seed_catalog_hidden_schemas()
        cases = [
            ({"BOOKIE_GTK_CATALOG_HIDDEN_SCHEMAS_JSON": "[]"}, ValueError),
            (
                {
                    "BOOKIE_GTK_POSTGRES_PORT": "55433",
                    "BOOKIE_GTK_CATALOG_HIDDEN_SCHEMAS_JSON": '["bench_00", 1]',
                },
                ValueError,
            ),
        ]
        for variables, error in cases:
            with self.subTest(variables=variables), tempfile.TemporaryDirectory() as directory:
                with patch.dict(os.environ, variables, clear=True):
                    with self.assertRaises(error):
                        seed(Path(directory))

    def test_postgres_profile_connection_accepts_a_read_only_tls_fixture(self):
        connection = load_postgres_safety_connection_record()
        with patch.dict(
            os.environ,
            {
                "BOOKIE_GTK_POSTGRES_PORT": "5432",
                "BOOKIE_GTK_POSTGRES_HOST": "test.example",
                "BOOKIE_GTK_POSTGRES_DB": "bench",
                "BOOKIE_GTK_POSTGRES_USER": "bench_ro",
                "BOOKIE_GTK_POSTGRES_TLS_MODE": "verify_ca",
                "BOOKIE_GTK_POSTGRES_TLS_CA": "/tmp/bench-ca.pem",
                "BOOKIE_GTK_POSTGRES_READ_ONLY": "true",
            },
            clear=True,
        ):
            record = connection("local")
        self.assertEqual(record["host"], "test.example")
        self.assertEqual(record["database"], "bench")
        self.assertEqual(record["username"], "bench_ro")
        self.assertEqual(record["tls_mode"], "verify_ca")
        self.assertTrue(record["use_tls"])
        self.assertTrue(record["read_only"])
        self.assertEqual(record["tls_root_cert"], "/tmp/bench-ca.pem")
        self.assertNotIn("password", record)

    def test_postgres_profile_connection_keeps_local_defaults_and_rejects_bad_tls(self):
        connection = load_postgres_safety_connection_record()
        with patch.dict(os.environ, {"BOOKIE_GTK_POSTGRES_PORT": "15432"}, clear=True):
            self.assertEqual(connection("local")["database"], "bookie_test")
            self.assertEqual(connection("local")["tls_mode"], "disabled")
        with patch.dict(
            os.environ,
            {"BOOKIE_GTK_POSTGRES_PORT": "15432", "BOOKIE_GTK_POSTGRES_TLS_MODE": "unknown"},
            clear=True,
        ), self.assertRaises(ValueError):
            connection("local")


if __name__ == "__main__":
    unittest.main()
