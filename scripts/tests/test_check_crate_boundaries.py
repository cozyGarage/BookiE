import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("check_crate_boundaries", ROOT / "scripts/check-crate-boundaries.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def graph(workspace, external, edges):
    ids = {name: f"{name} 0.1.0" for name in [*workspace, *external]}
    deps = {name: [] for name in ids}
    for source, target, kind in edges:
        deps[source].append({"pkg": ids[target], "dep_kinds": [{"kind": kind}]})
    return {
        "workspace_members": [ids[name] for name in workspace],
        "packages": [{"id": ids[name], "name": name} for name in ids],
        "resolve": {"nodes": [{"id": ids[name], "deps": deps[name]} for name in ids]},
    }


BASE = ["bookie-core", "bookie-policy", "bookie-driver-postgres", "bookie-app-services", "bookie-app"]


class CrateBoundaryTests(unittest.TestCase):
    def test_the_documented_direction_passes(self):
        metadata = graph(
            BASE,
            ["gtk4"],
            [
                ("bookie-policy", "bookie-core", None),
                ("bookie-driver-postgres", "bookie-core", None),
                ("bookie-app-services", "bookie-core", None),
                ("bookie-app", "bookie-app-services", None),
                ("bookie-app", "gtk4", None),
            ],
        )
        self.assertEqual(checker.check(metadata), [])

    def test_a_gui_crate_reached_through_another_crate_is_refused(self):
        metadata = graph(
            BASE,
            ["relm4", "glib"],
            [("bookie-app-services", "relm4", None), ("relm4", "glib", None)],
        )
        problems = checker.check(metadata)
        self.assertTrue(any("bookie-app-services" in problem and "glib" in problem for problem in problems), problems)

    def test_a_driver_may_not_depend_on_policy_except_in_tests(self):
        normal = graph(BASE, [], [("bookie-driver-postgres", "bookie-policy", None)])
        self.assertTrue(checker.check(normal))
        development = graph(BASE, [], [("bookie-driver-postgres", "bookie-policy", "dev")])
        self.assertEqual(checker.check(development), [])

    def test_no_crate_may_depend_on_the_application(self):
        for kind in (None, "dev"):
            metadata = graph(BASE, [], [("bookie-policy", "bookie-app", kind)])
            self.assertTrue(checker.check(metadata), kind)

    def test_core_depends_on_no_workspace_crate(self):
        metadata = graph(BASE, [], [("bookie-core", "bookie-policy", None)])
        self.assertTrue(checker.check(metadata))

    def test_a_new_workspace_crate_must_be_classified(self):
        metadata = graph([*BASE, "bookie-newcomer"], [], [])
        problems = checker.check(metadata)
        self.assertTrue(any("bookie-newcomer" in problem for problem in problems), problems)

    def test_an_external_crate_cannot_hide_a_forbidden_edge_behind_a_dev_dependency(self):
        metadata = graph(BASE, ["gtk4"], [("bookie-core", "gtk4", "dev")])
        self.assertTrue(checker.check(metadata))

    def test_headless_crates_refuse_gui_dependencies_of_every_kind(self):
        for source in ("bookie-core", "bookie-app-services", "bookie-driver-postgres"):
            for kind in (None, "build", "dev"):
                with self.subTest(source=source, kind=kind):
                    metadata = graph(BASE, ["gtk4"], [(source, "gtk4", kind)])
                    self.assertTrue(checker.check(metadata))

    def test_a_dev_dependency_cannot_reach_gui_through_its_build_dependencies(self):
        metadata = graph(
            BASE,
            ["helper", "glib"],
            [("bookie-app-services", "helper", "dev"), ("helper", "glib", "build")],
        )
        self.assertTrue(checker.check(metadata))

    def test_a_permitted_workspace_dev_dependency_cannot_reach_gui(self):
        metadata = graph(
            BASE,
            ["gtk4"],
            [("bookie-driver-postgres", "bookie-policy", "dev"), ("bookie-policy", "gtk4", None)],
        )
        problems = checker.check(metadata)
        self.assertTrue(any("bookie-driver-postgres" in problem for problem in problems))

    def test_an_external_dependency_does_not_compile_its_own_dev_dependencies(self):
        for kind in (None, "build", "dev"):
            with self.subTest(kind=kind):
                metadata = graph(
                    BASE,
                    ["helper", "gtk4"],
                    [("bookie-app-services", "helper", kind), ("helper", "gtk4", "dev")],
                )
                self.assertEqual(checker.check(metadata), [])

    def test_a_workspace_dependency_does_not_compile_its_own_tests_for_its_consumer(self):
        metadata = graph(
            BASE,
            ["helper", "glib"],
            [("bookie-driver-postgres", "bookie-policy", "dev"), ("bookie-policy", "helper", None),
             ("helper", "glib", "dev")],
        )
        self.assertEqual(checker.check(metadata), [])

    def test_the_application_may_use_gui_in_each_dependency_kind(self):
        for kind in (None, "build", "dev"):
            with self.subTest(kind=kind):
                metadata = graph(BASE, ["gtk4"], [("bookie-app", "gtk4", kind)])
                self.assertEqual(checker.check(metadata), [])

    def test_gui_reachability_handles_repeated_edges_and_cycles(self):
        metadata = graph(
            BASE,
            ["helper", "other", "gtk4"],
            [("bookie-app-services", "helper", "dev"), ("bookie-app-services", "helper", None),
             ("helper", "other", None), ("other", "helper", "build"), ("other", "gtk4", None)],
        )
        problems = checker.check(metadata)
        self.assertEqual(len(problems), 1)
        self.assertIn("bookie-app-services", problems[0])


class PreflightListTests(unittest.TestCase):
    SCRIPT = "CRATES=(\n  bookie-core\n  bookie-policy\n)\n"

    def test_a_workspace_crate_missing_from_the_preflight_list_is_reported(self):
        metadata = graph(["bookie-core", "bookie-policy", "bookie-app-services"], [], [])
        problems = checker.unlisted_in_preflight(metadata, self.SCRIPT)
        self.assertEqual(problems, ["bookie-app-services is missing from CRATES in scripts/preflight.sh"])

    def test_the_application_and_the_optional_duckdb_driver_are_exempt(self):
        metadata = graph(["bookie-core", "bookie-policy", "bookie-app", "bookie-driver-duckdb"], [], [])
        self.assertEqual(checker.unlisted_in_preflight(metadata, self.SCRIPT), [])


class ExperimentBoundaryTests(unittest.TestCase):
    def manifest(self, dependencies):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        crate = Path(directory.name) / "demo"
        crate.mkdir()
        (crate / "Cargo.toml").write_text(f"[package]\nname = \"demo\"\n\n[dependencies]\n{dependencies}\n")
        return directory.name

    def test_core_results_and_services_are_allowed(self):
        root = self.manifest('bookie-core = { path = "../../crates/core" }\nbookie-results = { path = "x" }')
        self.assertEqual(checker.experiment_problems(root), [])

    def test_a_driver_is_refused(self):
        root = self.manifest('bookie-driver-sqlite = { path = "../../crates/drivers/sqlite" }')
        self.assertEqual(len(checker.experiment_problems(root)), 1)

    def test_a_renamed_dependency_is_refused(self):
        root = self.manifest('db = { package = "bookie-storage", path = "x" }')
        self.assertEqual(len(checker.experiment_problems(root)), 1)

    def test_target_specific_dependencies_cannot_bypass_the_boundary(self):
        for table in ("dependencies", "dev-dependencies", "build-dependencies"):
            with self.subTest(table=table):
                root = self.manifest(
                    f'\n[target.\'cfg(unix)\'.{table}]\ndb = {{ package = "bookie-policy", path = "x" }}'
                )
                self.assertEqual(len(checker.experiment_problems(root)), 1)

    def test_allowed_target_specific_dependencies_remain_allowed(self):
        root = self.manifest('\n[target.\'cfg(windows)\'.dependencies]\nbookie-core = { path = "x" }')
        self.assertEqual(checker.experiment_problems(root), [])

    def test_the_gpui_app_follows_the_same_rule(self):
        root = self.manifest('bookie-driver-sqlite = { path = "x" }')
        empty = tempfile.TemporaryDirectory()
        self.addCleanup(empty.cleanup)
        self.assertEqual(len(checker.experiment_problems(empty.name, Path(root) / "demo")), 1)


if __name__ == "__main__":
    unittest.main()
