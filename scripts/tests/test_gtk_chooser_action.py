import ast
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock


class ChooserActionTests(unittest.TestCase):
    def harness(self, actions):
        source = Path(__file__).resolve().parents[2] / 'apps/linux/app/tests/gtk_safety.py'
        tree = ast.parse(source.read_text())
        functions = [node for node in tree.body if isinstance(node, ast.FunctionDef)
                     and node.name == 'show_file_chooser_location']
        fallback = Mock()
        namespace = dict(
            descendants=lambda _: [SimpleNamespace(queryAction=lambda: actions)],
            time=SimpleNamespace(monotonic=Mock(side_effect=[0, 0, 1, 3]), sleep=Mock()),
            POLL_SECONDS=0.01, press_x11_key=fallback,
        )
        exec(compile(ast.Module(body=functions, type_ignores=[]), str(source), 'exec'), namespace)
        return namespace['show_file_chooser_location'], fallback

    def test_disappearing_action_is_retried(self):
        action = SimpleNamespace(nActions=1, getName=lambda _: 'show_location',
                                 doAction=Mock(side_effect=[RuntimeError('action disappeared'), True]))
        show, fallback = self.harness(action)
        show(object())
        self.assertEqual(action.doAction.call_count, 2)
        fallback.assert_not_called()

    def test_disappearing_action_falls_back_to_keyboard(self):
        action = SimpleNamespace(nActions=1, getName=lambda _: 'show_location',
                                 doAction=Mock(side_effect=RuntimeError('action disappeared')))
        show, fallback = self.harness(action)
        show(object())
        fallback.assert_called_once_with('l', ('Control_L',))

    def test_available_action_needs_no_keyboard_fallback(self):
        action = SimpleNamespace(nActions=1, getName=lambda _: 'show_location', doAction=Mock(return_value=True))
        show, fallback = self.harness(action)
        show(object())
        action.doAction.assert_called_once_with(0)
        fallback.assert_not_called()


if __name__ == '__main__':
    unittest.main()
