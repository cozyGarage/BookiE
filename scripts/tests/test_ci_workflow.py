from pathlib import Path
import configparser
import fnmatch
import importlib.util
import json
import os
import re
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ci_jobs", ROOT / "scripts/check-ci-jobs.py")
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
bounded_spec = importlib.util.spec_from_file_location("bounded_operations", ROOT / "scripts/check-bounded-operations.py")
bounded_checker = importlib.util.module_from_spec(bounded_spec)
bounded_spec.loader.exec_module(bounded_checker)
port_spec = importlib.util.spec_from_file_location("ci_mcp_port", ROOT / "scripts/ci-mcp-port.py")
port_helper = importlib.util.module_from_spec(port_spec)
port_spec.loader.exec_module(port_helper)


class CiWorkflowTests(unittest.TestCase):
    def test_containerized_gtk_job_shares_docker_config_with_image_pulls(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        fast = re.search(r"^  fast:\n(.*?)(?=^  [a-z0-9-]+:\n|\Z)", workflow, re.M | re.S).group(1)
        container = fast.split("    container:\n", 1)[1].split("\n    env:\n", 1)[0]
        action = (ROOT / ".github/actions/dockerhub-login/action.yml").read_text()
        self.assertIn("DOCKER_CONFIG: /github/home/.docker", container)
        self.assertIn('docker login --username "${DOCKERHUB_USERNAME}" --password-stdin', action)

    def test_ci_build_artifacts_strip_symbols(self):
        build = (ROOT / "scripts/build-test-bundles.sh").read_text()
        self.assertIn('export CARGO_PROFILE_TEST_STRIP="${CARGO_PROFILE_TEST_STRIP:-symbols}"', build)
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        gtk_build = re.search(r"(?ms)^  gtk-build:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
        self.assertIn("CARGO_PROFILE_CI_STRIP: symbols", gtk_build)

    def test_forgejo_temporary_files_use_the_persistent_runner_volume(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        self.assertIn("TMPDIR: /var/cache/ci/target\n", workflow)
        self.assertIn("BOOKIE_BUNDLE_CACHE: /var/cache/ci/bundles", workflow)
        for name in ("gtk-build", "clippy", "test-build", "floor-build"):
            with self.subTest(job=name):
                job = re.search(rf"(?ms)^  {name}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
                self.assertIn("CARGO_TARGET_DIR: /target/bookie-${{ github.job }}", job)
                self.assertIn("TMPDIR: /target/tmp-${{ github.run_id }}-${{ github.job }}", job)
                self.assertIn('mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"', job)
                self.assertIn('touch "$CARGO_TARGET_DIR/.bookie-last-used" /target/.bookie-build-cache.lock', job)
                self.assertIn('exec 9>>/target/.bookie-build-cache.lock', job)
                self.assertIn('flock --exclusive 9', job)
                self.assertNotIn('rm -rf -- "$CARGO_TARGET_DIR"', job)
                self.assertIn('cleanup --min-free-gib 30', job)
        supply_chain = re.search(r"(?ms)^  supply-chain:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
        self.assertIn("CARGO_TARGET_DIR: /target/${{ github.run_id }}/${{ github.job }}", supply_chain)
        self.assertIn('rm -rf -- "$CARGO_TARGET_DIR" "$TMPDIR"', supply_chain)
        guards = re.search(r"(?ms)^  guards:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
        self.assertIn("CARGO_TARGET_DIR: /var/cache/ci/target/bookie-guards-${{ github.run_id }}", guards)
        self.assertIn("TMPDIR: /var/cache/ci/tmp/bookie-guards-${{ github.run_id }}", guards)
        self.assertIn('RUSTC_WRAPPER: ""', guards)
        self.assertIn('mkdir -p "$TMPDIR"', guards)

    def test_build_host_jobs_ignore_sccache_server_io_errors(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        for name in ("gtk-build", "clippy", "test-build", "floor-build"):
            with self.subTest(job=name):
                job = re.search(rf"(?ms)^  {name}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
                self.assertIn("SCCACHE_IGNORE_SERVER_IO_ERROR: \"1\"", job)

    def test_postgres_release_temp_directory_stays_short_for_unix_sockets(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        job = re.search(r"(?ms)^  pg-release:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
        self.assertIn("CARGO_TARGET_DIR: /var/cache/ci/target/pg-release\n", job)
        self.assertIn("TMPDIR: /var/cache/ci/tmp\n", job)
        self.assertIn('mkdir -p "$TMPDIR"', job)
        control_path = Path("/var/cache/ci/tmp/bookie.XXXXXX/runtime/bookie/ssh/0123456789abcdef/01234567/control.0123456789abcdef")
        self.assertLess(len(os.fsencode(control_path)), 108)

    def test_postgres_socket_fixture_waits_for_server_readiness(self):
        script = (ROOT / "scripts/test-postgres-socket.sh").read_text()
        self.assertIn('docker exec "$container_name" pg_isready', script)
        self.assertIn("for _ in $(seq 1 60); do", script)

    def test_actionlint_uses_a_read_only_go_module_lock(self):
        workflow = (ROOT / ".github/workflows/linux-ci-contracts.yml").read_text()
        module = (ROOT / "scripts/tools/actionlint/go.mod").read_text()
        sums = ROOT / "scripts/tools/actionlint/go.sum"
        self.assertIn("go-version: 1.25.12", workflow)
        self.assertIn("working-directory: scripts/tools/actionlint", workflow)
        self.assertIn("GOFLAGS: -mod=readonly", workflow)
        self.assertIn("GOTOOLCHAIN: local", workflow)
        self.assertIn("go install github.com/rhysd/actionlint/cmd/actionlint", workflow)
        self.assertNotIn("actionlint/cmd/actionlint@", workflow)
        self.assertIn("go 1.25.12", module)
        self.assertIn("tool github.com/rhysd/actionlint/cmd/actionlint", module)
        self.assertIn("github.com/rhysd/actionlint v1.7.12", module)
        self.assertTrue(sums.is_file())
        self.assertGreater(sums.stat().st_size, 0)
    def test_headless_fixtures_wait_for_cheap_checks_without_serializing_behind_widgets(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        expected = {"guards", "clippy", "test-build"}
        for name in ("driver", "pg-release", "pg-dump", "socket-ssh", "driver-tls"):
            with self.subTest(job=name):
                job = re.search(rf"(?ms)^  {name}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
                dependencies = re.search(r"(?m)^    needs: \[(.+)\]$", job).group(1)
                self.assertEqual({item.strip() for item in dependencies.split(",")}, expected)
        widgets = re.search(r"(?ms)^  widgets:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
        self.assertIn("test-gtk-widgets.sh", widgets)
        self.assertNotRegex(widgets, r"(?m)^    (if|continue-on-error):")

    def test_default_test_jobs_wait_for_the_one_test_bundle_and_duckdb_runs_with_the_drivers(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        default_build = re.search(r"(?ms)^  test-build:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
        self.assertIn('build-test-bundles.sh "$GITHUB_WORKSPACE/test-bundles" default', default_build)
        self.assertNotIn("duckdb-build", workflow)
        self.assertNotRegex(workflow, r"(?m)^  duckdb:\n")
        for name in ("unit", "sandbox", "widgets", "driver", "consumers", "gtk"):
            with self.subTest(job=name):
                job = re.search(rf"(?ms)^  {name}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
                self.assertIn("test-build", re.search(r"(?m)^    needs: \[(.+)\]$", job).group(1))
        driver = re.search(r"(?ms)^  driver:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
        self.assertIn("mongodb, oracle, duckdb]", driver)
        self.assertIn("--test integration --test read_only --test missing_file", driver)
        image_prepull = driver[driver.index("- name: Pre-pull fixture images"):]
        self.assertIn("if: ${{ matrix.driver != 'duckdb' }}", image_prepull)

    def test_native_duckdb_build_jobs_normalize_and_clean_the_shared_compiler_cache(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        for name in ("gtk-build", "clippy", "test-build", "floor-build"):
            with self.subTest(job=name):
                job = re.search(rf"(?ms)^  {name}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
                self.assertIn("RUSTC_WRAPPER: sccache", job)
                self.assertIn("CMAKE_C_COMPILER_LAUNCHER: /tools/sccache", job)
                self.assertIn("CMAKE_CXX_COMPILER_LAUNCHER: /tools/sccache", job)
                self.assertIn("SCCACHE_DIR: /cargo/sccache", job)
                self.assertIn("PATH: /tools:/opt/cargo/bin:/usr/local/bin:/usr/bin:/bin", job)
                self.assertIn("CARGO_TARGET_DIR: /target/bookie-${{ github.job }}", job)
                self.assertIn("TMPDIR: /target/tmp-${{ github.run_id }}-${{ github.job }}", job)
                self.assertIn("SCCACHE_BASEDIRS: /target/bookie-${{ github.job }}", job)
                self.assertIn("SCCACHE_SERVER_UDS: /target/tmp-${{ github.run_id }}-${{ github.job }}/sccache.sock", job)
                self.assertIn("sccache --start-server", job)
                self.assertIn("sccache --show-stats", job)
                self.assertIn("python3 src/scripts/ci-cache.py --root /target", job)
                stop = job.index("sccache --stop-server")
                cleanup = job.index('rm -rf -- "$TMPDIR"')
                self.assertLess(stop, cleanup)
                self.assertNotIn('rm -rf -- "$CARGO_TARGET_DIR"', job)

    def test_long_jobs_keep_artifact_dependencies_when_scheduled_after_fast_checks(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        jobs = (
            "driver", "e2e", "consumers", "gtk",
            "pg-release", "pg-dump", "socket-ssh", "gtk-server", "driver-tls",
            "keyring", "supply-chain",
        )
        for name in jobs:
            with self.subTest(job=name):
                job = re.search(rf"(?ms)^  {name}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
                dependencies = re.search(r"(?m)^    needs: \[(.+)\]$", job).group(1)
                expected = {"guards", "clippy"}
                if name != "supply-chain":
                    expected.add("test-build")
                if name in {"e2e", "gtk", "gtk-server"}:
                    expected.add("gtk-build")
                self.assertEqual({item.strip() for item in dependencies.split(",")}, expected)
        for name in ("unit", "sandbox", "widgets"):
            job = re.search(rf"(?ms)^  {name}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
            self.assertNotRegex(job, r"(?m)^    (if|continue-on-error):")

    def test_server_gtk_jobs_do_not_retry_stateful_scenarios_against_live_fixtures(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        for name in ("e2e", "gtk-server"):
            with self.subTest(job=name):
                job = re.search(rf"(?ms)^  {name}:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow).group(1)
                self.assertIn('BOOKIE_GTK_SCENARIO_RETRIES: "0"', job)

    def test_cargo_path_covers_runner_service_homes(self):
        for name in ("ci", "nightly"):
            workflow = (ROOT / f".forgejo/workflows/{name}.yml").read_text()
            cargo_path = re.search(r"(?m)^  PATH: (.+)$", workflow).group(1)
            self.assertIn("/opt/cargo/bin", cargo_path, name)
            self.assertIn("/home/runner/.cargo/bin", cargo_path, name)
            self.assertIn("/home/ubuntu/.cargo/bin", cargo_path, name)

    def test_acceptance_jobs_use_any_host_and_builder_pools(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        jobs = dict(re.findall(r"(?ms)^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)", workflow))
        self.assertNotIn("ci-pilot", workflow)
        for name in ("gtk-build", "clippy", "test-build", "floor-build", "supply-chain"):
            with self.subTest(job=name):
                self.assertIn("runs-on: build-host", jobs[name])
        for name in (
            "guards", "unit", "sandbox", "widgets", "distro-floor-scenarios", "driver", "e2e",
            "consumers", "gtk", "pg-release", "pg-dump", "socket-ssh", "gtk-server",
            "driver-tls", "keyring",
        ):
            with self.subTest(job=name):
                self.assertIn("runs-on: any-host", jobs[name])

    def test_driver_tls_fixture_wait_is_bounded_with_failure_diagnostics(self):
        script = (ROOT / "scripts/test-driver-tls.sh").read_text()
        self.assertIn("compose up -d --build --wait --wait-timeout 300", script)
        self.assertIn("compose ps --all", script)
        self.assertIn("docker inspect --format '{{json .State.Health}}'", script)
        self.assertIn("compose logs --no-color --tail 50", script)

    def test_forgejo_jobs_bound_runtime_and_scope_checkout_credentials(self):
        for name in ("ci", "nightly"):
            workflow = (ROOT / f".forgejo/workflows/{name}.yml").read_text()
            jobs = workflow.split("\njobs:\n", 1)[1]
            headings = list(re.finditer(r"^  ([A-Za-z0-9_-]+):\s*$", jobs, re.MULTILINE))
            self.assertTrue(headings, name)
            for index, heading in enumerate(headings):
                end = headings[index + 1].start() if index + 1 < len(headings) else len(jobs)
                block = jobs[heading.start() : end]
                self.assertRegex(block, r"(?m)^    timeout-minutes: [1-9][0-9]*$", heading.group(1))

            self.assertNotRegex(workflow, r"https?://oauth2:\$\{TOKEN\}@")
            lines = workflow.splitlines()
            clone_lines = [index for index, line in enumerate(lines) if "git clone --quiet" in line]
            self.assertTrue(clone_lines, name)
            for index in clone_lines:
                setup = "\n".join(lines[max(0, index - 4) : index])
                self.assertIn('GIT_CONFIG_KEY_0="http.${SERVER}/.extraheader"', setup)
                self.assertIn('GIT_CONFIG_VALUE_0="Authorization: Basic $auth"', setup)
                self.assertEqual(lines[index + 1].strip(), "unset TOKEN auth")

    def test_cargo_about_comes_from_the_pinned_installer_and_is_never_built_in_ci(self):
        for path in (
            ".github/workflows/build-linux.yml",
            ".github/workflows/linux-ci-contracts.yml",
            ".forgejo/workflows/ci.yml",
        ):
            with self.subTest(workflow=path):
                self.assertNotIn("cargo install cargo-about", (ROOT / path).read_text())
        harness = (ROOT / "scripts/test-harness.sh").read_text()
        self.assertLess(
            harness.index("scripts/ensure-cargo-about.sh"),
            harness.index("scripts/update-third-party-notices.py --check"),
        )

    def test_forgejo_ci_and_nightly_share_a_per_ref_concurrency_group(self):
        groups = []
        for name in ("ci", "nightly"):
            workflow = (ROOT / f".forgejo/workflows/{name}.yml").read_text()
            match = re.search(r"^concurrency:\n((?:[ \t]+[^\n]*\n)+)", workflow, re.MULTILINE)
            self.assertIsNotNone(match, name)
            settings = dict(line.strip().split(":", 1) for line in match[1].splitlines())
            self.assertEqual(settings["group"].strip(), "bookie-ci-${{ github.ref }}", name)
            self.assertEqual(settings["cancel-in-progress"].strip(), "false", name)
            groups.append(settings["group"].strip())
        self.assertEqual(groups[0], groups[1])
        gate = (ROOT / "scripts/forgejo-gate.sh").read_text()
        self.assertIn('printf \'%s/%s\' "$remote" "$target" | sha256sum', gate)

    def test_forgejo_gtk_jobs_select_distinct_run_scoped_mcp_ports(self):
        ci = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        nightly = (ROOT / ".forgejo/workflows/nightly.yml").read_text()
        for workflow in (ci, nightly):
            self.assertIn("scripts/ci-mcp-port.py", workflow)
        for call in [
            'scripts/ci-mcp-port.py "${{ matrix.shard }}"',
            "scripts/ci-mcp-port.py 4",
            'scripts/ci-mcp-port.py "${{ matrix.mcp_port_slot }}"',
            'scripts/ci-mcp-port.py "${{ matrix.run }}"',
        ]:
            self.assertIn(call, ci + nightly)
        ports = [port_helper.port_for_run(137, slot) for slot in range(1, 17)]
        self.assertEqual(len(ports), len(set(ports)))
        self.assertTrue(all(20000 <= port <= 59999 for port in ports))
        for slot in (0, 17):
            with self.assertRaises(ValueError):
                port_helper.port_for_run(137, slot)

    def test_forgejo_driver_jobs_reuse_and_clean_the_run_scoped_database(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        driver = workflow.split("  driver:\n", 1)[1].split("  consumers:\n", 1)[0]
        self.assertIn("BOOKIE_TEST_RUN_ID: forgejo-${{ github.run_id }}-${{ github.run_attempt }}-${{ matrix.driver }}", driver)
        self.assertIn("cleanup-testcontainers.py", driver)
        self.assertIn("trap cleanup_testcontainers EXIT", driver)
        self.assertIn("--test-threads=1", driver)

    def test_socket_ssh_job_cleans_run_scoped_containers_on_cancel(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        job = workflow.split("  socket-ssh:\n", 1)[1].split("  gtk-server:\n", 1)[0]
        script = (ROOT / "scripts/test-postgres-socket.sh").read_text()
        self.assertIn("BOOKIE_TEST_RUN_ID: forgejo-${{ github.run_id }}-${{ github.run_attempt }}-socket-ssh", job)
        self.assertIn("cleanup-testcontainers.py", job)
        self.assertIn("trap 'cleanup_testcontainers; exit 143' TERM", job)
        self.assertIn("trap 'cleanup_testcontainers; exit 130' INT", job)
        self.assertIn('--label "com.bookie.test-run=$test_run_id"', script)
        self.assertIn("trap 'exit 143' TERM", script)
        self.assertIn("trap 'exit 130' INT", script)

    def test_bounded_operation_guard_catches_multiline_calls(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "fixture.rs"
            source.write_text("connection\n    .query(\"SELECT 1\");\nconn.fetch_columns(None, \"x\");\nconnection.execute_controlled(\"ok\", &control);\n")
            found = list(bounded_checker.unbounded_calls([source]))
        self.assertEqual([row[2] for row in found], ["connection .query(", "conn.fetch_columns("])

    def test_cross_consumer_server_targets_are_gated_and_documented(self):
        local = (ROOT / "scripts/ci-local.sh").read_text()
        for package, target in [
            ("bookie-mcp", "mongodb_extended_json"),
            ("bookie-mcp", "postgres_custom_enum"),
            ("bookie-policy", "session_postgres"),
            ("bookie-policy", "session_mssql"),
        ]:
            self.assertIn(f"-p {package} --test {target} -- --include-ignored --test-threads=1", local)
        registry = json.loads((ROOT / "scripts/isolated-tests.json").read_text())
        self.assertEqual({entry[0] for entry in registry["gtk"]}, {"bookie-app"})
        self.assertEqual(
            {entry[2].split("::")[-1] for entry in registry["app-server"]},
            {
                "postgres_numeric_parser_outputs_round_trip_through_server",
                "postgres_extended_temporal_grid_edits_preserve_native_values",
                "value_contract_postgres_temporal_array_grid_edits_preserve_boundaries_and_siblings",
                "value_contract_postgres_float4_array_grid_edit_preserves_extreme_values_and_sibling",
                "value_contract_postgres_integer_array_grid_edits_preserve_width_and_siblings",
                "value_contract_postgres_text_array_grid_edit_preserves_escaped_values_and_siblings",
                "value_contract_postgres_bit_array_grid_edit_preserves_wire_values_and_sibling",
                "value_contract_postgres_citext_array_grid_edit_preserves_case_nulls_and_sibling",
                "value_contract_clickhouse_enum8_and_enum16_preserve_labels_and_refuse_ambiguous_edits",
                "value_contract_mysql_unparseable_procedure_uses_human_approval_before_execution",
                "value_contract_mongodb_int32_grid_edit_preserves_integer_width",
                "value_contract_mongodb_date_grid_edit_preserves_millisecond_instant",
                "value_contract_mongodb_decimal128_grid_edit_preserves_wide_precision",
                "value_contract_mongodb_nested_document_edit_preserves_extended_bson_and_row_identity",
                "value_contract_mongodb_off_page_type_change_during_census_refuses_edit",
                "value_contract_mongodb_browse_uses_a_bounded_sample_and_page_find",
                "value_contract_mongodb_census_is_not_a_snapshot_for_already_read_documents",
                "value_contract_mongodb_run_find_merges_page_types_and_exports_materialized_values",
                "postgres_value_query_refetches_by_enum_and_domain_composite_key",
                "a_select_that_calls_a_writing_function_runs_outside_a_read_only_cursor",
                "mysql_value_query_refetches_the_exact_blob_for_a_composite_key",
                "mssql_value_query_refetches_the_exact_blob_for_a_composite_key",
                "clickhouse_value_query_refuses_duplicate_sort_keys",
                "mongodb_guarded_refetch_preserves_long_text_and_binary_values",
                "redis_guarded_refetch_preserves_binary_key_and_value",
                "value_contract_mysql_unsigned_integer_grid_edits_refuse_coercion_and_preserve_u64",
                "value_contract_mysql_temporal_parser_keyed_edit_preserves_native_values_and_siblings",
                "value_contract_mysql_enum_set_keyed_edits_preserve_native_values_across_sql_modes",
                "value_contract_mariadb_enum_set_grid_edit_preserves_values_across_sql_modes",
                "value_contract_mysql_spatial_grid_refusal_preserves_native_bytes",
                "value_contract_mysql_bit_parser_edits_preserve_native_values",
                "value_contract_mongodb_collection_wide_mixed_metadata_refuses_edit",
                "value_contract_mssql_legacy_datetime_text_grid_edit_preserves_wire_value_and_siblings",
                "value_contract_mssql_datetimeoffset_grid_edit_preserves_local_time_offset_and_siblings",
                "value_contract_mssql_max_values_survive_connection_session_and_consumers",
                "value_contract_mssql_server_owned_columns_use_native_defaults_across_consumers",
                "value_contract_postgres_csv_import_preserves_explicit_identity_values",
                "a_pinned_spooled_page_reloads_without_retaining_its_payload",
                "evicting_old_stream_pins_releases_their_pages_and_unpin_releases_the_rest",
                "closing_one_of_two_editors_releases_only_its_pinned_and_active_stream_pages",
            },
        )
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        fast = workflow.split("  fast:\n", 1)[1].split("  gtk-safety:\n", 1)[0]
        for required in [
            "run-test-layer.py app-server",
            "/var/run/docker.sock:/var/run/docker.sock",
            "./.github/actions/dockerhub-login",
            "secrets.DOCKERHUB_USERNAME",
            "secrets.DOCKERHUB_TOKEN",
            "credentials:",
        ]:
            self.assertIn(required, fast)
        ledger = subprocess.check_output(["python3", str(ROOT / "scripts/inventory-ignored-tests.py")], text=True)
        for package, target in [
            ("bookie-mcp", "mongodb_extended_json"),
            ("bookie-mcp", "postgres_custom_enum"),
            ("bookie-policy", "session_postgres"),
            ("bookie-policy", "session_mssql"),
        ]:
            self.assertIn(f"-p {package} --test {target}", ledger)
        self.assertIn("scripts/run-test-layer.py app-server", ledger)
        self.assertNotIn("bookie-driver-tests", ledger)
        self.assertNotIn("bookie-driver-src", ledger)

    def test_ignored_arch_source_does_not_hide_new_nonignored_inventory_tests(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / ".gitignore").write_text("apps/linux/packaging/arch/src/\n")
            (root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/core"]\nresolver = "3"\n')
            crate = root / "crates/core"
            (crate / "src").mkdir(parents=True)
            (crate / "Cargo.toml").write_text(
                '[package]\nname = "bookie-core"\nversion = "0.1.0"\nedition = "2024"\n'
            )
            (crate / "src/lib.rs").write_text(
                '#[ignore = "requires Docker"]\nfn tracked_ignored_test() {}\n'
            )
            untracked = crate / "src/new_test.rs"
            untracked.write_text('#[ignore = "requires Docker"]\nfn untracked_ignored_test() {}\n')
            generated = root / "apps/linux/packaging/arch/src/BookiE/apps/linux/app/src/copied.rs"
            generated.parent.mkdir(parents=True)
            generated.write_text('#[ignore = "requires Docker"]\nfn copied_ignored_test() {}\n')
            (root / "scripts").mkdir()
            inventory = root / "scripts/inventory-ignored-tests.py"
            inventory.write_text((ROOT / "scripts/inventory-ignored-tests.py").read_text())
            (root / "scripts/isolated-tests.json").write_text("{}\n")
            subprocess.run(["git", "-C", str(root), "init", "-q"], check=True)
            subprocess.run(
                ["git", "-C", str(root), "add", ".gitignore", "Cargo.toml", "crates", "scripts"],
                check=True,
            )

            result = subprocess.run(
                ["python3", str(inventory)], capture_output=True, text=True, check=False
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("tracked_ignored_test", result.stdout)
            self.assertIn("untracked_ignored_test", result.stdout)
            self.assertNotIn("copied_ignored_test", result.stdout)

    def test_full_gate_enforces_function_size_like_preflight(self):
        script = (ROOT / "scripts/ci-local.sh").read_text()
        full = script.split("run_full() {", 1)[1].split("run_integration()", 1)[0]
        self.assertIn("python3 scripts/check-function-size.py", full)

    def test_full_gate_runs_the_unit_tier_under_nextest_like_forgejo(self):
        script = (ROOT / "scripts/ci-local.sh").read_text()
        full = script.split("run_full() {", 1)[1].split("run_integration()", 1)[0]
        ci = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        selection = "--workspace --lib --bins"
        self.assertIn("--archive-file \"$BOOKIE_TEST_BUNDLE/nextest.tar.zst\"", ci)
        self.assertIn("-E 'kind(lib) | kind(bin)'", ci)
        self.assertIn('"$ROOT/scripts/ensure-nextest.sh"', full)
        self.assertIn(f'nextest run {selection} --profile "${{BOOKIE_NEXTEST_PROFILE:-default}}" --no-tests=fail', full)
        self.assertNotIn("cargo test --workspace", full)

    def test_jobs_share_an_immutable_checkout(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        self.assertNotIn("continue-on-error:", workflow)
        self.assertIn("revision: ${{ steps.pin.outputs.revision }}", workflow)
        self.assertIn("ref: ${{ needs.pin-revision.outputs.revision }}", workflow)
        self.assertNotIn("needs.preflight.outputs.commit", workflow)
        self.assertNotIn("needs.resolve-ref.outputs.commit", workflow)

    def test_github_gtk_job_runs_only_app_unit_tests_after_non_gtk_preflight(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        fast = re.search(r"(?ms)^  fast:\n(.*?)(?=^  [a-z][a-z-]*:\n|\Z)", workflow).group(1)
        preflight = re.search(r"(?ms)^  preflight:\n(.*?)(?=^  [a-z][a-z-]*:\n|\Z)", workflow).group(1)
        self.assertIn("run-test-layer.py quick", preflight)
        self.assertIn("run-test-layer.py gtk-app", fast)
        self.assertNotIn("run-test-layer.py full", fast)
        gate = workflow.split("  regression-gate:\n", 1)[1]
        self.assertNotIn("needs.pin-revision.outputs.revision", gate.split("steps:", 1)[1])
        for line in workflow.splitlines():
            if line.strip().startswith("ref:"):
                self.assertNotIn("github.ref", line)
        quality = (ROOT / ".github/workflows/linux-quality.yml").read_text()
        self.assertEqual(quality.count("ref: ${{ needs.resolve-ref.outputs.commit }}"), 2)

    def test_preflight_and_fast_share_cache_and_run_after_pin_revision(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        preflight = workflow.split("  preflight:\n", 1)[1].split("  fast:\n", 1)[0]
        fast = workflow.split("  fast:\n", 1)[1].split("  gtk-safety:\n", 1)[0]
        ensure = workflow.split("  ensure-ci-gtk-image:\n", 1)[1].split("  preflight:\n", 1)[0]
        self.assertIn("needs: pin-revision", preflight)
        self.assertIn("needs: [pin-revision, ensure-ci-gtk-image]", fast)
        self.assertNotIn("needs: preflight", fast)
        self.assertIn("mozilla-actions/sccache-action@", preflight)
        self.assertIn("mozilla-actions/sccache-action@", fast)
        for job, compile_step in ((preflight, "run-test-layer.py quick"), (fast, "run-test-layer.py change-contracts")):
            cache_step = job.split("- name: Enable sccache for Rust\n", 1)[1].split("      - ", 1)[0]
            self.assertIn("RUSTC_WRAPPER=sccache", cache_step)
            self.assertIn("SCCACHE_GHA_ENABLED=true", cache_step)
            self.assertIn("if: github.event_name != 'workflow_dispatch' || inputs.ref == ''", cache_step)
            self.assertLess(job.index("Enable sccache for Rust"), job.index(compile_step))
        self.assertIn("shared-key: linux-build", preflight)
        self.assertIn("shared-key: linux-build", fast)
        self.assertIn(
            "save-if: ${{ github.event_name == 'pull_request' || (github.event_name == 'push' && github.ref == 'refs/heads/main') }}",
            preflight,
        )
        self.assertIn("persist-credentials: false", preflight)
        self.assertIn("ghcr.io/cozygarage/bookie/ci-debian-testing-gtk", ensure)
        self.assertIn('echo "tag=${tag}" >> "$GITHUB_OUTPUT"', ensure)
        self.assertNotIn('echo "image=', ensure)
        self.assertIn(
            "image: ghcr.io/cozygarage/bookie/ci-debian-testing-gtk:${{ needs.ensure-ci-gtk-image.outputs.tag }}",
            fast,
        )
        self.assertNotIn("needs.ensure-ci-gtk-image.outputs.image", fast)
        self.assertNotIn("prefix-key: linux-preflight", workflow)
        self.assertNotIn("prefix-key: linux-fast-gtk", workflow)

    def test_quality_workflow_only_saves_caches_from_the_scheduled_default_branch(self):
        workflow = (ROOT / ".github/workflows/linux-quality.yml").read_text().splitlines()
        cache_steps = [
            "\n".join(workflow[index : index + 8])
            for index, line in enumerate(workflow)
            if "uses: Swatinem/rust-cache@" in line
        ]
        self.assertEqual(len(cache_steps), 2)
        for step in cache_steps:
            self.assertIn("save-if: ${{ github.event_name == 'schedule' }}", step)

    def test_required_jobs_never_accept_failure_skip_cancel_or_missing(self):
        success = {name: {"result": "success"} for name in checker.REQUIRED | {checker.SCHEDULED}}
        self.assertEqual(checker.assess(success, "push")[0], [])
        for name in checker.REQUIRED:
            for state in ["failure", "skipped", "cancelled", "missing"]:
                results = success | {name: {"result": state}}
                self.assertEqual(checker.assess(results, "push")[0], [name], (name, state))
        self.assertEqual(set(checker.assess({}, "push")[0]), checker.REQUIRED | {checker.SCHEDULED})

    def test_only_pull_request_runs_are_cancelled_when_a_newer_run_starts(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        self.assertIn("cancel-in-progress: ${{ github.event_name == 'pull_request' }}", workflow)
        self.assertNotIn("event_name == 'push' }}", workflow.split("concurrency:", 1)[1].split("jobs:", 1)[0])

    def test_push_and_pull_request_require_every_regression_job(self):
        self.assertEqual(checker.MERGE_ONLY, set())
        self.assertEqual(checker.PULL_REQUEST_ONLY, set())
        success = {name: {"result": "success"} for name in checker.REQUIRED | {checker.SCHEDULED}}
        for name in checker.REQUIRED:
            results = success | {name: {"result": "skipped"}}
            self.assertEqual(checker.assess(results, "pull_request")[0], [name], name)
            self.assertEqual(checker.assess(results, "push")[0], [name], name)
        results = success | {"driver-tls": {"result": "skipped"}}
        self.assertEqual(checker.assess(results, "pull_request")[0], ["driver-tls"])
        self.assertEqual(checker.assess(results, "push")[0], ["driver-tls"])

    def test_only_current_stable_clippy_is_schedule_or_dispatch_only(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        guarded = set()
        for match in re.finditer(r"^  ([a-z0-9-]+):\n(.*?)(?=^  [a-z0-9-]+:|\Z)", workflow, re.M | re.S):
            header = match.group(2).split("\n    steps:\n", 1)[0]
            if re.search(
                r"(?m)^    if: github.event_name == 'schedule' \|\| github.event_name == 'workflow_dispatch'\s*$",
                header,
            ):
                guarded.add(match.group(1))
        self.assertEqual(guarded, {checker.SCHEDULED})
        for name in ["gtk-safety", "integration", "b4-rollback", "driver-tls", "postgres-release"]:
            section = re.search(rf"^  {name}:\n(.*?)(?=^  [a-z0-9-]+:|\Z)", workflow, re.M | re.S).group(1)
            self.assertNotRegex(
                section.split("\n    steps:\n", 1)[0],
                r"(?m)^    if: ",
                name,
            )

    def test_github_docker_hub_jobs_authenticate_when_secrets_exist(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        soak = (ROOT / ".github/workflows/gtk-soak.yml").read_text()
        action = (ROOT / ".github/actions/dockerhub-login/action.yml").read_text()
        self.assertIn("if: inputs.username != ''", action)
        self.assertIn('docker login --username "${DOCKERHUB_USERNAME}" --password-stdin', action)
        self.assertNotIn('auth="$(printf', action)
        self.assertNotIn("echo \"$DOCKERHUB_TOKEN\"", action)
        self.assertNotIn("echo '${{ inputs.password }}'", action)
        fast = re.search(r"^  fast:\n(.*?)(?=^  [a-z0-9-]+:|\Z)", workflow, re.M | re.S).group(1)
        ensure = re.search(r"^  ensure-ci-gtk-image:\n(.*?)(?=^  [a-z0-9-]+:|\Z)", workflow, re.M | re.S).group(1)
        self.assertIn(
            "image: ghcr.io/cozygarage/bookie/ci-debian-testing-gtk:${{ needs.ensure-ci-gtk-image.outputs.tag }}",
            fast,
        )
        self.assertNotIn("needs.ensure-ci-gtk-image.outputs.image", fast)
        self.assertIn("secrets.GITHUB_TOKEN", fast)
        self.assertIn("./.github/actions/dockerhub-login", fast)
        self.assertIn("secrets.DOCKERHUB_USERNAME", fast)
        self.assertIn("secrets.DOCKERHUB_TOKEN", fast)
        self.assertIn("ghcr.io/cozygarage/bookie/ci-debian-testing-gtk", ensure)
        self.assertIn("tag: ${{ steps.publish.outputs.tag }}", ensure)
        self.assertIn("./.github/actions/dockerhub-login", ensure)
        gtk_safety = re.search(r"^  gtk-safety:\n(.*?)(?=^  [a-z0-9-]+:|\Z)", workflow, re.M | re.S).group(1)
        self.assertIn(
            "image: ghcr.io/cozygarage/bookie/ci-debian-testing-gtk:${{ needs.ensure-ci-gtk-image.outputs.tag }}",
            gtk_safety,
        )
        self.assertIn("secrets.GITHUB_TOKEN", gtk_safety)
        for job in ["current-stable-clippy"]:
            section = re.search(rf"^  {job}:\n(.*?)(?=^  [a-z0-9-]+:|\Z)", workflow, re.M | re.S).group(1)
            self.assertIn("image: debian:testing", section, job)
            self.assertIn("secrets.DOCKERHUB_USERNAME", section, job)
            self.assertIn("secrets.DOCKERHUB_TOKEN", section, job)
        for job in ["fast", "integration", "b4-rollback", "driver-tls", "postgres-release"]:
            section = re.search(rf"^  {job}:\n(.*?)(?=^  [a-z0-9-]+:|\Z)", workflow, re.M | re.S).group(1)
            self.assertIn("./.github/actions/dockerhub-login", section, job)
        self.assertIn("secrets.DOCKERHUB_USERNAME", soak)
        self.assertIn("secrets.DOCKERHUB_TOKEN", soak)

    def test_only_push_and_pr_can_skip_current_stable_clippy(self):
        results = {name: {"result": "success"} for name in checker.REQUIRED}
        results[checker.SCHEDULED] = {"result": "skipped"}
        for event in ["push", "pull_request"]:
            self.assertEqual(checker.assess(results, event)[0], [])
        for event in ["schedule", "workflow_dispatch"]:
            self.assertEqual(checker.assess(results, event)[0], [checker.SCHEDULED])

    def test_summary_depends_on_every_required_job_and_always_runs(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        section = workflow.split("  regression-gate:\n", 1)[1]
        self.assertIn("if: always()", section)
        self.assertIn("CI_JOB_RESULTS: ${{ toJSON(needs) }}", section)
        self.assertIn("python3 scripts/check-ci-jobs.py", section)
        for name in checker.REQUIRED | {checker.SCHEDULED}:
            self.assertIn(f"      - {name}\n", section)

    def test_b4_rollback_gate_runs_focused_mysql_and_postgres_tests_before_broad_integration(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        focused = workflow.split("  b4-rollback:\n", 1)[1].split("  driver-tls:\n", 1)[0]
        for required in [
            "prefix-key: linux-integration",
            "run-test-layer.py b4-rollback",
            "regression-b4-rollback-${{ github.run_id }}-${{ github.run_attempt }}",
        ]:
            self.assertIn(required, focused)
        self.assertNotIn(
            "if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'",
            focused.split("\n    steps:\n", 1)[0],
        )
        integration = workflow.split("  integration:\n", 1)[1].split("  b4-rollback:\n", 1)[0]
        self.assertIn("needs: [pin-revision, b4-rollback, ensure-ci-gtk-image, reusable-test-builds]", integration)
        layers = json.loads((ROOT / "scripts/test-layers.json").read_text())["layers"]["b4-rollback"]
        commands = [step["argv"] for step in layers["steps"]]
        self.assertEqual(len(commands), 2)
        self.assertIn("mysql_atomic", commands[0])
        self.assertIn(
            "rollback_failure::a_batch_reports_rollback_failure_after_postgres_terminates_its_backend",
            commands[1],
        )

    def test_mysql_approval_dialog_runs_in_hosted_installed_gtk_acceptance(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        acceptance = workflow.split("  postgres-release:\n", 1)[1].split("  regression-gate:\n", 1)[0]
        self.assertIn('install -Dm755 target/release/bookie-app "$BOOKIE_GTK_BINARY"', acceptance)
        self.assertIn("BOOKIE_GTK_BINARY: target/installed/usr/bin/bookie", acceptance)
        self.assertIn("bash scripts/test-gtk-mysql-approval.sh", acceptance)

    def test_docker_ssh_targets_run_in_hosted_and_local_integration(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        hosted = workflow.split("  integration:\n", 1)[1].split("  b4-rollback:\n", 1)[0]
        local = (ROOT / "scripts/ci-local.sh").read_text().split("run_integration() {", 1)[1].split("run_release()", 1)[0]
        self.assertIn("run-test-layer.py drivers", hosted)
        self.assertIn("scripts/test-ssh.sh", local)
        script = (ROOT / "scripts/test-ssh.sh").read_text()
        for required in ["set -euo pipefail", "--test agent_auth", "--test openssh_session", "--include-ignored", "--test-threads=1"]:
            self.assertIn(required, script)

    def test_postgres_release_openssh_test_is_listed_in_its_actual_tier(self):
        ledger = subprocess.check_output(["python3", str(ROOT / "scripts/inventory-ignored-tests.py")], text=True)
        self.assertIn(
            "agentd_refuses_without_learning_an_unknown_system_openssh_key_then_queries_after_trust](../crates/agentd/tests/g5_system_openssh.rs) | Release",
            ledger,
        )
        self.assertIn("scripts/test-postgres-release.sh", ledger)

    def test_postgres_release_gtk_uses_the_staged_release_binary(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        release = workflow.split("  postgres-release:\n", 1)[1].split("  regression-gate:\n", 1)[0]
        self.assertIn("timeout-minutes: 60", release)
        self.assertIn("cargo build --locked --release -p bookie-app", release)
        self.assertIn('install -Dm755 target/release/bookie-app "$BOOKIE_GTK_BINARY"', release)
        self.assertIn("BOOKIE_GTK_BINARY: target/installed/usr/bin/bookie", release)

    def test_postgres_release_reuses_staged_app_without_debug_rebuild(self):
        script = (ROOT / "scripts/test-postgres-release.sh").read_text()
        self.assertIn('elif [[ -n "${BOOKIE_GTK_BINARY:-}" ]]', script)
        self.assertIn('[[ ! -x "$BOOKIE_GTK_BINARY" ]]', script)
        self.assertIn('cargo build --locked -p bookie-app --bin bookie-app', script)
        self.assertIn('cargo build --locked -p bookie-ssh --bin bookie-askpass', script)
        supplied_binary_branch = script.split('elif [[ -n "${BOOKIE_GTK_BINARY:-}" ]]', 1)[1].split("    else", 1)[0]
        self.assertNotIn('cargo build --locked -p bookie-app --bin bookie-app', supplied_binary_branch)
        layers = json.loads((ROOT / "scripts/test-layers.json").read_text())
        self.assertEqual(layers["layers"]["postgres-release"]["steps"][0]["timeout_seconds"], 2700)

    def test_postgres_release_gtk_smoke_can_find_the_askpass_helper(self):
        script = (ROOT / "scripts/test-postgres-release.sh").read_text()
        self.assertIn('PATH="$BOOKIE_TARGET_DIR/debug:$PATH" \\\n      bash scripts/test-gtk-safety.sh', script)

    def test_gtk_safety_reuses_the_binary_built_by_fast_checks(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        fast = workflow.split("  fast:\n", 1)[1].split("  gtk-safety:\n", 1)[0]
        gtk_safety = workflow.split("  gtk-safety:\n", 1)[1].split("  current-stable-clippy:\n", 1)[0]
        self.assertIn("cargo build --locked -p bookie-app --bin bookie-app", fast)
        self.assertIn("install -Dm755 target/debug/bookie-app target/gtk-safety/bookie", fast)
        self.assertIn("gtk-safety-binary-${{ github.run_id }}-${{ github.run_attempt }}", fast)
        self.assertIn("needs: [pin-revision, ensure-ci-gtk-image, fast]", gtk_safety)
        self.assertIn("image: ghcr.io/cozygarage/bookie/ci-debian-testing-gtk:${{ needs.ensure-ci-gtk-image.outputs.tag }}", gtk_safety)
        self.assertIn("actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131", gtk_safety)
        self.assertIn("chmod 0755 target/gtk-safety/bookie", gtk_safety)
        self.assertIn("BOOKIE_GTK_BINARY: target/gtk-safety/bookie", gtk_safety)

    def test_gtk_safety_installs_pinned_rust_before_keyring_tests(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        gtk_safety = workflow.split("  gtk-safety:\n", 1)[1].split("  current-stable-clippy:\n", 1)[0]
        toolchain_action = "uses: dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772"
        keyring_step = "name: Secret Service round-trip"

        self.assertIn(toolchain_action, gtk_safety)
        self.assertIn("toolchain: 1.98", gtk_safety)
        self.assertLess(gtk_safety.index(toolchain_action), gtk_safety.index(keyring_step))

    def test_postgres_release_runs_on_the_supported_gtk_floor_with_host_docker(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        release = workflow.split("  postgres-release:\n", 1)[1].split("  regression-gate:\n", 1)[0]
        image = (ROOT / "ci/debian-testing-gtk/Dockerfile").read_text()
        self.assertIn("needs: [pin-revision, ensure-ci-gtk-image]", release)
        self.assertIn("--network host", release)
        self.assertIn("bookie-ci:x:%s:%s:CI:/tmp:/usr/sbin/nologin", release)
        self.assertIn("/etc/passwd:ro", release)
        self.assertIn("/var/run/docker.sock:/var/run/docker.sock", release)
        self.assertIn('"$HOME/.docker:/docker"', release)
        for package in ["docker-compose", "gnome-keyring", "libsecret-tools", "libglib2.0-bin", "openssh-client"]:
            with self.subTest(package=package):
                self.assertIn(package, image)
        for floor in [
            "pkg-config --atleast-version=4.24 gtk4",
            "pkg-config --atleast-version=2.88 glib-2.0",
            "pkg-config --atleast-version=1.10 libadwaita-1",
            "pkg-config --atleast-version=5.22 gtksourceview-5",
        ]:
            with self.subTest(floor=floor):
                self.assertIn(floor, release)

    def test_postgres_release_docker_passes_host_vars_used_under_set_u(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        release = workflow.split("  postgres-release:\n", 1)[1].split("  regression-gate:\n", 1)[0]
        self.assertIn('--env GITHUB_WORKSPACE="$GITHUB_WORKSPACE"', release)
        match = re.search(r"bash -euo pipefail -c '([^']*)'", release, re.S)
        self.assertIsNotNone(match)
        inner_vars = sorted(set(re.findall(r"\$([A-Z_][A-Z0-9_]*)", match.group(1))))
        env_names = set(re.findall(r"--env ([A-Z_][A-Z0-9_]*)=", release))
        for name in inner_vars:
            with self.subTest(name=name):
                self.assertIn(name, env_names)

    def test_postgres_release_container_uses_workspace_toolchain(self):
        workflow = (ROOT / ".github/workflows/build-linux.yml").read_text()
        release = workflow.split("  postgres-release:\n", 1)[1].split("  regression-gate:\n", 1)[0]
        toolchain = re.search(r"toolchain: ([^\s]+)", release)
        workspace_toolchain = re.search(r"^channel = \"([^\"]+)\"$", (ROOT / "rust-toolchain.toml").read_text(), re.M)

        self.assertIsNotNone(toolchain)
        self.assertIsNotNone(workspace_toolchain)
        self.assertEqual(toolchain.group(1), workspace_toolchain.group(1))
        self.assertNotRegex(release, r"--env RUSTUP_TOOLCHAIN=")
        self.assertIn('toolchain_bin="$(dirname "$(rustup which cargo --toolchain 1.98)")"', release)
        self.assertIn('toolchain_relative="${toolchain_bin#"$HOME/.rustup"}"', release)
        self.assertIn('--env PATH="/rustup${toolchain_relative}:/cargo/bin:', release)
        self.assertIn('echo "Rust toolchain is outside the mounted Rustup home"', release)
        self.assertIn("cargo --version", release)
        self.assertIn("rustc --version", release)

    def test_gtk_scripts_reduce_xvfb_display_collisions(self):
        for path in [
            "scripts/test-gtk-widgets.sh",
            "scripts/test-gtk-safety.sh",
        ]:
            with self.subTest(path=path):
                script = (ROOT / path).read_text()
                self.assertIn("xvfb_server_num=$((1000 + RANDOM))", script)
                self.assertIn('xvfb-run --auto-servernum --server-num="$xvfb_server_num"', script)
                self.assertNotIn("--auto-display", script)
                self.assertNotIn("xvfb-run -a", script)

    def test_widget_wrapper_reports_xvfb_failure_and_preserves_test_status(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            binaries = directory / "bin"
            binaries.mkdir()
            xdpyinfo_log = directory / "xdpyinfo.log"
            commands = {
                "dbus-run-session": "while [ \"$1\" != -- ]; do shift; done\nshift\nexec \"$@\"\n",
                "xvfb-run": (
                    "error_file=/dev/null\n"
                    "while [ $# -gt 0 ]; do\n"
                    "  case \"$1\" in\n"
                    "    -e) error_file=$2; shift 2 ;;\n"
                    "    -s|--server-num) shift 2 ;;\n"
                    "    --server-num=*) display=${1#*=}; shift ;;\n"
                    "    --auto-servernum) shift ;;\n"
                    "    *) break ;;\n"
                    "  esac\n"
                    "done\n"
                    "printf 'simulated Xvfb startup failure\\n' > \"$error_file\"\n"
                    "DISPLAY=:\"${display:-1234}\" exec \"$@\"\n"
                ),
                "python3": "printf 'simulated GTK initialization failure\\n' >&2\nexit 23\n",
                "xdpyinfo": "printf '%s\\n' \"$*\" >> \"$XDPYINFO_LOG\"\nexit 1\n",
            }
            for name, contents in commands.items():
                executable = binaries / name
                executable.write_text("#!/bin/sh\n" + contents)
                executable.chmod(0o755)
            environment = os.environ.copy()
            environment.update(
                {
                    "PATH": f"{binaries}{os.pathsep}{environment['PATH']}",
                    "XDPYINFO_LOG": str(xdpyinfo_log),
                    "GITHUB_RUN_ID": "unit-run",
                    "GITHUB_RUN_ATTEMPT": "2",
                    "GITHUB_JOB": "widgets",
                }
            )
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/test-gtk-widgets.sh")],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 23, result.stderr)
            self.assertIn("simulated Xvfb startup failure", result.stderr)
            self.assertIn("unit-run", result.stderr)
            self.assertIn("attempt=2", result.stderr)
            self.assertIn("widgets", result.stderr)
            self.assertIn("exit=23", result.stderr)
            self.assertIn("display_alive=unavailable", result.stderr)
            self.assertIn("-display :", xdpyinfo_log.read_text())

    def test_postgres_release_default_covers_b4_ssh_audit_and_reconnect_scenarios(self):
        script = (ROOT / "scripts/test-postgres-release.sh").read_text()
        default_scenarios = next(
            line for line in script.splitlines() if 'BOOKIE_GTK_SCENARIO="${BOOKIE_GTK_SCENARIO:-' in line
        )
        for scenario in [
            "postgres_ssh_unknown_host_key_decline_is_durably_audited",
            "postgres_ssh_multihop_trusts_both_hops_and_queries",
            "postgres_ssh_setup_failure_is_durably_audited",
            "postgres_ssh_tunnel_loss_retires_session_and_reconnects",
            "postgres_ssh_second_hop_decline_does_not_learn_key",
            "postgres_ssh_changed_second_hop_key_is_refused",
        ]:
            with self.subTest(scenario=scenario):
                self.assertIn(scenario, default_scenarios)

    def test_codeql_scans_linux_after_merge_and_weekly_but_not_each_pull_request(self):
        workflow = (ROOT / ".github/workflows/codeql.yml").read_text()
        triggers = workflow.split("\non:\n", 1)[1].split("\npermissions:", 1)[0]
        events = set(re.findall(r"^  ([a-z_]+):", triggers, re.M))
        self.assertEqual(events, {"push", "schedule", "workflow_dispatch"})
        self.assertIn("branches: [main]", triggers.split("  push:", 1)[1])

    def test_analysis_skips_docs_and_reuse_cache(self):
        cancel_push_or_pr = (
            "cancel-in-progress: ${{ github.event_name == 'pull_request' || github.event_name == 'push' }}"
        )
        for name in ["codeql.yml", "sonar-rust.yml"]:
            workflow = (ROOT / ".github/workflows" / name).read_text()
            self.assertIn('"!docs/**"', workflow)
            self.assertIn('"!**.md"', workflow)
            self.assertIn('"**"', workflow)
            self.assertIn(
                cancel_push_or_pr,
                workflow.split("concurrency:", 1)[1].split("jobs:", 1)[0],
            )
        contracts = (ROOT / ".github/workflows/linux-ci-contracts.yml").read_text()
        self.assertIn(
            cancel_push_or_pr,
            contracts.split("concurrency:", 1)[1].split("jobs:", 1)[0],
        )

    def test_sonar_imports_python_tool_coverage_report(self):
        workflow = (ROOT / ".github/workflows/sonar-rust.yml").read_text()
        self.assertIn("coverage==7.10.6", workflow)
        self.assertIn("coverage run -m unittest discover -s scripts/tests", workflow)
        config = configparser.ConfigParser()
        config.read(ROOT / '.coveragerc')
        self.assertEqual(set(config['run']['patch'].split()), {'subprocess', 'execv'})
        self.assertTrue(config['run'].getboolean('branch'))
        self.assertTrue(config['run'].getboolean('parallel'))
        self.assertEqual(config['run']['source'], 'scripts')
        self.assertEqual(config['run']['omit'], 'scripts/tests/*')
        commands = ['coverage erase', 'coverage run -m unittest', 'coverage combine', 'coverage xml']
        positions = [workflow.index(command) for command in commands]
        self.assertEqual(positions, sorted(positions))
        self.assertIn("unittest discover -s scripts/tests", workflow)
        self.assertIn("coverage xml -o target/sonar/python-coverage.xml", workflow)
        self.assertIn("-Dsonar.python.coverage.reportPaths=target/sonar/python-coverage.xml", workflow)

    def test_sonar_excludes_python_test_sources_from_coverage(self):
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        exclusions = workspace["workspace"]["metadata"]["sonar"]["sonar.coverage.exclusions"]
        for path in (ROOT / 'scripts/tests').rglob('test_*.py'):
            relative = path.relative_to(ROOT).as_posix()
            self.assertTrue(any(fnmatch.fnmatchcase(relative, pattern) for pattern in exclusions), relative)
        self.assertFalse(any(fnmatch.fnmatchcase('scripts/ci-cache.py', pattern) for pattern in exclusions))

    def test_sonar_excludes_app_pinning_test_support_from_coverage(self):
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        exclusions = workspace["workspace"]["metadata"]["sonar"]["sonar.coverage.exclusions"]
        self.assertIn("**/pinned_tests.rs", exclusions)

    def test_evidence_harness_fetches_history_for_manifest_blob_checks(self):
        workflow = (ROOT / ".github/workflows/linux-ci-contracts.yml").read_text()
        match = re.search(r"(?ms)^  harness:\n(?P<job>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)", workflow)
        self.assertIsNotNone(match)
        self.assertRegex(match.group("job"), r"(?m)^          fetch-depth: 0$")

    def test_arch_package_container_mounts_the_repository_root(self):
        script = (ROOT / "scripts/test-arch-package-container.sh").read_text()
        self.assertIn('-v "$PWD:/src:ro"', script)
        self.assertNotIn('$PWD/..', script)

    def test_nightly_soak_installs_the_binary_under_usr_bin(self):
        nightly = (ROOT / ".forgejo/workflows/nightly.yml").read_text()
        soak = nightly.split("\n  gtk-soak:\n", 1)[1].split("\n  arch-package:\n", 1)[0]
        self.assertIn('binary="$PWD/target-soak-${{ matrix.run }}/usr/bin/bookie"', soak)

    def test_forgejo_artifact_actions_are_pinned_and_bootstrap_their_runtime(self):
        allowed = {
            "https://code.forgejo.org/actions/upload-artifact@ff15f0306b3f739f7b6fd43fb5d26cd321bd4de5",
            "https://code.forgejo.org/actions/download-artifact@9bc31d5ccc31df68ecc42ccf4149144866c47d8a",
        }
        for workflow in (ROOT / ".forgejo/workflows").glob("*.yml"):
            for job in re.split(r"(?m)^  [a-z0-9-]+:\n", workflow.read_text().split("jobs:\n", 1)[1]):
                actions = re.findall(r"(?m)^        uses: (\S+)$", job)
                if not actions:
                    continue
                self.assertTrue(set(actions) <= allowed, workflow.name)
                self.assertLess(job.index("ensure-node.sh"), job.index("uses:"), workflow.name)
                self.assertIn('>> "$GITHUB_PATH"', job)

    def test_forgejo_distro_floor_keeps_runtime_coverage_without_deb_release_lifecycle(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        self.assertNotIn("\n  distro-floor:\n", workflow)
        self.assertNotIn("tablepro_0.1.5-1_amd64.deb", workflow)
        scenarios = workflow.split("\n  distro-floor-scenarios:\n", 1)[1].split("\n  driver:\n", 1)[0]
        producer = workflow.split("\n  floor-build:\n", 1)[1].split("\n  distro-floor-scenarios:\n", 1)[0]
        self.assertIn("runs-on: build-host", producer)
        self.assertIn("scripts/build-deb.sh", producer)
        self.assertNotIn("scripts/test-deb-package.sh", workflow)
        self.assertIn("needs: [floor-build]", scenarios)
        self.assertIn("ci-gtk-artifact.py create --package", producer)
        self.assertIn("ci-gtk-artifact.py verify --package", scenarios)
        builder = (ROOT / "scripts/distro-floor-package.sh").read_text()
        self.assertIn("BASE=debian:testing", builder)
        self.assertIn("scripts/build-deb.sh", builder)
        self.assertIn("flock", builder)
        self.assertIn('test -e "/probe/', builder)
        self.assertIn('.floor-package', builder)
        self.assertLess(builder.index("flock 9"), builder.index("docker build -q"))
        self.assertIn("chown", builder)
        floor_script = (ROOT / "scripts/test-distro-floor.sh").read_text()
        self.assertIn('docker build -t "$tag"', floor_script)
        self.assertIn("apt-get update\n", floor_script)
        self.assertIn('cd /package\n      apt-get install -y "./$DISTRO_FLOOR_DEB_NAME"', floor_script)
        self.assertLess(floor_script.index('if [ -n "${DISTRO_FLOOR_DEB_NAME:-}" ]'), floor_script.index("pkg-config --modversion"))
        gtk_safety_script = (ROOT / "scripts/test-gtk-safety.sh").read_text()
        self.assertIn('timeout 1500s "$system_python" -u "$ROOT/apps/linux/app/tests/gtk_safety.py"', gtk_safety_script)
        runtime_image = (ROOT / "scripts/distro-floor/Dockerfile.runtime").read_text()
        self.assertIn("python3-pyatspi", runtime_image)
        self.assertIn("gir1.2-gtk-4.0", runtime_image)
        self.assertNotIn("rustup", runtime_image)

    def test_gtk_safety_uses_private_atspi_activation(self):
        script = (ROOT / "scripts/test-gtk-safety.sh").read_text()
        self.assertIn('XDG_DATA_HOME="$gtk_runtime/data"', script)
        self.assertIn("org.a11y.Bus.service", script)
        self.assertIn('cp "/usr/share/dbus-1/services/org.a11y.Bus.service"', script)
        self.assertIn('"$gtk_runtime/data/dbus-1/services/org.a11y.Bus.service"', script)
        self.assertLess(script.index("xvfb-run --auto-servernum"), script.index('dbus-run-session -- "$ROOT/scripts/test-gtk-safety.sh"'))

    def test_distro_package_builder_uses_a_distinct_image_tag(self):
        builder = (ROOT / "scripts/distro-floor-package.sh").read_text()
        self.assertIn("image=bookie-floor-package-debian-testing", builder)
        self.assertIn('docker build -q -t "$image"', builder)
        self.assertIn('"$image" bash -ceu', builder)

    def test_distro_scenarios_share_the_package_and_partition_all_six_shards(self):
        workflow = (ROOT / ".forgejo/workflows/ci.yml").read_text()
        scenarios = workflow.split("\n  distro-floor-scenarios:\n", 1)[1].split("\n  driver:\n", 1)[0]
        builder = workflow.split("\n  floor-build:\n", 1)[1].split("\n  distro-floor-scenarios:\n", 1)[0]
        self.assertIn("ci-gtk-artifact.py create --package", builder)
        self.assertIn("needs: [floor-build]", scenarios)
        self.assertIn("max-parallel: 2", scenarios)
        self.assertIn("shard_group: [1, 2]", scenarios)
        self.assertIn("ci-gtk-artifact.py verify --package", scenarios)
        self.assertIn('SHARD_GROUP: ${{ matrix.shard_group }}', scenarios)
        self.assertIn('for shard in "$SHARD_GROUP" "$((SHARD_GROUP + 2))" "$((SHARD_GROUP + 4))"; do', scenarios)
        self.assertIn('BOOKIE_GTK_SHARD="${shard}/6" bash scripts/test-distro-floor.sh debian:testing', scenarios)
        self.assertIn('scripts/ci-mcp-port.py "$((10 + shard))"', scenarios)
        self.assertNotIn("scripts/distro-floor-package.sh", scenarios)
        self.assertIn("scripts/test-distro-floor.sh debian:testing", scenarios)
        for job in (builder, scenarios):
            self.assertIn("name: floor-package-${{ github.run_id }}-${{ github.sha }}", job)

    def test_packaged_distro_smoke_does_not_mount_a_stale_build_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            binaries = directory / "bin"
            binaries.mkdir()
            docker_log = directory / "docker-args"
            docker = binaries / "docker"
            docker.write_text(
                "#!/bin/sh\nprintf '%s\\0' \"$@\" >> \"$DOCKER_ARG_LOG\"\nprintf '\\n' >> \"$DOCKER_ARG_LOG\"\n"
            )
            docker.chmod(0o755)
            package = directory / "bookie.deb"
            package.write_bytes(b"package")
            environment = os.environ.copy()
            environment["PATH"] = f"{binaries}{os.pathsep}{environment['PATH']}"
            environment["DOCKER_ARG_LOG"] = str(docker_log)
            environment["DISTRO_FLOOR_PACKAGE"] = str(package)

            result = subprocess.run(
                ["bash", str(ROOT / "scripts/test-distro-floor.sh"), "debian:testing"],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            calls = docker_log.read_bytes().splitlines()
            self.assertTrue(calls)
            self.assertFalse(any(b":/target" in call for call in calls))

    def test_distro_runtime_retries_a_docker_hub_rate_limit(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            binaries = directory / "bin"
            binaries.mkdir()
            docker_count = directory / "docker-count"
            docker = binaries / "docker"
            docker.write_text(
                "#!/bin/sh\n"
                "if [ \"$1\" = build ]; then\n"
                "  count=$(cat \"$DOCKER_COUNT_FILE\" 2>/dev/null || printf 0)\n"
                "  count=$((count + 1))\n"
                "  printf '%s' \"$count\" > \"$DOCKER_COUNT_FILE\"\n"
                "  if [ \"$count\" -eq 1 ]; then\n"
                "    echo '429 Too Many Requests'\n"
                "    exit 1\n"
                "  fi\n"
                "fi\n"
            )
            docker.chmod(0o755)
            sleep = binaries / "sleep"
            sleep.write_text("#!/bin/sh\nexit 0\n")
            sleep.chmod(0o755)
            package = directory / "bookie.deb"
            package.write_bytes(b"package")
            environment = os.environ.copy()
            environment["PATH"] = f"{binaries}{os.pathsep}{environment['PATH']}"
            environment["DOCKER_COUNT_FILE"] = str(docker_count)
            environment["DISTRO_FLOOR_PACKAGE"] = str(package)

            result = subprocess.run(
                ["bash", str(ROOT / "scripts/test-distro-floor.sh"), "debian:testing"],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(docker_count.read_text(), "2")


if __name__ == "__main__":
    unittest.main()
