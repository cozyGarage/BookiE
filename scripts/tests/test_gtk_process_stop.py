import importlib.util
import signal
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("process_stop", ROOT / "apps/linux/app/tests/process_stop.py")
process_stop = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(process_stop)


class FakeProcess:
    def __init__(self, exits_on_term=False, exits_on_kill=False, already_exited=False):
        self.pid = 4242
        self.exits_on_term = exits_on_term
        self.exits_on_kill = exits_on_kill
        self.exited = already_exited
        self.signals = []
        self.killed = False

    def poll(self):
        return 0 if self.exited else None

    def send_signal(self, sent):
        self.signals.append(sent)
        if sent == signal.SIGTERM and self.exits_on_term:
            self.exited = True

    def kill(self):
        self.killed = True
        if self.exits_on_kill:
            self.exited = True

    def wait(self, timeout=None):
        if self.exited:
            return 0
        raise subprocess.TimeoutExpired("bookie", timeout)


class TerminateTests(unittest.TestCase):
    def test_an_application_that_already_exited_gets_no_signal(self):
        process = FakeProcess(already_exited=True)
        process_stop.terminate(process)
        self.assertEqual(process.signals, [])
        self.assertFalse(process.killed)

    def test_an_application_that_exits_on_sigterm_is_not_killed(self):
        process = FakeProcess(exits_on_term=True)
        process_stop.terminate(process)
        self.assertEqual(process.signals, [signal.SIGTERM])
        self.assertFalse(process.killed)

    def test_an_application_that_ignores_sigterm_is_killed(self):
        process = FakeProcess(exits_on_kill=True)
        process_stop.terminate(process)
        self.assertEqual(process.signals, [signal.SIGTERM])
        self.assertTrue(process.killed)

    def test_an_application_that_survives_sigkill_is_a_scenario_failure_not_a_crash(self):
        process = FakeProcess()
        with self.assertRaises(AssertionError) as raised:
            process_stop.terminate(process)
        self.assertTrue(process.killed)
        self.assertIn("4242", str(raised.exception))
        self.assertIn("SIGKILL", str(raised.exception))
        self.assertNotIsInstance(raised.exception, subprocess.TimeoutExpired)


if __name__ == "__main__":
    unittest.main()
