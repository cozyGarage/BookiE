def scenarios(ui):
    def assert_refusal_and_recovery(database, base, restart, broken_link):
        policy = base / "config" / ui.storage_dir_name() / "policy.toml"
        target = policy.with_name("policy-target.toml")

        def seed(_environment):
            if broken_link:
                policy.symlink_to(target)
            else:
                policy.write_text("[environments.nowhere]\n", encoding="utf-8")

        restart(seed)
        ui.invoke_named_action_within(ui.CONNECTION_NAME, "Open connection")
        ui.wait_for_node(name="Connection failed")
        ui.wait_for_node_containing("Policy unavailable")
        ui.invoke(ui.wait_for_node(name="Close", role=ui.pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="Connection failed", present=False)
        ui.assert_database_count_stable(database, 0, seconds=0.2)

        def repair(_environment):
            (target if broken_link else policy).write_text("", encoding="utf-8")

        restart(repair)
        ui.invoke_named_action_within(ui.CONNECTION_NAME, "Open connection")
        ui.wait_for_frame_containing(f"{ui.CONNECTION_NAME} — BookiE")
        ui.invoke(ui.wait_for_node(name="Open SQL editor"))
        ui.wait_for_node(name="Run", role=ui.pyatspi.ROLE_PUSH_BUTTON)
        ui.run_sql("INSERT INTO safety_items(id) VALUES (81)")
        ui.wait_for_database_count(database, 1)
        assert ui.database_ids(database) == [81]

    def a_broken_policy_link_refuses_connections_and_recovers_after_repair(database, base, restart):
        assert_refusal_and_recovery(database, base, restart, True)

    def a_malformed_policy_refuses_connections_and_recovers_after_repair(database, base, restart):
        assert_refusal_and_recovery(database, base, restart, False)

    result = [
        a_broken_policy_link_refuses_connections_and_recovers_after_repair,
        a_malformed_policy_refuses_connections_and_recovers_after_repair,
    ]
    for scenario in result:
        scenario.environment = "local"
        scenario.requires_editor = False
        scenario.needs_restart = True
    return result
