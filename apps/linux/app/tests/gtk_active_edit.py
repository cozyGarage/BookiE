import sqlite3
import time

import gtk_ux


def scenarios(ui):
    def notes(database):
        with sqlite3.connect(database) as connection:
            return connection.execute("SELECT id, note FROM safety_items ORDER BY id").fetchall()

    def begin_edit(database):
        with sqlite3.connect(database) as connection:
            connection.executemany(
                "INSERT INTO safety_items(id, note) VALUES (?, ?)", [(1, "alpha"), (2, "beta")]
            )
        ui.invoke_named_action_within("safety_items", "Open safety_items")
        cell = ui.wait_for_node(name="alpha", role=ui.pyatspi.ROLE_LABEL)
        bounds = cell.queryComponent().getExtents(ui.pyatspi.WINDOW_COORDS)
        gtk_ux.x11_click(bounds.x + min(bounds.width, 40) // 2, bounds.y + bounds.height // 2, button=1, clicks=2)
        time.sleep(0.3)
        for key in "gamma":
            ui.press_x11_key(key)
        assert notes(database) == [(1, "alpha"), (2, "beta")]

    def cancel_close_and_save(database, action, heading):
        begin_edit(database)
        if action == "keyboard":
            ui.press_x11_key("w", ("Control_L",))
        elif action == "button":
            page = ui.wait_for_node_containing("safety_items", role=ui.pyatspi.ROLE_PAGE_TAB)
            ui.invoke(ui.wait_within(page, name="Close Tab", role=ui.pyatspi.ROLE_PUSH_BUTTON))
        else:
            ui.invoke_accessible_action(action)
        ui.wait_for_node(name=heading)
        ui.invoke(ui.wait_for_node(name="Cancel", role=ui.pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name=heading, present=False)
        ui.wait_for_node_containing("safety_items", role=ui.pyatspi.ROLE_PAGE_TAB)
        assert notes(database) == [(1, "alpha"), (2, "beta")], "cancel committed the edit"
        save_and_assert_notes(database)

    def save_and_assert_notes(database):
        ui.invoke_accessible_action("win.save-changes")
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline and notes(database) != [(1, "gamma"), (2, "beta")]:
            time.sleep(ui.POLL_SECONDS)
        assert notes(database) == [(1, "gamma"), (2, "beta")], "the typed value was lost or its sibling changed"

    def active_cell_edit_enter_control(database, base):
        begin_edit(database)
        ui.press_x11_key("Return")
        ui.wait_for_node(name="1 unsaved change")
        save_and_assert_notes(database)

    def active_cell_edit_survives_cancelled_tab_close(database, base):
        cancel_close_and_save(database, "win.close-current", "Save changes to “safety_items”?")

    def active_cell_edit_survives_cancelled_keyboard_close(database, base):
        cancel_close_and_save(database, "keyboard", "Save changes to “safety_items”?")

    def active_cell_edit_survives_cancelled_close_button(database, base):
        cancel_close_and_save(database, "button", "Save changes to “safety_items”?")

    def active_cell_edit_save_then_close_persists_the_value(database, base):
        begin_edit(database)
        ui.invoke_accessible_action("win.close-current")
        ui.wait_for_node(name="Save changes to “safety_items”?")
        prompt = ui.wait_for_node(name="Save changes to “safety_items”?", role=ui.pyatspi.ROLE_ALERT)
        ui.invoke(ui.wait_within(prompt, name="Save", role=ui.pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing("safety_items", role=ui.pyatspi.ROLE_PAGE_TAB, present=False)
        assert notes(database) == [(1, "gamma"), (2, "beta")]

    def active_cell_edit_discard_then_close_leaves_the_database_unchanged(database, base):
        begin_edit(database)
        ui.invoke_accessible_action("win.close-current")
        ui.wait_for_node(name="Save changes to “safety_items”?")
        prompt = ui.wait_for_node(name="Save changes to “safety_items”?", role=ui.pyatspi.ROLE_ALERT)
        ui.invoke(ui.wait_within(prompt, name="Discard", role=ui.pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing("safety_items", role=ui.pyatspi.ROLE_PAGE_TAB, present=False)
        assert notes(database) == [(1, "alpha"), (2, "beta")]

    def active_cell_edit_keeps_its_preview_when_another_table_opens(database, base):
        ui.run_sql("CREATE TABLE other_items(id INTEGER PRIMARY KEY, note TEXT)")
        ui.wait_for_node(name="other_items", role=ui.pyatspi.ROLE_LIST_ITEM)
        begin_edit(database)
        ui.invoke_named_action_within("other_items", "Open other_items")
        ui.wait_for_node_containing("other_items", role=ui.pyatspi.ROLE_PAGE_TAB)
        ui.wait_for_node_containing("safety_items", role=ui.pyatspi.ROLE_PAGE_TAB)
        assert notes(database) == [(1, "alpha"), (2, "beta")]
        ui.invoke_named_action_within("safety_items", "Open safety_items")
        ui.wait_for_node(name="1 unsaved change")
        save_and_assert_notes(database)

    def active_cell_edit_survives_cancelled_window_quit(database, base):
        cancel_close_and_save(database, "win.quit", "Save changes before closing?")

    def active_cell_edit_survives_cancelled_disconnect(database, base):
        cancel_close_and_save(database, "win.disconnect", "Discard pending changes?")

    def active_cell_edit_saves_when_close_button_targets_an_inactive_tab(database, base):
        begin_edit(database)
        ui.invoke_accessible_action("win.open-editor")
        ui.wait_for_node(name="Run", role=ui.pyatspi.ROLE_PUSH_BUTTON)
        page = ui.wait_for_node_containing("safety_items", role=ui.pyatspi.ROLE_PAGE_TAB)
        ui.invoke(ui.wait_within(page, name="Close Tab", role=ui.pyatspi.ROLE_PUSH_BUTTON))
        prompt = ui.wait_for_node(name="Save changes to “safety_items”?", role=ui.pyatspi.ROLE_ALERT)
        ui.invoke(ui.wait_within(prompt, name="Save", role=ui.pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing("safety_items", role=ui.pyatspi.ROLE_PAGE_TAB, present=False)
        assert notes(database) == [(1, "gamma"), (2, "beta")]

    result = [
        active_cell_edit_enter_control,
        active_cell_edit_survives_cancelled_tab_close,
        active_cell_edit_survives_cancelled_keyboard_close,
        active_cell_edit_survives_cancelled_close_button,
        active_cell_edit_save_then_close_persists_the_value,
        active_cell_edit_discard_then_close_leaves_the_database_unchanged,
        active_cell_edit_survives_cancelled_window_quit,
        active_cell_edit_keeps_its_preview_when_another_table_opens,
        active_cell_edit_survives_cancelled_disconnect,
        active_cell_edit_saves_when_close_button_targets_an_inactive_tab,
    ]
    for scenario in result:
        scenario.environment = "local"
    return result
