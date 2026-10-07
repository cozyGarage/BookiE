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


ATSPI_WINDOW_Y_OFFSET = int(os.environ.get("TABLEPRO_GTK_Y_OFFSET", "19"))


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

    def wait_for_adw_dialog(name, present=True):
        if os.environ.get("TABLEPRO_GTK_OLD_ADW") == "1":
            return None
        return ui.wait_for_node(name=name, role=pyatspi.ROLE_DIALOG, present=present)

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

    def open_edit_dialog_for(name):
        ui.invoke(ui.wait_for_node(name="Open saved connection", role=pyatspi.ROLE_TOGGLE_BUTTON))
        ui.invoke_named_action_within(name, "Edit connection")
        ui.wait_for_node(name=f"Edit {name}")

    def test_connection_reports_success_in_the_dialog(database, base):
        open_edit_dialog_for(ui.CONNECTION_B_NAME)
        ui.invoke(ui.wait_for_node(name="Test", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing("Connection ok")

    def test_connection_reports_failure_in_the_dialog(database, base):
        open_edit_dialog_for(ui.BROKEN_CONNECTION_NAME)
        ui.invoke(ui.wait_for_node(name="Test", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node_containing("Test failed")
        assert ui.find_node_containing("Connection ok") is None

    def connect_dialog_cancel_stops_a_hanging_connection(database, base):
        import socket
        import threading

        server = socket.socket()
        server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        server.bind(("127.0.0.1", 5432))
        server.listen(4)
        held = []
        threading.Thread(target=lambda: [held.append(server.accept()) for _ in range(4)], daemon=True).start()
        try:
            ui.invoke(ui.wait_for_node(name="New connection", role=pyatspi.ROLE_PUSH_BUTTON))
            ui.wait_for_node(name="Connect to PostgreSQL")
            set_entry("Host", "127.0.0.1")
            ui.invoke(ui.wait_for_node(name="Connect", role=pyatspi.ROLE_PUSH_BUTTON))
            ui.invoke(ui.wait_for_node(name="Cancel", role=pyatspi.ROLE_PUSH_BUTTON))
            ui.wait_for_node_containing("Connection cancelled")
            ui.wait_for_node(name="Connect", role=pyatspi.ROLE_PUSH_BUTTON)
            assert ui.find_node(name="Cancel", role=pyatspi.ROLE_PUSH_BUTTON) is None
        finally:
            server.close()

    def app_memory_kb():
        import glob
        for status in glob.glob("/proc/[0-9]*/status"):
            try:
                executable = os.readlink(status.replace("status", "exe"))
                text = open(status).read()
            except OSError:
                continue
            if not executable.endswith("/usr/bin/tablepro"):
                continue
            fields = dict(line.split(":", 1) for line in text.splitlines() if ":" in line)
            return int(fields["VmRSS"].split()[0]), int(fields["VmHWM"].split()[0])
        raise AssertionError("application process not found")

    def a_second_launch_raises_the_window_and_exits_cleanly(database, base):
        import glob
        import subprocess
        for status in glob.glob("/proc/[0-9]*/status"):
            try:
                executable = os.readlink(status.replace("status", "exe"))
                raw_environment = open(status.replace("status", "environ"), "rb").read()
            except OSError:
                continue
            if executable.endswith("/usr/bin/tablepro"):
                break
        else:
            raise AssertionError("application process not found")
        environment = dict(item.split("=", 1) for item in raw_environment.decode().split("\0") if "=" in item)
        result = subprocess.run([executable], env=environment, capture_output=True, text=True, timeout=30)
        assert result.returncode == 0, (result.returncode, result.stderr)
        assert "did not unregister" not in result.stderr, result.stderr
        assert "asked it to show its window" in result.stderr, result.stderr
        ui.wait_for_frame_containing(" — BookiE")

    def profile_large_result_in_the_grid(database, base):
        rows = int(os.environ["TABLEPRO_PROFILE_ROWS"])
        sql = (
            f"WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {rows}) "
            "SELECT x, 'person-' || x, x * 1.5, x % 2, datetime('2026-01-01', '+' || (x % 100000) || ' seconds'), "
            "'a note of moderate length for row ' || x || ' to give text columns some weight' FROM c"
        )
        before_rss, _ = app_memory_kb()
        started = time.monotonic()
        ui.run_sql(sql)
        ui.wait_for_node_containing("done in", timeout=300)
        shown = time.monotonic() - started
        time.sleep(3)
        after_rss, peak = app_memory_kb()
        line = json.dumps({
            "rows_requested": rows, "seconds_to_result": round(shown, 2),
            "rss_before_mb": round(before_rss / 1024, 1), "rss_after_mb": round(after_rss / 1024, 1),
            "peak_mb": round(peak / 1024, 1),
        })
        with open(os.environ["TABLEPRO_PROFILE_OUT"], "a") as out:
            out.write(line + "\n")

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

    def wait_for_dialog(name, present=True):
        if os.environ.get("TABLEPRO_GTK_OLD_ADW") == "1":
            return
        ui.wait_for_node(name=name, role=pyatspi.ROLE_DIALOG, present=present)

    def view_value_opens_the_whole_cell_with_pretty_json(database, base):
        ui.run_sql("""SELECT '{"a":1}' AS payload""")
        open_cell_menu('{"a":1}')
        choose_menu_item(3)
        wait_for_adw_dialog("payload")
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
        wait_for_adw_dialog("Columns")
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
        wait_for_adw_dialog("Columns", present=False)
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
        wait_for_adw_dialog("name")
        ui.wait_for_node(name="Copy value", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.press_x11_key("Escape")
        wait_for_adw_dialog("name", present=False)

    def psql(sql):
        import subprocess
        out = subprocess.run(
            ["docker", "exec", os.environ["TABLEPRO_GTK_POSTGRES_CONTAINER"], "psql", "--username=postgres",
             "--dbname=bookie_test", "--tuples-only", "--no-align", f"--command={sql}"],
            check=True, capture_output=True, text=True,
        )
        return out.stdout.strip()

    def wait_for_psql(sql, expected):
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline and psql(sql) != expected:
            time.sleep(ui.POLL_SECONDS)
        assert psql(sql) == expected, (sql, psql(sql), expected)

    def grid_edit_and_delete_commit_to_the_server(connection_name, table_anchor, oracle):
        ui.open_saved_connection(connection_name)
        ui.wait_for_frame_containing(f"{connection_name} — BookiE")
        ui.invoke_named_action_within(table_anchor, "Open people")
        ui.wait_for_node(name="Grace Hopper", role=pyatspi.ROLE_LABEL)
        click_cell("Grace Hopper", count=2)
        for key in "gracey":
            ui.press_x11_key(key)
        ui.press_x11_key("Return")
        ui.wait_for_node(name="1 unsaved change")
        assert oracle("SELECT name FROM people WHERE id = 2") == "Grace Hopper", "an unsaved edit reached the server"
        ui.press_x11_key("s", ("Control_L",))
        wait_for_oracle(oracle, "SELECT name FROM people WHERE id = 2", "gracey")
        assert ui.find_node(name="Approve once") is None, "a plain grid edit asked for approval"
        ui.wait_for_node(name="1 unsaved change", present=False)

        open_cell_menu("gracey")
        choose_menu_item(13)
        ui.wait_for_node(name="1 unsaved change")
        assert oracle("SELECT count(*) FROM people") == "2", "an unsaved delete reached the server"
        ui.press_x11_key("s", ("Control_L",))
        wait_for_oracle(oracle, "SELECT count(*) FROM people", "1")
        wait_for_oracle(oracle, "SELECT id FROM people", "1")
        assert ui.find_node(name="Approve once") is None, "a plain grid delete asked for approval"

    def wait_for_oracle(oracle, sql, expected):
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline and oracle(sql) != expected:
            time.sleep(ui.POLL_SECONDS)
        assert oracle(sql) == expected, (sql, oracle(sql), expected)

    def postgres_database_switcher_reconnects_to_the_chosen_database(database, base):
        ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
        ui.wait_for_node(name="public.people", role=pyatspi.ROLE_LIST_ITEM)
        ui.invoke(ui.wait_for_node(name="Switch database", role=pyatspi.ROLE_TOGGLE_BUTTON))
        ui.wait_for_node(name="bookie_test")
        ui.invoke(ui.wait_for_node(name="bookie_other", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="public.other_things", role=pyatspi.ROLE_LIST_ITEM)
        ui.wait_for_node(name="public.people", role=pyatspi.ROLE_LIST_ITEM, present=False)
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline and {c["name"]: c["database"] for c in saved_connections(base)}.get(ui.POSTGRES_CONNECTION_NAME) != "bookie_other":
            time.sleep(ui.POLL_SECONDS)
        databases = {connection["name"]: connection["database"] for connection in saved_connections(base)}
        assert databases[ui.POSTGRES_CONNECTION_NAME] == "bookie_other", databases

    def mysql(sql):
        import subprocess
        out = subprocess.run(
            ["docker", "exec", os.environ["TABLEPRO_GTK_MYSQL_CONTAINER"], "mysql", "--user=root",
             "--password=tablepro_test", "--batch", "--skip-column-names", "bookie_test", f"--execute={sql}"],
            check=True, capture_output=True, text=True,
        )
        return out.stdout.strip()

    def postgres_grid_edit_and_delete_commit_to_the_server(database, base):
        grid_edit_and_delete_commit_to_the_server(ui.POSTGRES_CONNECTION_NAME, "public.people", psql)

    def mysql_grid_edit_and_delete_commit_to_the_server(database, base):
        grid_edit_and_delete_commit_to_the_server(ui.MYSQL_CONNECTION_NAME, "bookie_test.people", mysql)

    result = [
        editing_a_saved_connection_prefills_it_and_saves_the_new_name,
        find_bar_replaces_every_match_in_the_editor,
        view_value_opens_the_whole_cell_with_pretty_json,
        columns_dialog_hides_a_column_and_keeps_the_last_one,
        ctrl_slash_toggles_a_comment_in_the_editor,
        browse_edit_cell_and_save_persists_to_the_database,
        test_connection_reports_success_in_the_dialog,
        test_connection_reports_failure_in_the_dialog,
        connect_dialog_cancel_stops_a_hanging_connection,
        a_second_launch_raises_the_window_and_exits_cleanly,
    ]
    if os.environ.get("TABLEPRO_PROFILE_ROWS"):
        result.append(profile_large_result_in_the_grid)
    if os.environ.get("TABLEPRO_GTK_MYSQL_CONTAINER"):
        result.append(mysql_grid_edit_and_delete_commit_to_the_server)
    if os.environ.get("TABLEPRO_GTK_POSTGRES_PORT"):
        result.append(postgres_saved_connection_browses_rows_and_values)
        result.append(postgres_grid_edit_and_delete_commit_to_the_server)
        result.append(postgres_database_switcher_reconnects_to_the_chosen_database)
    for scenario in result:
        scenario.environment = "local"
    return result
