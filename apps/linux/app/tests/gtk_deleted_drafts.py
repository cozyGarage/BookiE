import json
from pathlib import Path


def scenarios(ui):
    pyatspi = ui.pyatspi

    def remove_through_the_gui(name):
        if ui.find_node(name=name, role=pyatspi.ROLE_LIST_ITEM) is None:
            ui.invoke(ui.wait_for_node(name="Open saved connection", role=pyatspi.ROLE_TOGGLE_BUTTON))
        row = ui.wait_for_node(name=name, role=pyatspi.ROLE_LIST_ITEM)
        ui.invoke(ui.wait_within(row, name="Connection actions", role=pyatspi.ROLE_TOGGLE_BUTTON))
        ui.invoke(ui.wait_for_node(name="Remove connection"))
        ui.wait_for_node(name=f"Delete {name}?")
        ui.invoke(ui.wait_for_node(name="Delete", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name=f"Delete {name}?", present=False)

    def deleting_the_open_connection_leaves_no_draft_after_restart(_database, base, restart):
        config = base / "config" / ui.storage_dir_name()
        target = next(
            record
            for record in json.loads((config / "connections.json").read_text())["connections"]
            if record["name"] == ui.CONNECTION_B_NAME
        )
        ui.open_saved_connection(ui.CONNECTION_B_NAME)
        ui.wait_for_frame_containing(f"{ui.CONNECTION_B_NAME} — BookiE")
        ui.invoke_named_action_within("safety_items", "Open safety_items")
        ui.wait_for_node_containing("Rows")
        remove_through_the_gui(ui.CONNECTION_B_NAME)
        restart(lambda _environment: None)
        ui.wait_for_node(name=ui.CONNECTION_NAME)
        ui.wait_for_node(name=ui.CONNECTION_B_NAME, present=False)
        state = Path(config / "workspace_state.json")
        drafts = json.loads(state.read_text())["connections"] if state.exists() else {}
        assert target["id"] not in drafts, f"the deleted connection's draft came back: {drafts[target['id']]}"

    deleting_the_open_connection_leaves_no_draft_after_restart.environment = "local"
    deleting_the_open_connection_leaves_no_draft_after_restart.needs_restart = True
    return [deleting_the_open_connection_leaves_no_draft_after_restart]
