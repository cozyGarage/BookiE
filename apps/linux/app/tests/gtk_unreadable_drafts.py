import json

UNREADABLE = b"\xff\xfe not valid utf-8 \xc3\x28"


def scenarios(ui):
    pyatspi = ui.pyatspi

    def remove_from_welcome(name):
        ui.invoke_accessible_action("win.disconnect")
        if ui.find_node(name=name, role=pyatspi.ROLE_LIST_ITEM) is None:
            ui.invoke(ui.wait_for_node(name="Open saved connection", role=pyatspi.ROLE_TOGGLE_BUTTON))
        row = ui.wait_for_node(name=name, role=pyatspi.ROLE_LIST_ITEM)
        ui.invoke(ui.wait_within(row, name="Connection actions", role=pyatspi.ROLE_TOGGLE_BUTTON))
        ui.invoke(ui.wait_for_node(name="Remove connection"))
        ui.wait_for_node(name=f"Delete {name}?")
        ui.invoke(ui.wait_for_node(name="Delete", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name=f"Delete {name}?", present=False)

    def deleting_another_connection_keeps_an_unreadable_draft_recoverable(_database, base, restart):
        config = base / "config" / ui.storage_dir_name()
        records = json.loads((config / "connections.json").read_text())["connections"]
        kept = next(record["id"] for record in records if record["name"] == ui.CONNECTION_NAME)
        draft_id = "7f0c2f5e-3b1a-4c55-9a53-0d1f6f0b7c11"
        draft = base / "data" / ui.storage_dir_name() / "drafts" / kept / f"{draft_id}.sql"
        metadata = {"connections": {kept: {"tabs": [{"kind": "editor", "query": "", "draft_id": draft_id}], "active_idx": 0}}}

        def seed(_environment):
            draft.parent.mkdir(parents=True, exist_ok=True)
            draft.write_bytes(UNREADABLE)
            (config / "workspace_state.json").write_text(json.dumps(metadata), encoding="utf-8")

        restart(seed)
        ui.wait_for_node(name=ui.CONNECTION_NAME)
        remove_from_welcome(ui.CONNECTION_B_NAME)
        ui.open_saved_connection(ui.CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.CONNECTION_NAME} — BookiE")
        assert draft.read_bytes() == UNREADABLE, "the unreadable draft was rewritten or removed"
        state = json.loads((config / "workspace_state.json").read_text())["connections"]
        tabs = [tab.get("draft_id") for tab in state.get(kept, {}).get("tabs", [])]
        assert draft_id in tabs, f"the workspace metadata lost the unreadable draft reference: {state}"

    deleting_another_connection_keeps_an_unreadable_draft_recoverable.environment = "local"
    deleting_another_connection_keeps_an_unreadable_draft_recoverable.needs_restart = True
    return [deleting_another_connection_keeps_an_unreadable_draft_recoverable]
