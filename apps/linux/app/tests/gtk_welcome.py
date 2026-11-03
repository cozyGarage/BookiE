import json


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

    def recent_connection_card_displays_production_and_opens_the_connection(_database, base, restart):
        def seed_recent(_environment):
            path = base / "config" / ui.storage_dir_name() / "connections.json"
            contents = json.loads(path.read_text(encoding="utf-8"))
            connection = next(item for item in contents["connections"] if item["name"] == ui.CONNECTION_NAME)
            connection["last_opened_at"] = "2026-01-01T00:00:00Z"
            connection["environment"] = "prod"
            path.write_text(json.dumps(contents), encoding="utf-8")

        restart(seed_recent, restored="welcome")
        ui.wait_for_node(name="Welcome to BookiE")
        card = ui.wait_for_node(
            name=f"Open recent connection: {ui.CONNECTION_NAME}, SQLite · Production",
            role=pyatspi.ROLE_PUSH_BUTTON,
        )
        ui.invoke(card)
        ui.wait_for_frame_containing(" — BookiE")

    recent_connection_card_displays_production_and_opens_the_connection.requires_editor = False
    recent_connection_card_displays_production_and_opens_the_connection.needs_restart = True
    recent_connection_card_displays_production_and_opens_the_connection.environment = "local"
    return [the_empty_profile_offers_to_import_connections, recent_connection_card_displays_production_and_opens_the_connection]
