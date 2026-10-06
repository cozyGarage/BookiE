import ctypes
import json
import os
import time


class XWindowAttributes(ctypes.Structure):
    _fields_ = [
        ("x", ctypes.c_int), ("y", ctypes.c_int), ("width", ctypes.c_int), ("height", ctypes.c_int),
        ("border_width", ctypes.c_int), ("depth", ctypes.c_int), ("visual", ctypes.c_void_p),
        ("root", ctypes.c_ulong), ("window_class", ctypes.c_int), ("bit_gravity", ctypes.c_int),
        ("win_gravity", ctypes.c_int), ("backing_store", ctypes.c_int), ("backing_planes", ctypes.c_ulong),
        ("backing_pixel", ctypes.c_ulong), ("save_under", ctypes.c_int), ("colormap", ctypes.c_ulong),
        ("map_installed", ctypes.c_int), ("map_state", ctypes.c_int), ("all_event_masks", ctypes.c_long),
        ("your_event_mask", ctypes.c_long), ("do_not_propagate_mask", ctypes.c_long),
        ("override_redirect", ctypes.c_int), ("screen", ctypes.c_void_p),
    ]


ATSPI_WINDOW_Y_OFFSET = 19


def x11_click(window_x, window_y, button=3, clicks=1):
    x11 = ctypes.CDLL("libX11.so.6")
    xtst = ctypes.CDLL("libXtst.so.6")
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x11.XDefaultRootWindow.restype = ctypes.c_ulong
    x11.XQueryTree.argtypes = [
        ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_uint),
    ]
    x11.XGetWindowAttributes.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(XWindowAttributes)]
    x11.XFree.argtypes = [ctypes.c_void_p]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
    xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    display = x11.XOpenDisplay(None)
    assert display, "X11 display is unavailable"
    try:
        root = x11.XDefaultRootWindow(display)
        root_return, parent, children, count = ctypes.c_ulong(), ctypes.c_ulong(), ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
        x11.XQueryTree(display, root, ctypes.byref(root_return), ctypes.byref(parent), ctypes.byref(children), ctypes.byref(count))
        origin = None
        for index in range(count.value):
            attrs = XWindowAttributes()
            x11.XGetWindowAttributes(display, children[index], ctypes.byref(attrs))
            if attrs.map_state == 2 and attrs.width >= 800 and attrs.height >= 600:
                origin = (attrs.x, attrs.y)
        if children:
            x11.XFree(children)
        assert origin is not None, "the application window was not found on the X display"
        xtst.XTestFakeMotionEvent(display, -1, origin[0] + int(window_x), origin[1] + int(window_y) + ATSPI_WINDOW_Y_OFFSET, 0)
        x11.XFlush(display)
        time.sleep(0.2)
        for _ in range(clicks):
            xtst.XTestFakeButtonEvent(display, button, 1, 0)
            xtst.XTestFakeButtonEvent(display, button, 0, 0)
            x11.XFlush(display)
            time.sleep(0.08)
    finally:
        x11.XCloseDisplay(display)


def scenarios(ui):
    pyatspi = ui.pyatspi

    def text_of(node):
        text = node.queryText()
        return text.getText(0, text.characterCount)

    def editor_text():
        application = ui.application_node()
        candidates = []
        for node in ui.descendants(application):
            if ui.node_role(node) != pyatspi.ROLE_TEXT:
                continue
            try:
                node.queryEditableText()
            except Exception:
                continue
            candidates.append(node)
        return text_of(max(candidates, key=lambda node: node.queryText().characterCount))

    def set_entry(name, text):
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline:
            for node in ui.descendants(ui.application_node()):
                if ui.node_name(node) == name and ui.node_role(node) in (pyatspi.ROLE_ENTRY, pyatspi.ROLE_TEXT):
                    try:
                        node.queryEditableText().setTextContents(text)
                        return
                    except Exception:
                        continue
            time.sleep(ui.POLL_SECONDS)
        raise AssertionError(f"no entry named {name!r}:\n{ui.accessible_snapshot()}")

    def saved_connections(base):
        files = list((base / "config").glob("*/connections.json"))
        assert len(files) == 1, files
        return json.loads(files[0].read_text())["connections"]

    def open_cell_menu(cell_text):
        cell = ui.wait_for_node(name=cell_text, role=pyatspi.ROLE_LABEL)
        extents = cell.queryComponent().getExtents(pyatspi.WINDOW_COORDS)
        x11_click(extents.x + min(extents.width, 40) // 2, extents.y + extents.height // 2)
        time.sleep(0.4)

    def choose_menu_item(position):
        for _ in range(position):
            ui.press_x11_key("Down")
            time.sleep(0.1)
        ui.press_x11_key("Return")

    def click_cell(cell_text, count=1):
        cell = ui.wait_for_node(name=cell_text, role=pyatspi.ROLE_LABEL)
        extents = cell.queryComponent().getExtents(pyatspi.WINDOW_COORDS)
        x11_click(extents.x + min(extents.width, 40) // 2, extents.y + extents.height // 2, button=1, clicks=count)
        time.sleep(0.3)

    def stored_notes(database):
        import sqlite3
        with sqlite3.connect(database) as connection:
            return connection.execute("SELECT id, note FROM safety_items ORDER BY id").fetchall()

    def browse_edit_cell_and_save_persists_to_the_database(database, base):
        import sqlite3
        with sqlite3.connect(database) as connection:
            connection.executemany("INSERT INTO safety_items(id, note) VALUES (?, ?)", [(1, "alpha"), (2, "beta")])
        ui.invoke_named_action_within("safety_items", "Open safety_items")
        ui.wait_for_node(name="alpha", role=pyatspi.ROLE_LABEL)
        click_cell("alpha", count=2)
        for key in "gamma":
            ui.press_x11_key(key)
        ui.press_x11_key("Return")
        ui.wait_for_node(name="1 unsaved change")
        assert stored_notes(database) == [(1, "alpha"), (2, "beta")], "an unsaved edit reached the database"
        ui.press_x11_key("s", ("Control_L",))
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline and stored_notes(database) != [(1, "gamma"), (2, "beta")]:
            time.sleep(ui.POLL_SECONDS)
        assert stored_notes(database) == [(1, "gamma"), (2, "beta")], stored_notes(database)
        ui.wait_for_node(name="1 unsaved change", present=False)

    def editing_a_saved_connection_prefills_it_and_saves_the_new_name(database, base):
        before = saved_connections(base)
        ui.invoke(ui.wait_for_node(name="Open saved connection", role=pyatspi.ROLE_TOGGLE_BUTTON))
        ui.invoke_named_action_within(ui.CONNECTION_B_NAME, "Edit connection")
        ui.wait_for_node(name=f"Edit {ui.CONNECTION_B_NAME}")
        name = ui.wait_for_node(name="Name (optional)", role=pyatspi.ROLE_TEXT)
        assert text_of(name) == ui.CONNECTION_B_NAME, text_of(name)
        ui.set_text_by_name("Name (optional)", "Renamed B")
        ui.invoke(ui.wait_for_node(name="Connect", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="Renamed B — BookiE", role=pyatspi.ROLE_FRAME)
        after = saved_connections(base)
        assert len(after) == len(before), (before, after)
        assert "Renamed B" in [connection["name"] for connection in after], after
        assert ui.CONNECTION_B_NAME not in [connection["name"] for connection in after], after

    def find_bar_replaces_every_match_in_the_editor(database, base):
        ui.set_editor_text("select a, a from t where a = 1")
        time.sleep(0.3)
        ui.press_x11_key("f", ("Control_L",))
        ui.wait_for_node(name="Next match", role=pyatspi.ROLE_PUSH_BUTTON)
        set_entry("Find", "a")
        ui.invoke(ui.wait_for_node(name="Replace", role=pyatspi.ROLE_TOGGLE_BUTTON))
        set_entry("Replace with", "b")
        ui.invoke(ui.wait_for_node(name="Replace All", role=pyatspi.ROLE_PUSH_BUTTON))
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline and editor_text() != "select b, b from t where b = 1":
            time.sleep(ui.POLL_SECONDS)
        assert editor_text() == "select b, b from t where b = 1", editor_text()

    def view_value_opens_the_whole_cell_with_pretty_json(database, base):
        ui.run_sql("""SELECT '{"a":1}' AS payload""")
        open_cell_menu('{"a":1}')
        choose_menu_item(3)
        ui.wait_for_node(name="payload", role=pyatspi.ROLE_DIALOG)
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline:
            if any(
                ui.node_role(node) == pyatspi.ROLE_TEXT and '"a": 1' in text_of(node)
                for node in ui.descendants(ui.application_node())
            ):
                break
            time.sleep(ui.POLL_SECONDS)
        else:
            raise AssertionError(f"the viewer did not show pretty JSON:\n{ui.accessible_snapshot()}")
        ui.wait_for_node(name="Copy value", role=pyatspi.ROLE_PUSH_BUTTON)

    def columns_dialog_hides_a_column_and_keeps_the_last_one(database, base):
        ui.run_sql("SELECT 1 AS alpha, 2 AS beta")
        ui.wait_for_node(name="beta")
        open_cell_menu("1")
        choose_menu_item(6)
        ui.wait_for_node(name="Columns", role=pyatspi.ROLE_DIALOG)
        switches = [
            node for node in ui.descendants(ui.application_node())
            if ui.node_name(node) in ("alpha", "beta")
            and ui.node_role(node) == pyatspi.ROLE_SWITCH
            and node.queryAction().nActions > 0
        ]
        assert len(switches) == 2, ui.accessible_snapshot()
        by_name = {ui.node_name(node): node for node in switches}
        ui.invoke(by_name["beta"])
        ui.invoke(by_name["alpha"])
        time.sleep(0.3)
        ui.press_x11_key("Escape")
        ui.wait_for_node(name="Columns", role=pyatspi.ROLE_DIALOG, present=False)
        ui.wait_for_node(name="beta", present=False)
        ui.wait_for_node(name="alpha")

    def ctrl_slash_toggles_a_comment_in_the_editor(database, base):
        editor = ui.set_editor_text("select 1")
        extents = editor.queryComponent().getExtents(pyatspi.WINDOW_COORDS)
        x11_click(extents.x + 60, extents.y + 10, button=1)
        time.sleep(0.3)
        ui.press_x11_key("slash", ("Control_L",))
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline and not editor_text().startswith("--"):
            time.sleep(ui.POLL_SECONDS)
        assert editor_text().startswith("--"), editor_text()
        assert ui.find_node(name="Keyboard Shortcuts") is None, "the shortcuts window opened instead"

    def postgres_saved_connection_browses_rows_and_values(database, base):
        ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
        ui.wait_for_node(name="people")
        ui.invoke(ui.wait_for_node(name="Open SQL editor"))
        ui.wait_for_node(name="Run", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.run_sql("SELECT id, name, profile, active FROM people ORDER BY id")
        ui.wait_for_node(name="Ada Lovelace", role=pyatspi.ROLE_LABEL)
        ui.wait_for_node(name="Grace Hopper", role=pyatspi.ROLE_LABEL)
        open_cell_menu("Ada Lovelace")
        choose_menu_item(3)
        ui.wait_for_node(name="name", role=pyatspi.ROLE_DIALOG)
        ui.wait_for_node(name="Copy value", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.press_x11_key("Escape")
        ui.wait_for_node(name="name", role=pyatspi.ROLE_DIALOG, present=False)

    result = [
        editing_a_saved_connection_prefills_it_and_saves_the_new_name,
        find_bar_replaces_every_match_in_the_editor,
        view_value_opens_the_whole_cell_with_pretty_json,
        columns_dialog_hides_a_column_and_keeps_the_last_one,
        ctrl_slash_toggles_a_comment_in_the_editor,
        browse_edit_cell_and_save_persists_to_the_database,
    ]
    if os.environ.get("TABLEPRO_GTK_POSTGRES_PORT"):
        result.append(postgres_saved_connection_browses_rows_and_values)
    for scenario in result:
        scenario.environment = "local"
    return result
