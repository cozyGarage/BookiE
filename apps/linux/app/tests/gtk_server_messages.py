import os
import subprocess
import sys
import time

import pyatspi

import gtk_copy_statements


def clipboard_text(ui):
    result = subprocess.run(
        [sys.executable, "-c", gtk_copy_statements.READ_CLIPBOARD],
        env=os.environ.copy(),
        capture_output=True,
        timeout=ui.WAIT_SECONDS + 15,
    )
    assert result.returncode == 0, result.stderr.decode(errors="replace")
    return result.stdout.decode()


def clipboard_text_containing(ui, needle):
    deadline = time.monotonic() + ui.WAIT_SECONDS
    text = clipboard_text(ui)
    while needle not in text and time.monotonic() < deadline:
        time.sleep(ui.POLL_SECONDS)
        text = clipboard_text(ui)
    return text


def scenarios(ui):
    def postgres_server_notice_output_keeps_rows_and_clears_on_rerun(_database, _base):
        ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
        ui.invoke_accessible_action("win.open-editor")
        ui.wait_for_node(name="Run", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.run_sql("SELECT public.bookie_gtk_notice() AS notice_result")
        ui.wait_for_node_containing("bookie GTK server message")
        ui.wait_for_node(name="42")
        copy = ui.wait_for_node(name="Copy messages", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.invoke_accessible_action("win.focus-grid")
        deadline = time.monotonic() + ui.WAIT_SECONDS
        while not copy.getState().contains(pyatspi.STATE_FOCUSED) and time.monotonic() < deadline:
            ui.press_x11_key("Tab")
            time.sleep(ui.POLL_SECONDS)
        assert copy.getState().contains(pyatspi.STATE_FOCUSED), (
            f"Copy messages is not keyboard reachable:\n{ui.accessible_snapshot()}"
        )
        ui.press_x11_key("space")
        copied = clipboard_text_containing(ui, "bookie GTK server message")
        assert "bookie GTK server message" in copied, f"clipboard text after Copy messages: {copied!r}"
        assert "SELECT public.bookie_gtk_notice()" not in copied, copied
        assert "42" not in copied, copied
        ui.wait_for_node(name="42")
        ui.run_sql("SELECT 43 AS quiet_result")
        ui.wait_for_node(name="43")
        ui.wait_for_node_containing("bookie GTK server message", present=False)

    def postgres_server_notice_flood_can_be_cancelled_and_reused(_database, _base):
        ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
        ui.invoke_accessible_action("win.open-editor")
        ui.wait_for_node(name="Run", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.run_sql("SELECT public.bookie_gtk_notice_flood()")
        cancel = ui.wait_for_node(name="Cancel", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.time.sleep(0.7)
        ui.invoke(cancel)
        ui.wait_for_node(name="Cancel", role=pyatspi.ROLE_PUSH_BUTTON, present=False)
        ui.run_sql("SELECT 44 AS after_cancel")
        ui.wait_for_node(name="44")
        ui.wait_for_node_containing("bookie GTK notice flood", present=False)

    postgres_server_notice_output_keeps_rows_and_clears_on_rerun.environment = "local"
    postgres_server_notice_flood_can_be_cancelled_and_reused.environment = "local"
    return [
        postgres_server_notice_output_keeps_rows_and_clears_on_rerun,
        postgres_server_notice_flood_can_be_cancelled_and_reused,
    ]
