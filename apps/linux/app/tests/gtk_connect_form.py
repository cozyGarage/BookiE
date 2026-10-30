def scenarios(ui):
    pyatspi = ui.pyatspi

    def edit_dialog_for(name):
        ui.invoke(ui.wait_for_node(name="Open saved connection", role=pyatspi.ROLE_TOGGLE_BUTTON))
        row = ui.wait_for_node(name=name, role=pyatspi.ROLE_LIST_ITEM)
        ui.invoke(ui.wait_within(row, name="Connection actions", role=pyatspi.ROLE_TOGGLE_BUTTON))
        ui.invoke(ui.wait_for_node(name="Edit connection"))
        ui.wait_for_node(name=f"Edit {name}")

    def close_dialog(title):
        dialog = ui.wait_for_node(name=title, role=(pyatspi.ROLE_DIALOG, pyatspi.ROLE_FRAME))
        control = next(
            node
            for node in ui.descendants(dialog)
            if ui.node_name(node) in ("Close", "Cancel") and ui.node_role(node) == pyatspi.ROLE_PUSH_BUTTON
        )
        ui.invoke(control)
        ui.wait_for_node(name=title, role=(pyatspi.ROLE_DIALOG, pyatspi.ROLE_FRAME), present=False)

    def the_connect_timeout_is_offered_only_to_engines_that_connect_over_a_network(_database, _base):
        ui.invoke(ui.wait_for_node(name="New connection", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="Connect to PostgreSQL")
        ui.wait_for_node(name="Connect timeout (seconds)", role=pyatspi.ROLE_SPIN_BUTTON)
        close_dialog("Connect to PostgreSQL")

        edit_dialog_for(ui.CONNECTION_B_NAME)
        ui.wait_for_node(name="Query timeout (seconds)", role=pyatspi.ROLE_SPIN_BUTTON)
        assert ui.find_node(name="Connect timeout (seconds)", role=pyatspi.ROLE_SPIN_BUTTON) is None, (
            f"a SQLite file has no connect phase, yet the form offers a connect timeout:\n{ui.accessible_snapshot()}"
        )

    return [the_connect_timeout_is_offered_only_to_engines_that_connect_over_a_network]
