A SQLite transaction that is dropped after its own COMMIT or ROLLBACK statement no longer leaves the connection in a state where later writes fail or are lost.
