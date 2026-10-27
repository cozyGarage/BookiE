import os
import subprocess


def scenarios(ui):
    def open_structure(table):
        row = ui.wait_for_node(name=table, role=ui.pyatspi.ROLE_LIST_ITEM)
        actions = row.queryAction()
        for index in range(actions.nActions):
            if actions.getName(index) == "sidebar-row.edit-structure":
                assert actions.doAction(index), f"Structure did not open for {table}"
                ui.wait_for_node_containing(table.rsplit(".", 1)[-1], role=ui.pyatspi.ROLE_PAGE_TAB)
                return
        raise AssertionError(f"No Structure action for {table}")

    def section(name, add_action):
        ui.invoke(ui.wait_for_node(name=name))
        return ui.wait_for_sensitive_node(add_action)

    def postgres(sql):
        return subprocess.run(
            ["docker", "exec", os.environ["TABLEPRO_GTK_POSTGRES_CONTAINER"], "psql",
             "--username=postgres", "--dbname=bookie_test", "--set=ON_ERROR_STOP=1", "--command", sql],
            check=True, capture_output=True, text=True, timeout=30,
        ).stdout

    def postgres_structure_preserves_empty_and_populated_metadata(_database, _base):
        cleanup = "DROP TABLE IF EXISTS bookie_metadata_child, bookie_metadata_parent, bookie_metadata_empty"
        postgres(cleanup)
        postgres(
            "CREATE TABLE bookie_metadata_empty (id integer); "
            "CREATE TABLE bookie_metadata_parent (id integer PRIMARY KEY); "
            "CREATE TABLE bookie_metadata_child (parent_id integer, "
            "CONSTRAINT bookie_metadata_parent_fk FOREIGN KEY (parent_id) REFERENCES bookie_metadata_parent(id)); "
            "CREATE INDEX bookie_metadata_parent_idx ON bookie_metadata_child(parent_id)"
        )
        try:
            ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
            ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
            open_structure("public.bookie_metadata_empty")
            section("Indexes", "Add Index…")
            ui.wait_for_node(name="Index metadata unavailable", present=False)
            section("Foreign Keys", "Add Foreign Key…")
            ui.wait_for_node(name="Foreign-key metadata unavailable", present=False)
            open_structure("public.bookie_metadata_child")
            section("Indexes", "Add Index…")
            ui.wait_for_node(name="bookie_metadata_parent_idx")
            section("Foreign Keys", "Add Foreign Key…")
            ui.wait_for_node(name="bookie_metadata_parent_fk")
            ui.gtk_ux.capture_grid_navigation_artifact("postgres-structure-foreign-keys")
        finally:
            postgres(cleanup)

    def clickhouse_structure_distinguishes_unavailable_foreign_keys(_database, _base):
        ui.open_saved_connection(ui.CLICKHOUSE_CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.CLICKHOUSE_CONNECTION_NAME} — BookiE")
        open_structure("default.enum_grid")
        section("Indexes", "Add Index…")
        ui.wait_for_node(name="Index metadata unavailable", present=False)
        section("Foreign Keys", "Add Foreign Key…")
        ui.wait_for_node(name="Foreign-key metadata unavailable")
        ui.wait_for_node(name="This driver doesn't expose foreign-key metadata for existing tables.")
        ui.gtk_ux.capture_grid_navigation_artifact("clickhouse-structure-unavailable-foreign-keys")

    result = []
    if os.environ.get("TABLEPRO_GTK_POSTGRES_CONTAINER"):
        result.append(postgres_structure_preserves_empty_and_populated_metadata)
    if os.environ.get("TABLEPRO_GTK_CLICKHOUSE_CONTAINER"):
        result.append(clickhouse_structure_distinguishes_unavailable_foreign_keys)
    for scenario in result:
        scenario.environment = "local"
        scenario.requires_editor = False
    return result
