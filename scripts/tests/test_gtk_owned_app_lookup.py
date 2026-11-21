import ast
from pathlib import Path
from types import SimpleNamespace
import unittest


SOURCE = Path(__file__).resolve().parents[2] / "apps/linux/app/tests/gtk_safety.py"


class GtkOwnedApplicationLookupTests(unittest.TestCase):
    def load_lookup(self, desktop, owner):
        tree = ast.parse(SOURCE.read_text())
        names = {"desktop_applications", "application_node"}
        functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
        namespace = {
            "pyatspi": SimpleNamespace(Registry=SimpleNamespace(getDesktop=lambda _index: desktop)),
            "_owned_application": owner,
        }
        exec(compile(ast.Module(body=functions, type_ignores=[]), str(SOURCE), "exec"), namespace)
        return namespace

    def test_queries_are_scoped_to_the_process_launched_by_the_harness(self):
        def inaccessible_process():
            raise RuntimeError("stale AT-SPI reference")

        unrelated = SimpleNamespace(get_process_id=lambda: 101)
        owned = SimpleNamespace(get_process_id=lambda: 202)
        stale = SimpleNamespace(get_process_id=inaccessible_process)
        namespace = self.load_lookup([stale, unrelated, owned], SimpleNamespace(process=SimpleNamespace(pid=202)))

        self.assertEqual(namespace["desktop_applications"](), [owned])
        self.assertIs(namespace["application_node"](), owned)

    def test_unowned_diagnostics_can_still_snapshot_the_desktop(self):
        applications = [SimpleNamespace(), SimpleNamespace()]
        namespace = self.load_lookup(applications, None)

        self.assertEqual(namespace["desktop_applications"](), applications)


if __name__ == "__main__":
    unittest.main()
