import contextlib
import importlib.util
import io
import json
import os
import subprocess
import tempfile
import unittest
from collections import Counter
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "ci-nightly-plan.py"
spec = importlib.util.spec_from_file_location("ci_nightly_plan", SCRIPT)
planner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(planner)

COMMIT = "c" * 40
GREEN = "a" * 40
MONDAY, TUESDAY, WEDNESDAY, FRIDAY, SATURDAY, SUNDAY = 0, 1, 2, 4, 5, 6
RUST_CHANGE = ["crates/core/src/lib.rs"]
BASELINES = frozenset({"bookie-agentd", "bookie-app-services", "bookie-transport"})


def plan(weekday, **kwargs):
    kwargs.setdefault("last_green", GREEN)
    kwargs.setdefault("changed", RUST_CHANGE)
    return planner.plan_nightly(weekday, COMMIT, **kwargs)


class RotationTableTests(unittest.TestCase):
    def test_every_driver_runs_exactly_once_across_the_rotating_weekdays(self):
        nights = [driver for day in range(SUNDAY) for driver in planner.DOCKER_ROTATION.get(day, ())]
        self.assertEqual(Counter(nights), Counter(planner.DRIVERS))

    def test_a_driver_night_runs_two_or_three_drivers(self):
        for day, drivers in planner.DOCKER_ROTATION.items():
            self.assertIn(len(drivers), (2, 3), day)

    def test_every_mutation_shard_runs_exactly_once_across_the_rotating_weekdays(self):
        nights = [label for day in range(SUNDAY) for label in planner.MUTATION_ROTATION.get(day, ())]
        self.assertEqual(Counter(nights), Counter(list(planner.MUTATION_SHARDS)))

    def test_rotation_tables_name_only_known_entries_on_known_weekdays(self):
        self.assertTrue(set(planner.DOCKER_ROTATION) <= set(range(SUNDAY)))
        self.assertTrue(set(planner.MUTATION_ROTATION) <= set(range(SUNDAY)))
        for drivers in planner.DOCKER_ROTATION.values():
            self.assertTrue(set(drivers) <= set(planner.DRIVERS))

    def test_the_driver_list_matches_the_workflow_floors(self):
        floors = (planner.ROOT / "baselines/coverage-floors-crates.txt").read_text()
        for driver in planner.DRIVERS:
            self.assertIn(f"crates/drivers/{driver}", floors)

    def test_mutation_shard_counts_cover_each_package_once(self):
        per_package = {}
        for spec_ in planner.MUTATION_SHARDS.values():
            per_package.setdefault(spec_["package"], []).append(spec_["shard"])
        for package, shards in per_package.items():
            total = int(shards[0].split("/")[1])
            self.assertEqual(sorted(shards), sorted(f"{i}/{total}" for i in range(total)), package)

    def test_mutation_directories_exist(self):
        for label, spec_ in planner.MUTATION_SHARDS.items():
            self.assertTrue((planner.ROOT / spec_["dir"]).is_dir(), label)

    def test_every_weekday_fits_its_time_budget(self):
        for day in range(7):
            result = plan(day)
            self.assertLessEqual(result["estimated_worst_case_minutes"], result["budget_minutes"], day)

    def test_the_mutation_baseline_directory_lists_known_packages(self):
        packages = {spec_["package"] for spec_ in planner.MUTATION_SHARDS.values()}
        self.assertTrue(planner.mutation_baselines() <= packages)


class PlanTests(unittest.TestCase):
    def test_coverage_of_the_unit_tier_runs_every_night(self):
        for day in range(7):
            self.assertTrue(plan(day)["coverage"], day)

    def test_weekday_rotation_selects_that_days_drivers_and_shards(self):
        monday = plan(MONDAY)
        self.assertEqual(monday["drivers"], ["postgres", "mysql"])
        self.assertEqual([entry["label"] for entry in monday["mutation"]], ["core-1", "core-2"])
        tuesday = plan(TUESDAY)
        self.assertEqual(tuesday["drivers"], [])
        self.assertEqual([entry["label"] for entry in tuesday["mutation"]], ["core-3", "core-4"])
        self.assertEqual(plan(WEDNESDAY)["drivers"], ["sqlite", "mongodb"])

    def test_sunday_is_a_full_run(self):
        result = plan(SUNDAY)
        self.assertTrue(result["full"])
        self.assertEqual(result["drivers"], list(planner.DRIVERS))
        self.assertEqual({e["label"] for e in result["mutation"]}, set(planner.MUTATION_SHARDS))
        self.assertEqual(result["gtk_soak"], [1, 2, 3])
        self.assertTrue(result["arch_package"])

    def test_weeknights_leave_the_soak_and_package_jobs_for_sunday(self):
        for day in range(SUNDAY):
            result = plan(day)
            self.assertEqual(result["gtk_soak"], [], day)
            self.assertFalse(result["arch_package"], day)

    def test_force_runs_everything_on_a_weekday(self):
        result = plan(TUESDAY, force=True, changed=["README.md"], last_green=COMMIT)
        self.assertFalse(result["skip"])
        self.assertTrue(result["full"])
        self.assertEqual(result["drivers"], list(planner.DRIVERS))
        self.assertEqual(len(result["mutation"]), len(planner.MUTATION_SHARDS))

    def test_sunday_is_never_skipped_for_being_unchanged(self):
        self.assertFalse(plan(SUNDAY, last_green=COMMIT, changed=[])["skip"])

    def test_the_plan_reports_max_parallel_and_a_time_estimate(self):
        result = plan(MONDAY)
        self.assertEqual(result["max_parallel"], planner.MAX_PARALLEL)
        self.assertGreater(result["estimated_worst_case_minutes"], 0)


class SkipTests(unittest.TestCase):
    def test_same_commit_as_the_last_green_nightly_skips(self):
        result = plan(MONDAY, last_green=COMMIT, changed=None)
        self.assertTrue(result["skip"])
        self.assertEqual(result["reason"], "nothing changed since the last green nightly")
        self.assertFalse(result["coverage"])
        self.assertEqual(result["drivers"], [])
        self.assertEqual(result["mutation"], [])

    def test_empty_change_list_with_a_baseline_skips(self):
        self.assertTrue(plan(MONDAY, changed=[])["skip"])

    def test_docs_and_markdown_only_skips(self):
        for changed in (["README.md"], ["docs/a.png", "docs/b.txt"], ["crates/core/NOTES.md", "changelog.d/x.md"]):
            result = plan(MONDAY, changed=changed)
            self.assertTrue(result["skip"], changed)
            self.assertIn("documentation", result["reason"])

    def test_docs_plus_code_does_not_skip(self):
        self.assertFalse(plan(MONDAY, changed=["README.md", "crates/core/src/lib.rs"])["skip"])

    def test_a_workflow_only_change_does_not_skip(self):
        self.assertFalse(plan(MONDAY, changed=[".forgejo/workflows/nightly.yml"])["skip"])

    def test_unknown_last_green_runs_everything_planned_for_today(self):
        result = plan(MONDAY, last_green=None, changed=None)
        self.assertFalse(result["skip"])
        self.assertEqual(result["drivers"], ["postgres", "mysql"])
        self.assertIn("unknown", result["reason"])

    def test_unknown_changes_do_not_skip_and_do_not_limit_mutation_files(self):
        result = plan(3, changed=None, baselines=BASELINES)
        self.assertFalse(result["skip"])
        for entry in result["mutation"]:
            self.assertEqual(entry["files"], "")


class MutationLimitTests(unittest.TestCase):
    def test_a_package_with_a_baseline_is_limited_to_changed_files(self):
        changed = ["crates/agentd/src/a.rs", "crates/agentd/src/b.rs", "crates/core/src/lib.rs"]
        result = plan(3, changed=changed, baselines=BASELINES)
        entries = {entry["label"]: entry for entry in result["mutation"]}
        self.assertEqual(entries["agentd"]["files"], "crates/agentd/src/a.rs crates/agentd/src/b.rs")
        self.assertTrue(entries["agentd"]["partial"])
        self.assertNotIn("app-services", entries)

    def test_a_package_with_a_baseline_and_no_changed_files_is_dropped(self):
        result = plan(3, changed=["crates/core/src/lib.rs"], baselines=BASELINES)
        self.assertEqual(result["mutation"], [])

    def test_a_package_without_a_baseline_keeps_its_rotated_shards(self):
        result = plan(MONDAY, changed=RUST_CHANGE, baselines=BASELINES)
        self.assertEqual([entry["shard"] for entry in result["mutation"]], ["0/4", "1/4"])
        self.assertTrue(all(entry["partial"] for entry in result["mutation"]))

    def test_shards_of_a_limited_package_collapse_to_one_entry(self):
        result = plan(MONDAY, changed=RUST_CHANGE, baselines=BASELINES | {"bookie-core"})
        self.assertEqual(len(result["mutation"]), 1)
        self.assertEqual(result["mutation"][0]["shard"], "0/1")
        self.assertEqual(result["mutation"][0]["files"], "crates/core/src/lib.rs")

    def test_non_rust_files_never_become_mutation_targets(self):
        result = plan(3, changed=["crates/agentd/Cargo.toml"], baselines=BASELINES)
        self.assertEqual(result["mutation"], [])

    def test_full_runs_ignore_the_changed_file_limit(self):
        result = plan(SUNDAY, changed=["crates/agentd/src/a.rs"], baselines=BASELINES)
        self.assertEqual(len(result["mutation"]), len(planner.MUTATION_SHARDS))
        agentd = next(entry for entry in result["mutation"] if entry["label"] == "agentd")
        self.assertEqual(agentd["files"], "")
        self.assertFalse(agentd["partial"])

    def test_postgres_keeps_its_fixed_file_list(self):
        result = plan(SATURDAY, changed=["crates/drivers/postgres/src/array.rs"], baselines=BASELINES)
        postgres = next(entry for entry in result["mutation"] if entry["label"] == "postgres")
        self.assertEqual(postgres["files"].split(), list(planner.POSTGRES_FILES))
        self.assertFalse(postgres["partial"])


class LookupTests(unittest.TestCase):
    API = "http://forgejo.invalid/api/v1/repos/Bookie/bookie"

    def runs(self, *entries):
        return {"workflow_runs": [dict(entry) for entry in entries]}

    def fetcher(self, runs, jobs, seen=None):
        def fetch(url, token):
            if seen is not None:
                seen.append((url, token))
            if "/jobs" in url:
                run_id = int(url.split("/runs/")[1].split("/")[0])
                return jobs[run_id]
            return runs if "page=1" in url else {"workflow_runs": []}

        return fetch

    def test_returns_the_newest_nightly_run_whose_coverage_job_succeeded(self):
        runs = self.runs(
            {"id": 5, "workflow_id": "ci.yml", "commit_sha": "ci"},
            {"id": 4, "workflow_id": "nightly.yml", "commit_sha": "plan-only"},
            {"id": 3, "workflow_id": "nightly.yml", "commit_sha": "real"},
        )
        jobs = {
            4: [{"name": "plan", "status": "success"}, {"name": "coverage", "status": "skipped"}],
            3: [{"name": "plan", "status": "success"}, {"name": "coverage", "status": "success"}],
        }
        self.assertEqual(planner.find_last_green(self.API, None, self.fetcher(runs, jobs)), "real")

    def test_the_request_asks_for_successful_runs_and_sends_the_token(self):
        seen = []
        runs = self.runs({"id": 1, "workflow_id": "nightly.yml", "commit_sha": "x"})
        jobs = {1: {"jobs": [{"name": "coverage", "status": "success"}]}}
        planner.find_last_green(self.API, "secret", self.fetcher(runs, jobs, seen))
        self.assertIn("status=success", seen[0][0])
        self.assertEqual(seen[0][1], "secret")

    def test_no_nightly_run_gives_none(self):
        runs = self.runs({"id": 1, "workflow_id": "ci.yml", "commit_sha": "x"})
        self.assertIsNone(planner.find_last_green(self.API, None, self.fetcher(runs, {})))

    def test_a_failed_lookup_degrades_to_none_without_printing_the_token(self):
        def broken(url, token):
            raise OSError(f"connection refused {token}")

        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            result = planner.lookup_last_green(self.API, {"FORGEJO_TOKEN": "s3cret"}, broken)
        self.assertIsNone(result)
        self.assertNotIn("s3cret", err.getvalue())
        self.assertIn("unavailable", err.getvalue())

    def test_the_token_comes_from_forgejo_token_then_the_workflow_token(self):
        seen = []

        def fetch(url, token):
            seen.append(token)
            return {"workflow_runs": []}

        planner.lookup_last_green(self.API, {"GITHUB_TOKEN": "workflow"}, fetch)
        planner.lookup_last_green(self.API, {"FORGEJO_TOKEN": "own", "GITHUB_TOKEN": "workflow"}, fetch)
        planner.lookup_last_green(self.API, {}, fetch)
        self.assertEqual(seen, ["workflow", "own", None])

    def test_a_lookup_failure_still_yields_todays_rotation(self):
        def broken(url, token):
            raise TimeoutError()

        with contextlib.redirect_stderr(io.StringIO()):
            green = planner.lookup_last_green(self.API, {}, broken)
        result = plan(MONDAY, last_green=green, changed=None)
        self.assertFalse(result["skip"])
        self.assertEqual(result["drivers"], ["postgres", "mysql"])


class CommandLineTests(unittest.TestCase):
    def run_cli(self, *args, env=None):
        merged = {k: v for k, v in os.environ.items() if k not in ("FORGEJO_TOKEN", "GITHUB_TOKEN")}
        merged.update(env or {})
        return subprocess.run(
            ["python3", str(SCRIPT), "--commit", COMMIT, *args],
            capture_output=True,
            text=True,
            env=merged,
        )

    def test_prints_json_for_a_named_weekday(self):
        result = self.run_cli("--weekday", "Monday", "--last-green", GREEN)
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data["weekday"], "Monday")
        self.assertEqual(data["drivers"], ["postgres", "mysql"])

    def test_changed_file_drives_the_docs_only_skip(self):
        with tempfile.TemporaryDirectory() as directory:
            listing = Path(directory) / "changed.txt"
            listing.write_text("docs/a.md\nREADME.md\n")
            result = self.run_cli("--weekday", "Tue", "--last-green", GREEN, "--changed-file", str(listing))
        self.assertTrue(json.loads(result.stdout)["skip"])

    def test_force_flag_and_full_alias_run_everything(self):
        for flag in ("--force", "--full"):
            data = json.loads(self.run_cli("--weekday", "2", "--last-green", COMMIT, flag).stdout)
            self.assertFalse(data["skip"], flag)
            self.assertEqual(data["drivers"], list(planner.DRIVERS), flag)

    def test_a_bad_weekday_is_rejected(self):
        self.assertEqual(self.run_cli("--weekday", "Funday").returncode, 2)

    def test_github_output_carries_skip_and_the_matrices(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "out"
            self.run_cli("--weekday", "Mon", "--last-green", GREEN, "--github-output", str(target))
            lines = dict(line.split("=", 1) for line in target.read_text().splitlines())
        self.assertEqual(lines["skip"], "false")
        self.assertEqual(json.loads(lines["drivers"]), ["postgres", "mysql"])
        self.assertEqual({e["label"] for e in json.loads(lines["mutation"])}, {"core-1", "core-2"})
        self.assertEqual(lines["gtk_soak"], "[]")

    def test_plan_only_stops_every_other_job_but_still_prints_the_plan(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "out"
            result = self.run_cli(
                "--weekday", "Mon", "--last-green", GREEN, "--plan-only", "--github-output", str(target)
            )
            lines = dict(line.split("=", 1) for line in target.read_text().splitlines())
        self.assertEqual(lines["skip"], "true")
        data = json.loads(result.stdout)
        self.assertFalse(data["skip"])
        self.assertTrue(data["plan_only"])
        self.assertEqual(data["drivers"], ["postgres", "mysql"])

    def test_an_unreachable_lookup_degrades_and_never_prints_the_token(self):
        result = self.run_cli(
            "--weekday", "Mon", "--lookup", "--api", "http://127.0.0.1:9/api", env={"FORGEJO_TOKEN": "s3cret-value"}
        )
        self.assertEqual(result.returncode, 0)
        self.assertNotIn("s3cret-value", result.stdout + result.stderr)
        data = json.loads(result.stdout)
        self.assertFalse(data["skip"])
        self.assertEqual(data["drivers"], ["postgres", "mysql"])
        self.assertIn("unknown", data["reason"])

    def test_the_default_weekday_is_a_real_one(self):
        data = json.loads(self.run_cli().stdout)
        self.assertIn(data["weekday"], planner.WEEKDAY_NAMES)


class ChangedFromGitTests(unittest.TestCase):
    def test_lists_changed_paths(self):
        def run(command, **kwargs):
            return subprocess.CompletedProcess(command, 0, stdout="a.rs\n\nb.md\n")

        self.assertEqual(planner.changed_from_git(GREEN, COMMIT, run), ["a.rs", "b.md"])

    def test_a_failed_diff_gives_none(self):
        def run(command, **kwargs):
            raise subprocess.CalledProcessError(128, command)

        with contextlib.redirect_stderr(io.StringIO()):
            self.assertIsNone(planner.changed_from_git(GREEN, COMMIT, run))


if __name__ == "__main__":
    unittest.main()
