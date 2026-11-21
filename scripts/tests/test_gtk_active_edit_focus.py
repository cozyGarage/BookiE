import ast
from pathlib import Path
from types import SimpleNamespace
import unittest


SOURCE = Path(__file__).resolve().parents[2] / "apps/linux/app/tests/gtk_active_edit.py"


class GtkActiveEditFocusTests(unittest.TestCase):
    def load_predicate(self, state_focused=1, state_showing=2):
        tree = ast.parse(SOURCE.read_text())
        scenarios = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "scenarios")
        predicate = next(node for node in scenarios.body if isinstance(node, ast.FunctionDef) and node.name == "has_visible_focus")
        namespace = {
            "ui": SimpleNamespace(pyatspi=SimpleNamespace(STATE_FOCUSED=state_focused, STATE_SHOWING=state_showing)),
        }
        exec(compile(ast.Module(body=[predicate], type_ignores=[]), str(SOURCE), "exec"), namespace)
        return namespace["has_visible_focus"]

    def test_focus_requires_both_visible_and_focused_state(self):
        class State:
            def __init__(self, values):
                self.values = values

            def contains(self, value):
                return value in self.values

        predicate = self.load_predicate()
        self.assertTrue(predicate(SimpleNamespace(getState=lambda: State({1, 2}))))
        self.assertFalse(predicate(SimpleNamespace(getState=lambda: State({1}))))
        self.assertFalse(predicate(SimpleNamespace(getState=lambda: None)))

    def test_removed_accessibility_nodes_do_not_abort_focus_polling(self):
        def stale_state():
            raise RuntimeError("AT-SPI node was removed")

        self.assertFalse(self.load_predicate()(SimpleNamespace(getState=stale_state)))


if __name__ == "__main__":
    unittest.main()
