import json
import sqlite3
import time


def scenarios(ui):
    pyatspi = ui.pyatspi

    def export_bundle(path, passphrase):
        ui.invoke_accessible_action("win.disconnect")
        ui.invoke_accessible_action("welcome.export-bundle")
        ui.set_text_by_name("Passphrase (optional)", passphrase)
        ui.invoke_export_file_chooser_response()
        chooser = ui.wait_for_node(name="Export connections", role=ui.FILE_CHOOSER_ROLES)
        ui.set_visible_editable_within("Export connections", ui.FILE_CHOOSER_ROLES, str(path))
        ui.invoke(ui.wait_within(chooser, name="Save", role=pyatspi.ROLE_PUSH_BUTTON))
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while not path.exists():
            assert time.monotonic() < deadline, "the connection bundle was not exported"
            time.sleep(ui.POLL_SECONDS)

    def remove_from_welcome(name):
        if ui.find_node(name=name, role=pyatspi.ROLE_LIST_ITEM) is None:
            ui.invoke(ui.wait_for_node(name="Open saved connection", role=pyatspi.ROLE_TOGGLE_BUTTON))
        row = ui.wait_for_node(name=name, role=pyatspi.ROLE_LIST_ITEM)
        ui.invoke(ui.wait_within(row, name="Connection actions", role=pyatspi.ROLE_TOGGLE_BUTTON))
        ui.invoke(ui.wait_for_node(name="Remove connection"))
        ui.wait_for_node(name=f"Delete {name}?")
        ui.invoke(ui.wait_for_node(name="Delete", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name=f"Delete {name}?", present=False)

    def import_bundle(path, passphrase):
        ui.choose_import_bundle(path)
        ui.wait_for_node(name="Passphrase", role=ui.PASSPHRASE_ROLES)
        ui.set_text_by_name("Passphrase", passphrase)
        ui.invoke(ui.wait_for_node(name="Unlock", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.invoke(ui.wait_for_node(name="Import", role=pyatspi.ROLE_PUSH_BUTTON))

    def a_reimported_connection_saves_its_workspace_again(_database, base, restart):
        config = base / "config" / ui.storage_dir_name()
        target = next(
            record
            for record in json.loads((config / "connections.json").read_text())["connections"]
            if record["name"] == ui.CONNECTION_B_NAME
        )
        with sqlite3.connect(base / "safety-b.sqlite") as connection:
            connection.execute("CREATE TABLE IF NOT EXISTS kept_items (id INTEGER PRIMARY KEY)")
        bundle, passphrase = base / "home" / "reimport.bundle", "reimport passphrase"
        export_bundle(bundle, passphrase)
        remove_from_welcome(ui.CONNECTION_B_NAME)
        import_bundle(bundle, passphrase)
        ui.wait_for_node(name=ui.CONNECTION_B_NAME)
        ui.open_saved_connection(ui.CONNECTION_B_NAME)
        ui.wait_for_frame_containing(f"{ui.CONNECTION_B_NAME} — BookiE")
        ui.open_table_in_new_tab("kept_items")
        restart()
        state = json.loads((config / "workspace_state.json").read_text())["connections"]
        tabs = state.get(target["id"], {}).get("tabs", [])
        assert tabs, f"workspace not saved after the reimport (target {target['id']}): {state}"

    a_reimported_connection_saves_its_workspace_again.environment = "local"
    a_reimported_connection_saves_its_workspace_again.needs_restart = True
    return [a_reimported_connection_saves_its_workspace_again]
