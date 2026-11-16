import pyatspi


def scenarios(ui):
    def postgres_server_notice_output_keeps_rows_and_clears_on_rerun(_database, _base):
        ui.open_saved_connection(ui.POSTGRES_CONNECTION_NAME)
        ui.wait_for_frame_containing(f"{ui.POSTGRES_CONNECTION_NAME} — BookiE")
        ui.invoke_accessible_action("win.open-editor")
        ui.wait_for_node(name="Run", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.run_sql("SELECT public.bookie_gtk_notice() AS notice_result")
        ui.wait_for_node_containing("bookie GTK server message")
        ui.wait_for_node_containing("Rows 1 – 1 of 1")
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
