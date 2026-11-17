from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "run-driver-group.sh"


def group_arms():
    arms = {}
    for line in SCRIPT.read_text().splitlines():
        match = re.match(r"\s+(\w+)\) drivers=\((.*)\) ;;", line)
        if match:
            arms[match.group(1)] = match.group(2).split()
    return arms


def single_drivers():
    match = re.search(r'case "((?: \w+)+ )" in', SCRIPT.read_text())
    return set(match.group(1).split())


class RunDriverGroupTests(unittest.TestCase):
    def test_every_driver_of_a_group_can_also_be_run_alone(self):
        arms = group_arms()
        grouped = {driver for drivers in arms.values() for driver in drivers}
        self.assertEqual(single_drivers(), grouped)
        self.assertEqual(set(arms), {"core", "analytics", "extended"})

    def test_a_single_driver_runs_exactly_that_driver(self):
        self.assertIn('*" $group "*) drivers=("$group") ;;', SCRIPT.read_text())

    def test_an_unknown_name_is_refused_before_anything_runs(self):
        result = subprocess.run(
            ["bash", str(SCRIPT), "nonsense"], cwd=ROOT, capture_output=True, text=True, check=False
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("unknown driver group or driver: nonsense", result.stderr)
        self.assertNotIn("cleanup", result.stdout + result.stderr)

    def test_the_usage_line_names_the_single_driver_form(self):
        result = subprocess.run(["bash", str(SCRIPT)], cwd=ROOT, capture_output=True, text=True, check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("DRIVER", result.stderr)


class ConformanceTargetTests(unittest.TestCase):
    def run_in_scratch_tree(self, driver, with_conformance):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            shutil.copy(SCRIPT, root / "scripts" / "run-driver-group.sh")
            log = root / "commands.log"
            (root / "scripts" / "prepull-test-images.sh").write_text("#!/usr/bin/env bash\nexit 0\n")
            (root / "scripts" / "cleanup-testcontainers.py").write_text("")
            (root / "scripts" / "run-rust-tests.sh").write_text(
                f'#!/usr/bin/env bash\necho "$@" >> "{log}"\n'
            )
            tests = root / "crates" / "drivers" / driver / "tests"
            tests.mkdir(parents=True)
            if with_conformance:
                (tests / "conformance.rs").write_text("")
            result = subprocess.run(
                ["bash", "scripts/run-driver-group.sh", driver], cwd=root, capture_output=True, text=True, check=False
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            return log.read_text().splitlines()

    def test_a_driver_with_a_conformance_file_runs_its_conformance_target(self):
        for driver in ("postgres", "mysql", "mssql", "clickhouse"):
            commands = self.run_in_scratch_tree(driver, with_conformance=True)
            self.assertEqual(len(commands), 1, commands)
            self.assertIn(f"-p bookie-driver-{driver} --test integration --test conformance -- ", commands[0])

    def test_a_driver_without_a_conformance_file_runs_only_its_integration_target(self):
        for driver in ("mongodb", "cassandra", "oracle"):
            commands = self.run_in_scratch_tree(driver, with_conformance=False)
            self.assertIn(f"-p bookie-driver-{driver} --test integration -- ", commands[0])
            self.assertNotIn("conformance", " ".join(commands))

    def test_every_network_driver_conformance_file_is_reachable_through_a_group(self):
        grouped = {driver for drivers in group_arms().values() for driver in drivers}
        for path in (ROOT / "crates" / "drivers").glob("*/tests/conformance.rs"):
            name = path.parts[-3]
            if name in {"shared", "sqlite", "duckdb"}:
                continue
            self.assertIn(name, grouped, f"{path} is not run by any driver group")


if __name__ == "__main__":
    unittest.main()
