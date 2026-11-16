from pathlib import Path
import re
import subprocess
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


if __name__ == "__main__":
    unittest.main()
