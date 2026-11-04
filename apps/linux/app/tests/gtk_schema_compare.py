import os
import subprocess
import time

CURRENT = "a_cmp_current"
DESIRED = "a_cmp_desired"


def scenarios(ui):
    def postgres(sql):
        return subprocess.run(
            ["docker", "exec", os.environ["BOOKIE_GTK_POSTGRES_CONTAINER"], "psql",
             "--username=postgres", "--dbname=bookie_test", "--set=ON_ERROR_STOP=1",
             "--tuples-only", "--no-align", "--command", sql],
            check=True, capture_output=True, text=True, timeout=30,
        ).stdout.strip()

    def invoke_row_action(row_name, action):
        row = ui.wait_for_node(name=row_name, role=ui.pyatspi.ROLE_LIST_ITEM)
        actions = row.queryAction()
        for index in range(actions.nActions):
            if actions.getName(index) == action:
                assert actions.doAction(index), f"{action} did not run for {row_name}"
                return
        names = [actions.getName(index) for index in range(actions.nActions)]
        raise AssertionError(f"{row_name!r} has no {action!r} action: {names!r}")

    def editor_texts():
        texts = []
        for application in ui.desktop_applications():
            for node in ui.descendants(application):
                try:
                    texts.append(node.queryText().getText(0, -1))
                except Exception:
                    continue
        return texts

    def wait_for_editor_text(fragment):
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline:
            for text in editor_texts():
                if fragment in text:
                    return text
            time.sleep(ui.POLL_SECONDS)
        raise AssertionError(f"no editor text contains {fragment!r}")

    def postgres_schema_compare_opens_a_script_and_runs_nothing(_database, _base):
        cleanup = f"DROP SCHEMA IF EXISTS {CURRENT}, {DESIRED} CASCADE"
        postgres(cleanup)
        postgres(
            f"CREATE SCHEMA {CURRENT}; CREATE SCHEMA {DESIRED}; "
            f"CREATE TABLE {CURRENT}.people (id integer PRIMARY KEY); "
            f"CREATE TABLE {DESIRED}.people (id integer PRIMARY KEY, email text); "
            f"CREATE TABLE {DESIRED}.orders (id integer PRIMARY KEY)"
        )
        try:
            ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
            ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
            invoke_row_action(CURRENT, "sidebar-row.compare-schema")
            ui.wait_for_node(name="Compare schema")
            ui.invoke(ui.wait_for_node(name="Compare", role=ui.pyatspi.ROLE_PUSH_BUTTON))

            script = wait_for_editor_text("Schema compare: review before running")
            assert f'CREATE TABLE "{CURRENT}"."orders"' in script, script
            assert f'ALTER TABLE "{CURRENT}"."people" ADD COLUMN "email"' in script, script
            assert f'"{DESIRED}"' not in script, "the script must only touch the schema being changed"
            ui.wait_for_node_containing("1 new, 1 changed and 0 dropped tables")

            tables = postgres(
                f"SELECT string_agg(table_name, ',' ORDER BY table_name) FROM information_schema.tables "
                f"WHERE table_schema = '{CURRENT}'"
            )
            columns = postgres(
                f"SELECT count(*) FROM information_schema.columns WHERE table_schema = '{CURRENT}' AND table_name = 'people'"
            )
            assert tables == "people" and columns == "1", f"opening the script changed the database: {tables!r} {columns!r}"
        finally:
            postgres(cleanup)

    result = []
    if os.environ.get("BOOKIE_GTK_POSTGRES_CONTAINER"):
        result.append(postgres_schema_compare_opens_a_script_and_runs_nothing)
    for scenario in result:
        scenario.environment = "local"
        scenario.requires_editor = False
    return result
