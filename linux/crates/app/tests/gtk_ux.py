import ctypes
import json
import time


def x11_click(x, y, button=3):
    x11 = ctypes.CDLL("libX11.so.6")
    xtst = ctypes.CDLL("libXtst.so.6")
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
    xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    display = x11.XOpenDisplay(None)
    assert display, "X11 display is unavailable"
    try:
        xtst.XTestFakeMotionEvent(display, -1, int(x), int(y), 0)
        x11.XFlush(display)
        time.sleep(0.2)
        xtst.XTestFakeButtonEvent(display, button, 1, 0)
        xtst.XTestFakeButtonEvent(display, button, 0, 0)
        x11.XFlush(display)
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

    def saved_connections(base):
        files = list((base / "config").glob("*/connections.json"))
        assert len(files) == 1, files
        return json.loads(files[0].read_text())["connections"]

    def open_cell_menu(cell_text):
        cell = ui.wait_for_node(name=cell_text, role=pyatspi.ROLE_LABEL)
        extents = cell.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
        x11_click(extents.x + min(extents.width, 40) // 2, extents.y + extents.height // 2)

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
        ui.set_text_by_name("Find", "a")
        ui.invoke(ui.wait_for_node(name="Replace", role=pyatspi.ROLE_TOGGLE_BUTTON))
        ui.set_text_by_name("Replace with", "b")
        ui.invoke(ui.wait_for_node(name="Replace All", role=pyatspi.ROLE_PUSH_BUTTON))
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline and editor_text() != "select b, b from t where b = 1":
            time.sleep(ui.POLL_SECONDS)
        assert editor_text() == "select b, b from t where b = 1", editor_text()

    def view_value_opens_the_whole_cell_with_pretty_json(database, base):
        ui.run_sql("""SELECT '{"a":1}' AS payload""")
        open_cell_menu('{"a":1}')
        ui.invoke(ui.wait_for_node(name="View Value…"))
        ui.wait_for_node(name="payload", role=pyatspi.ROLE_DIALOG)
        ui.wait_for_node_containing('"a": 1')
        ui.wait_for_node(name="Copy value", role=pyatspi.ROLE_PUSH_BUTTON)

    def columns_dialog_hides_a_column_and_keeps_the_last_one(database, base):
        ui.run_sql("SELECT 1 AS alpha, 2 AS beta")
        ui.wait_for_node(name="beta")
        open_cell_menu("1")
        ui.invoke(ui.wait_for_node(name="Columns…"))
        ui.wait_for_node(name="Columns", role=pyatspi.ROLE_DIALOG)
        switches = [
            node for node in ui.descendants(ui.application_node())
            if ui.node_name(node) in ("alpha", "beta") and ui.node_role(node) == pyatspi.ROLE_SWITCH
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

    result = [
        editing_a_saved_connection_prefills_it_and_saves_the_new_name,
        find_bar_replaces_every_match_in_the_editor,
        view_value_opens_the_whole_cell_with_pretty_json,
        columns_dialog_hides_a_column_and_keeps_the_last_one,
    ]
    for scenario in result:
        scenario.environment = "local"
    return result
