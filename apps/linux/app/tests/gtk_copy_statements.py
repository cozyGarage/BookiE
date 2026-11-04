import os
import sqlite3
import subprocess
import sys
import time

import pyatspi

import gtk_ux

COPY_AS_POSITION = 3
COPY_AS_FORMATS = 14
UPDATE_STATEMENT_POSITION = 11

READ_CLIPBOARD = """import sys, gi
gi.require_version('Gtk', '4.0')
gi.require_version('Gdk', '4.0')
from gi.repository import Gdk, GLib, Gtk
Gtk.init()
clipboard = Gdk.Display.get_default().get_clipboard()
loop = GLib.MainLoop()
def done(source, result):
    sys.stdout.write(clipboard.read_text_finish(result) or '')
    sys.stdout.flush()
    loop.quit()
clipboard.read_text_async(None, done)
GLib.timeout_add_seconds(10, loop.quit)
loop.run()
"""


def scenarios(ui):
    def clipboard_text():
        result = subprocess.run(
            [sys.executable, "-c", READ_CLIPBOARD],
            env=os.environ.copy(),
            capture_output=True,
            timeout=ui.WAIT_SECONDS + 15,
        )
        assert result.returncode == 0, result.stderr.decode(errors="replace")
        return result.stdout.decode()

    def menu_with_items(count):
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while time.monotonic() < deadline:
            for application in ui.desktop_applications():
                for menu in (node for node in ui.descendants(application) if ui.node_role(node) == pyatspi.ROLE_MENU):
                    items = [node for node in ui.descendants(menu) if ui.node_role(node) == pyatspi.ROLE_MENU_ITEM]
                    if len(items) == count and items[0].getState().contains(pyatspi.STATE_SHOWING):
                        return items
            time.sleep(ui.POLL_SECONDS)
        raise AssertionError(f"no showing menu with {count} items")

    def choose_menu_item(items, index):
        ui.invoke(items[index])

    def copy_all_rows_as_update_statements_keeps_null_and_empty_text_apart(database, _base):
        with sqlite3.connect(database) as connection:
            connection.executemany(
                "INSERT INTO safety_items(id, note) VALUES (?, ?)", [(1, "alpha"), (2, None), (3, "")]
            )
        ui.invoke_named_action_within("safety_items", "Open safety_items")
        cell = ui.wait_for_node(name="alpha", role=pyatspi.ROLE_LABEL)
        extents = cell.queryComponent().getExtents(pyatspi.WINDOW_COORDS)
        x = extents.x + min(extents.width, 40) // 2
        y = extents.y + extents.height // 2
        gtk_ux.x11_click(x, y, button=1)
        ui.press_x11_key("a", ("Control_L",))
        gtk_ux.x11_click(x, y)
        main = [node for node in ui.descendants(ui.wait_for_node(role=pyatspi.ROLE_MENU)) if ui.node_role(node) == pyatspi.ROLE_MENU_ITEM]
        # Edit cell, Edit Full Value, Copy value, Copy as: the grid menu's fourth entry opens the formats.
        choose_menu_item(main, COPY_AS_POSITION)
        formats = menu_with_items(COPY_AS_FORMATS)
        choose_menu_item(formats, UPDATE_STATEMENT_POSITION)
        ui.wait_for_node_containing("UPDATE statement copied", role=pyatspi.ROLE_LABEL)
        assert clipboard_text() == (
            'UPDATE "safety_items" SET "note" = \'alpha\' WHERE "id" = 1;\n'
            'UPDATE "safety_items" SET "note" = NULL WHERE "id" = 2;\n'
            'UPDATE "safety_items" SET "note" = \'\' WHERE "id" = 3;'
        )

    copy_all_rows_as_update_statements_keeps_null_and_empty_text_apart.environment = "local"
    return [copy_all_rows_as_update_statements_keeps_null_and_empty_text_apart]
