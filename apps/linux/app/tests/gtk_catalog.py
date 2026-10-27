import sqlite3


def scenarios(ui):
    def table_exists(database, table):
        with sqlite3.connect(database) as connection:
            return connection.execute(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?", (table,),
            ).fetchone()[0] == 1

    def begin_transaction():
        ui.invoke(ui.wait_for_node(name="Session", role=ui.pyatspi.ROLE_TOGGLE_BUTTON))
        ui.wait_for_sensitive_node(name="Run", role=ui.pyatspi.ROLE_PUSH_BUTTON)
        ui.run_sql("BEGIN")
        ui.wait_for_node(name="Transaction active")

    def finish_transaction(commit):
        action = "Commit & close session" if commit else "Roll back & close session"
        ui.invoke(ui.wait_for_sensitive_node(name=action, role=ui.pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="Session committed" if commit else "Session rolled back")

    def session_commit_refreshes_committed_catalog(database, _base):
        table = "catalog_button_commit_probe"
        begin_transaction()
        ui.run_sql(f"CREATE TABLE {table} (id INTEGER PRIMARY KEY)")
        ui.wait_for_node_containing("done in")
        assert not table_exists(database, table), "uncommitted DDL became visible to another connection"
        finish_transaction(True)
        assert table_exists(database, table), "the session did not commit its DDL"
        print("database oracle: committed table exists; checking the sidebar", flush=True)
        ui.wait_for_node(name=table, role=ui.pyatspi.ROLE_LIST_ITEM)

    def session_dialog_commit_refreshes_committed_catalog(database, _base):
        table = "catalog_dialog_commit_probe"
        begin_transaction()
        ui.run_sql(f"CREATE TABLE {table} (id INTEGER PRIMARY KEY)")
        ui.wait_for_node_containing("done in")
        assert not table_exists(database, table), "uncommitted DDL became visible to another connection"
        ui.invoke(ui.wait_for_node(name="Session · transaction open", role=ui.pyatspi.ROLE_TOGGLE_BUTTON))
        ui.wait_for_node(name="End the session with an open transaction?")
        ui.invoke(ui.wait_for_node(name="Commit", role=ui.pyatspi.ROLE_PUSH_BUTTON))
        ui.wait_for_node(name="Session committed")
        assert table_exists(database, table), "the dialog did not commit its DDL"
        print("database oracle: dialog-committed table exists; checking the sidebar", flush=True)
        ui.wait_for_node(name=table, role=ui.pyatspi.ROLE_LIST_ITEM)

    def session_rollback_does_not_hide_later_autocommit_ddl(database, _base):
        rolled_back = "catalog_button_rollback_probe"
        committed = "catalog_after_rollback_probe"
        begin_transaction()
        ui.run_sql(f"CREATE TABLE {rolled_back} (id INTEGER PRIMARY KEY)")
        ui.wait_for_node_containing("done in")
        finish_transaction(False)
        assert not table_exists(database, rolled_back), "the session did not roll back its DDL"
        ui.wait_for_node(name=rolled_back, role=ui.pyatspi.ROLE_LIST_ITEM, present=False)
        ui.run_sql(f"CREATE TABLE {committed} (id INTEGER PRIMARY KEY)")
        ui.wait_for_node_containing("done in")
        assert table_exists(database, committed), "the later DDL was not committed"
        print("database oracle: later autocommit table exists; checking the sidebar", flush=True)
        ui.wait_for_node(name=committed, role=ui.pyatspi.ROLE_LIST_ITEM)

    result = [
        session_commit_refreshes_committed_catalog,
        session_dialog_commit_refreshes_committed_catalog,
        session_rollback_does_not_hide_later_autocommit_ddl,
    ]
    for scenario in result:
        scenario.environment = "local"
    return result
