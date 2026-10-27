"""Order-dependent dialog sequences: a dialog that is dismissed must be released, not just hidden.

AT-SPI cannot see a dismissed dialog that is still referenced, so the app writes a leak report
(see ui/leak_probe.rs) when asked through `win.report-leaks`.
"""
import sqlite3
import time
from pathlib import Path

# key (the name the app tracks it under) -> (action, or "button:<name>", that opens it; accessible title)
DIALOGS = {
    "preferences": ("win.preferences", "Preferences"),
    "shortcuts": ("win.shortcuts", "Keyboard Shortcuts"),
    "about": ("win.about", "About"),
    "catalog": ("win.show-catalog", "Catalog"),
    "audit": ("win.show-audit", "Audit Log"),
    "saved_queries": ("win.show-saved-queries", "Saved Queries"),
    "quick_switcher": ("win.open-quickly", "Open quickly"),
    "connect": ("button:New connection", "Connect to PostgreSQL"),
    "structure_form": ("structure:Add Index…", "Add Index"),
    "column_jump": ("table:win.jump-column", "Jump to Column"),
    "export": ("table:win.export-csv", "Export Results"),
    "value_viewer": ("grid-value:", "payload"),
    "column_visibility": ("grid-menu:9", "Columns"),
    "pending_sql": ("preview:", "Pending SQL"),
    "import_csv": ("import-csv:", "Import CSV"),
    "history_panel": ("win.show-history", "Close history"),
    "full_value_editor": ("grid-menu:1", "note"),
}
# AdwAlertDialog is not tracked: libadwaita keeps a closed alert alive even with no handlers.
# Typing into a field before dismissal exercises the edit-then-discard path.
EDITS = {"connect": ("Host", "leak.example.invalid"), "structure_form": ("Name", "leak_idx")}
CYCLES = 4


def scenarios(ui):
    dialog_roles = (ui.pyatspi.ROLE_DIALOG, ui.pyatspi.ROLE_FRAME)
    def leak_report(base):
        path = Path(base) / "leak-report.txt"
        path.unlink(missing_ok=True)
        ui.invoke_accessible_action("win.report-leaks")
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while not path.exists():
            assert time.monotonic() < deadline, "the app did not write a leak report"
            time.sleep(ui.POLL_SECONDS)
        return dict(line.split("=") for line in path.read_text().splitlines())

    def close_with_own_control(dialog):
        actions = dialog.queryAction()
        for index in range(actions.nActions):
            if actions.getName(index) == "window.close":
                return actions.doAction(index)
        for node in ui.descendants(dialog):
            if ui.node_name(node) in ("Close", "Cancel") and ui.node_role(node) == ui.pyatspi.ROLE_PUSH_BUTTON:
                return ui.invoke(node)
        return False

    def insert_row(database):
        with sqlite3.connect(database) as connection:
            connection.execute("INSERT OR IGNORE INTO safety_items(id, note) VALUES (1, 'alpha')")

    def open_grid_value():
        if ui.find_node(name="leak-probe", role=ui.pyatspi.ROLE_LABEL) is None:
            focus_grid_cell()
        ui.press_x11_key("Menu")
        for _ in range(4):
            ui.press_x11_key("Down")
            time.sleep(0.1)
        ui.press_x11_key("Return")

    def open_import_dialog(base):
        path = Path(base) / "home" / "leak-probe.csv"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("id,note\n1,a\n", encoding="utf-8")
        row = ui.wait_for_node(name="safety_items", role=ui.pyatspi.ROLE_LIST_ITEM)
        actions = row.queryAction()
        index = next(i for i in range(actions.nActions) if actions.getName(i) == "sidebar-row.import-csv")
        actions.doAction(index)
        chooser = ui.wait_for_node(name="Import CSV", role=ui.FILE_CHOOSER_ROLES)
        ui.show_file_chooser_location(chooser)
        location = ui.set_visible_editable_within("Import CSV", ui.FILE_CHOOSER_ROLES, str(path), action_name="Open")
        ui.invoke(location)
        open_button = ui.wait_within(chooser, name="Open", role=ui.pyatspi.ROLE_PUSH_BUTTON)
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while not open_button.getState().contains(ui.pyatspi.STATE_SENSITIVE):
            assert time.monotonic() < deadline, "the file chooser did not resolve the CSV path"
            time.sleep(ui.POLL_SECONDS)
        ui.invoke(open_button)
        ui.wait_for_node(name="Import", role=ui.pyatspi.ROLE_PUSH_BUTTON)

    def open_pending_sql():
        if ui.find_node(name="1 unsaved change") is None:
            open_table()
            ui.invoke(ui.wait_for_node(name="Insert row", role=ui.pyatspi.ROLE_PUSH_BUTTON))
            ui.wait_for_node(name="1 unsaved change")
        ui.invoke(ui.wait_for_node(name="Preview SQL", role=ui.pyatspi.ROLE_PUSH_BUTTON))

    def open_grid_menu_item(database, presses):
        insert_row(database)
        open_table()
        cell = ui.wait_for_node(name="alpha", role=ui.pyatspi.ROLE_LABEL)
        extents = cell.queryComponent().getExtents(ui.pyatspi.WINDOW_COORDS)
        ui.gtk_ux.x11_click(extents.x + min(extents.width, 40) // 2, extents.y + extents.height // 2)
        time.sleep(0.4)
        for _ in range(presses):
            ui.press_x11_key("Down")
            time.sleep(0.1)
        ui.press_x11_key("Return")

    def focus_grid_cell():
        ui.run_sql("SELECT 'leak-probe' AS payload")
        ui.wait_for_node_containing("payload\n", role=ui.pyatspi.ROLE_LABEL)
        ui.press_x11_key("j", ("Control_L", "Shift_L"))
        search = ui.wait_for_node(name="Search columns")
        search.queryEditableText().setTextContents("payload")
        ui.press_x11_key("Return")
        ui.wait_for_node(name="Search columns", present=False)
        ui.gtk_ux.wait_for_focused_grid_row(ui, ui.pyatspi, "leak-probe")

    def open_table():
        if ui.find_node(name="safety_items", role=ui.pyatspi.ROLE_PAGE_TAB) is None:
            ui.invoke_named_action_within("safety_items", "Open safety_items")
            ui.wait_for_node(name="safety_items", role=ui.pyatspi.ROLE_PAGE_TAB)

    def open_structure_form(add_button):
        if ui.find_node(name="safety_items", role=ui.pyatspi.ROLE_PAGE_TAB) is None:
            row = ui.wait_for_node(name="safety_items", role=ui.pyatspi.ROLE_LIST_ITEM)
            actions = row.queryAction()
            for index in range(actions.nActions):
                if actions.getName(index) == "sidebar-row.edit-structure":
                    actions.doAction(index)
        ui.invoke(ui.wait_for_node(name="Indexes"))
        row = ui.wait_for_sensitive_node(add_button, role=ui.pyatspi.ROLE_LIST_ITEM)
        extents = row.queryComponent().getExtents(ui.pyatspi.WINDOW_COORDS)
        ui.gtk_ux.x11_click(extents.x + 40, extents.y + extents.height // 2, button=1)

    def entry_text(name):
        entry = ui.wait_for_node(name=name, role=(ui.pyatspi.ROLE_ENTRY, ui.pyatspi.ROLE_TEXT))
        text = entry.queryText()
        return text.getText(0, text.characterCount)

    def set_entry(name, text):
        entry = ui.wait_for_node(name=name, role=(ui.pyatspi.ROLE_ENTRY, ui.pyatspi.ROLE_TEXT))
        entry.queryEditableText().setTextContents(text)

    def dismiss(title):
        # Escape is dropped when focus is not inside the dialog, so use the dialog's own close control.
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while (dialog := ui.find_node(name=title, role=dialog_roles)) is not None:
            assert time.monotonic() < deadline, f"{title!r} did not close:\n{ui.accessible_snapshot()}"
            if not close_with_own_control(dialog):
                ui.press_x11_key("Escape")
            time.sleep(0.5)

    def make_scenario(key):
        action, title = DIALOGS[key]

        def scenario(_database, base):
            for index in range(CYCLES):
                if action == "grid-value:":
                    open_grid_value()
                elif action == "import-csv:":
                    open_import_dialog(base)
                elif action == "preview:":
                    open_pending_sql()
                elif action.startswith("grid-menu:"):
                    open_grid_menu_item(_database, int(action[10:]))
                elif action.startswith("table:"):
                    if key == "export":
                        insert_row(_database)
                    open_table()
                    ui.invoke_accessible_action(action[6:])
                elif action.startswith("structure:"):
                    open_structure_form(action[10:])
                elif action.startswith("button:"):
                    ui.invoke(ui.wait_for_node(name=action[7:], role=ui.pyatspi.ROLE_PUSH_BUTTON))
                else:
                    ui.invoke_accessible_action(action)
                if key == "history_panel":
                    ui.invoke(ui.wait_for_node(name=title, role=ui.pyatspi.ROLE_PUSH_BUTTON))
                    ui.wait_for_node(name=title, present=False)
                else:
                    ui.wait_for_node(name=title, role=dialog_roles)
                if key in EDITS:
                    name, text = EDITS[key]
                    shown = entry_text(name)
                    assert text not in shown, f"{key} reopened with the discarded edit {shown!r}"
                    set_entry(name, text)
                if key != "history_panel":
                    dismiss(title)
                # Closing animations and idle callbacks release a dialog shortly after it hides.
                deadline = time.monotonic() + ui.WAIT_SECONDS
                while True:
                    report = leak_report(base)
                    if int(report.get(key, 0)) == 0 or time.monotonic() > deadline:
                        break
                    time.sleep(ui.POLL_SECONDS)
                assert int(report.get(key, 0)) == 0, (
                    f"{key} dialog is still alive after dismissal {index + 1}: {report}\n"
                    f"{ui.accessible_snapshot()}"
                )

        scenario.__name__ = f"{key}_dialog_is_released_after_repeated_open_and_dismiss"
        return scenario

    return [make_scenario(key) for key in DIALOGS]
