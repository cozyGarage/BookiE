def scenarios(ui):
    pyatspi = ui.pyatspi

    def the_empty_profile_offers_to_import_connections(_database, base, restart):
        def empty_profile(environment):
            for kind in ("config", "data"):
                (base / "empty-profile" / kind).mkdir(parents=True)
            environment["XDG_CONFIG_HOME"] = str(base / "empty-profile" / "config")
            environment["XDG_DATA_HOME"] = str(base / "empty-profile" / "data")

        restart(empty_profile, empty=True)
        ui.wait_for_node(name="No connections yet")
        ui.wait_for_node(name="Add Connection", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.wait_for_node(name="Import from URL…", role=pyatspi.ROLE_PUSH_BUTTON)
        ui.invoke(ui.wait_for_node(name="Import connections…", role=pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="Import connections", role=ui.FILE_CHOOSER_ROLES)

    the_empty_profile_offers_to_import_connections.environment = "local"
    the_empty_profile_offers_to_import_connections.needs_restart = True
    return [the_empty_profile_offers_to_import_connections]
