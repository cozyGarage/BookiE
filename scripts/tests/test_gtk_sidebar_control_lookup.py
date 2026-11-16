import ast
from pathlib import Path
from types import SimpleNamespace
import unittest


class SidebarControlLookupTests(unittest.TestCase):
    def test_search_controls_are_found_without_traversing_catalog_rows(self):
        source = Path(__file__).resolve().parents[2] / "apps/linux/app/tests/gtk_safety.py"
        tree = ast.parse(source.read_text())
        names = {"sidebar_control_nodes", "find_sidebar_control"}
        functions = [node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name in names]
        self.assertEqual(len(functions), len(names))
        traversed_lists = []

        class Node:
            def __init__(self, name, role, children=()):
                self.name, self.role, self.children = name, role, children

            def __iter__(self):
                if self.role == "list":
                    traversed_lists.append(self)
                return iter(self.children)

        entry = Node("Search tables", "entry")
        root = Node("window", "frame", [Node("catalog", "list"), Node("search", "box", [entry])])
        namespace = {
            "pyatspi": SimpleNamespace(ROLE_LIST="list", ROLE_LIST_ITEM="list-item", ROLE_TABLE="table", ROLE_TREE="tree"),
            "desktop_applications": lambda: [root],
            "node_role": lambda node: node.role,
            "node_name": lambda node: node.name,
            "role_matches": lambda node, role: node.role == role,
        }
        exec(compile(ast.Module(body=functions, type_ignores=[]), str(source), "exec"), namespace)
        self.assertIs(namespace["find_sidebar_control"]("Search tables", "entry"), entry)
        self.assertIsNone(namespace["find_sidebar_control"]("Search tables", "toggle"))
        self.assertEqual(traversed_lists, [])


if __name__ == "__main__":
    unittest.main()
