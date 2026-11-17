import os
import subprocess
import sys

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


def scenarios(ui):
    def postgres_server_notice_output_keeps_rows_and_clears_on_rerun(_database, _base):
        ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
        ui.invoke_accessible_action("win.open-editor")
        ui.wait_for_node(name="Run", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.run_sql("SELECT public.bookie_gtk_notice() AS notice_result")
        ui.wait_for_node_containing("bookie GTK server message")
        ui.wait_for_node(name="42")
        ui.invoke(ui.wait_for_node(name="Copy messages", role=pyatspi.ROLE_PUSH_BUTTON))
        copied = clipboard_text(ui)
        assert "bookie GTK server message" in copied, copied
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
