import importlib.util
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "check-gtk-blocking.py"
SPEC = importlib.util.spec_from_file_location("check_gtk_blocking", SCRIPT)
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class GtkBlockingTests(unittest.TestCase):
    def test_multiline_fully_qualified_sleep_is_rejected(self):
        source = "std :: thread :: sleep (Duration::from_secs(1));"
        self.assertEqual(CHECKER.violations(source), [(1, "std::thread::sleep")])

    def test_module_and_function_aliases_are_rejected(self):
        module_alias = "use std::thread as worker; worker::sleep(Duration::ZERO);"
        function_alias = "use std::thread::sleep as pause; pause(Duration::ZERO);"
        self.assertEqual(CHECKER.violations(module_alias)[0][1], "worker::sleep")
        self.assertEqual(CHECKER.violations(function_alias)[0][1], "pause(...)")

    def test_sleep_import_tree_and_block_on_are_rejected(self):
        sleep_import = "use std::thread::{self, sleep as pause}; pause(Duration::ZERO);"
        block_on = "context.block_on(async { 1 });"
        self.assertEqual(CHECKER.violations(sleep_import)[0][1], "pause(...)")
        self.assertEqual(CHECKER.violations(block_on), [(1, "block_on")])
        alias = "use futures::executor::block_on as wait; wait(async { 1 });"
        self.assertEqual(CHECKER.violations(alias)[0][1], "wait")

    def test_test_only_items_comments_and_strings_are_allowed(self):
        source = '''
            #[cfg(test)]
            mod tests {
                fn wait() { std::thread::sleep(Duration::ZERO); }
            }
            // std::thread::sleep(Duration::ZERO);
            let text = r#"context.block_on(async {})"#;
        '''
        self.assertEqual(CHECKER.violations(source), [])

    def test_owned_relm4_commands_are_allowed(self):
        source = "sender.oneshot_command(|shutdown, _| async move { shutdown.await });"
        self.assertEqual(CHECKER.violations(source), [])


if __name__ == "__main__":
    unittest.main()
