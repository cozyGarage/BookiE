import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NIGHTLY = (ROOT / ".forgejo/workflows/nightly.yml").read_text()
FLOORS = (ROOT / "baselines/coverage-floors-crates.txt").read_text()


def floor_crates():
    return {line.split()[0] for line in FLOORS.splitlines() if line.strip() and not line.startswith("#")}


def coverage_step():
    return NIGHTLY.split("- name: Per-crate line coverage floors\n", 1)[1].split("\n      - name:", 1)[0]


class CoverageFloorWiringTests(unittest.TestCase):
    def test_every_non_driver_floor_is_enforced_by_the_nightly_coverage_job(self):
        enforced = set(re.search(r"--only ((?:crates/[a-z\-]+ ?)+)", coverage_step()).group(1).split())
        non_driver = {crate for crate in floor_crates() if "/drivers/" not in crate}
        self.assertEqual(enforced, non_driver)

    def test_the_measured_packages_are_exactly_the_enforced_crates(self):
        packages = set(re.findall(r"-p (bookie-[a-z\-]+)", coverage_step()))
        enforced = set(re.search(r"--only ((?:crates/[a-z\-]+ ?)+)", coverage_step()).group(1).split())
        self.assertEqual(packages, {"bookie-" + crate.split("/")[-1] for crate in enforced})

    def test_every_driver_floor_is_enforced_by_a_docker_coverage_driver(self):
        drivers = {crate.split("/")[-1] for crate in floor_crates() if "/drivers/" in crate}
        rotation = set(re.findall(r'"(postgres|mysql|sqlite|mongodb|redis|clickhouse)"', (ROOT / "scripts/ci-nightly-plan.py").read_text()))
        self.assertTrue(drivers <= rotation, drivers - rotation)

    def test_the_step_measures_with_the_command_the_floors_were_taken_with(self):
        for flag in ("--lib", "--bins", "--tests", "--no-fail-fast", "--json", "--summary-only"):
            self.assertIn(flag, coverage_step())


if __name__ == "__main__":
    unittest.main()
