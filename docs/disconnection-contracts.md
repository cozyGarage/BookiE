# Disconnect and cancellation contracts

This page maps connection-loss and cancellation expectations to the automated tests and runner layers that own them. A server that refuses a new connection remains `ConnectionRefused`; loss of an established connection is `Disconnected`. An interrupted query is not reported as a successful partial result.

## Driver behavior matrix

| Driver | Established server loss | Recovery after restart | Cancellation result and later use |
| --- | --- | --- | --- |
| PostgreSQL | Terminated query and mid-stream backend loss return `Disconnected`; the full result fails. | The existing pool reconnects after restart. | Server cancellation returns `Cancelled`; pool and session remain usable, and cancelled transactions can roll back. |
| MySQL | Terminated query and mid-stream loss return `Disconnected`; the full result fails. | The existing pool reconnects after restart. | Server cancellation returns `Cancelled`; pool and session remain usable. A timed-out write is stopped and the pool recovers. |
| ClickHouse | Hard stop during row streaming returns `Disconnected`; graceful stop preserves the server cancellation error. | The failed connection stays retired; a fresh connection works after restart. | The query is killed server-side, returns `Cancelled`, and the same client remains usable. |
| SQL Server | Loss during a multi-result stream returns `Disconnected` with no partial result. | A fresh connection works after restart. | BookiE does not yet use Tiberius 0.13's `cancel_query` API. Interruption returns `OperationOutcomeUnknown`, retires that connection or session, and leaves the shared connection usable after a session interruption. |
| MongoDB | Loss before an operation and loss during cursor `getMore` return `Disconnected`; no partial cursor result is returned. | The existing client recovers after restart at the same mapped endpoint. | Cancellation returns `OperationOutcomeUnknown(Cancelled)`; after cancellation the same client can list collections. |
| Redis | Loss during a browse page returns `Disconnected`; the incomplete page is rejected. | A later browse through the same driver object obtains a fresh operation-local connection and returns the complete row. | Local socket cancellation returns `OperationOutcomeUnknown`; ordinary commands remain usable and interrupted browse operations do not change their database selection. |

The session policies differ by protocol, so the expected result is recorded per driver. The tests check server state or returned values where available; they do not assume that every driver can cancel a server operation or reuse a physical socket after interruption.

PostgreSQL, MySQL and SQL Server sessions are retired when an established backend is lost. A session must not reconnect transparently and present a replacement backend as the same session, because temporary tables, settings and transaction state no longer match.

## Test ownership

Run the six Docker-backed driver suites, including ignored fixtures, with:

```sh
python3 scripts/run-test-layer.py drivers
```

The runner invokes `scripts/ci-local.sh integration`, which executes the full PostgreSQL, MySQL, SQL Server, ClickHouse, Redis and MongoDB integration targets with `--include-ignored --test-threads=1`. The focused connection-loss, mid-stream, restart and server-cancellation cases are included in those targets.

Redis cancellation uses a local RESP fixture in the separate `cancellation` integration target. It is owned by the sandbox layer:

```sh
python3 scripts/run-test-layer.py sandbox
```

That layer runs `scripts/test-sandbox.sh`, which invokes the Redis cancellation target explicitly after the sandbox crate tests. MongoDB's cancellation and same-client recovery cases can be run individually:

```sh
cargo test --locked -p tablepro-driver-mongodb --test integration a_lost_mongodb_server_is_reported_as_disconnected -- --ignored --exact --test-threads=1
cargo test --locked -p tablepro-driver-mongodb --test integration a_cancelled_mongodb_read_leaves_the_client_usable -- --ignored --exact --test-threads=1
```

These suites are separate from the strict shared value-contract filter. A green value layer alone does not prove disconnect, cancellation, or reconnect behavior.
