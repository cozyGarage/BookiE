# Disconnect and cancellation contracts

This page maps connection-loss and cancellation expectations to the automated tests and runner layers that own them. A server that refuses a new connection remains `ConnectionRefused`; loss of an established connection is `Disconnected`. An interrupted query is not reported as a successful partial result.

## Driver behavior matrix

| Driver | Established server loss | Recovery after restart | Cancellation result and later use |
| --- | --- | --- | --- |
| PostgreSQL | Terminated query and mid-stream backend loss return `Disconnected`; the full result fails. | The existing pool reconnects after restart. | Server cancellation returns `Cancelled`; pool and session remain usable, and cancelled transactions can roll back. Every controlled call, including metadata reads, browse pages and grid saves, asks the server to stop when its deadline passes or the caller cancels. A stop the server confirms returns `TimedOut` or `Cancelled`; a stop it cannot confirm returns `OperationOutcomeUnknown` and retires that backend. |
| MySQL | Terminated query and mid-stream loss return `Disconnected`; the full result fails. | The existing pool reconnects after restart. | Server cancellation returns `Cancelled`; pool and session remain usable. A timed-out write is stopped and the pool recovers. Every controlled call that sends a statement, including metadata reads, browse pages, grid saves and transaction starts, asks the server to stop when its deadline passes or the caller cancels. A stop the server confirms returns `TimedOut` or `Cancelled`, and a stopped grid save returns it inside the rolled-back `Transaction` error so the audit record can say rolled back; a stop it cannot confirm returns `OperationOutcomeUnknown` and closes that connection. |
| ClickHouse | Hard stop during row streaming returns `Disconnected`; graceful stop preserves the server cancellation error. | The failed connection stays retired; a fresh connection works after restart. | The query is killed server-side, returns `Cancelled`, and the same client remains usable. |
| SQL Server | Loss during a multi-result stream returns `Disconnected` with no partial result. | A fresh connection works after restart. | BookiE does not yet use Tiberius 0.13's `cancel_query` API. Interruption returns `OperationOutcomeUnknown`, retires that connection or session, and leaves the shared connection usable after a session interruption. |
| MongoDB | Loss before an operation and loss during cursor `getMore` return `Disconnected`; no partial cursor result is returned. | The existing client recovers after restart at the same mapped endpoint. | Cancellation returns `OperationOutcomeUnknown(Cancelled)`; after cancellation the same client can list collections. |
| Redis | Loss during a browse page returns `Disconnected`; the incomplete page is rejected. | A later browse through the same driver object obtains a fresh operation-local connection and returns the complete row. | Local socket cancellation returns `OperationOutcomeUnknown`; ordinary commands remain usable and interrupted browse operations do not change their database selection. |

The session policies differ by protocol, so the expected result is recorded per driver. The tests check server state or returned values where available; they do not assume that every driver can cancel a server operation or reuse a physical socket after interruption.

### Calls that stop on the server

A driver that reports server cancellation must stop the statement on the server for every controlled call, not only for editor statements. A call that falls back to the contract's default form only stops waiting: the statement keeps running, and the pooled connection is returned while its reply is still pending.

A caller that drops a controlled call's future, for example under its own shorter timeout, instead of passing a control, stops nothing on SQLite and DuckDB: the statement keeps running, a SQLite write keeps its lock, and on DuckDB every later call waits. Use the control's deadline or token.

| Driver | State |
| --- | --- |
| PostgreSQL | Every controlled call stops on the server. `every_controlled_call_of_the_contract_has_a_stop_case` fails when the contract gains a controlled call without a stop case. |
| MySQL | Every controlled call that sends a statement stops on the server through `KILL QUERY`, on a connection and inside an interactive transaction. `every_controlled_call_of_the_contract_has_a_mysql_stop_case` fails when the contract gains a controlled call without a stop case. |
| SQLite | Every controlled call that can run unboundedly stops inside SQLite through its interrupt: queries, writes, table pages, both transaction batches, and statements inside an interactive transaction. An interrupted write ends the transaction, because SQLite rolls it back, and the transaction object refuses further statements. A transaction takes no bound parameters, so its parameterised controlled calls refuse them and otherwise run the plain controlled call. A transaction that is dropped, or whose controlled rollback passes its deadline, finishes its rollback on its own task, including when SQL text inside the transaction already ended it, so the pooled connection is never returned with a stuck transaction. The catalog reads keep the default form and are listed with the reason in `every_controlled_call_of_the_contract_is_stoppable_or_bounded_with_a_reason`, which fails when the contract gains a controlled call that is in neither list. |
| DuckDB | The same for queries, writes, table pages and both transaction batches, through DuckDB's interrupt. DuckDB has one connection, so a statement that is not interrupted blocks every later call. The catalog reads keep the default form and are listed with the reason in the matching test. |
| ClickHouse | Every controlled call that sends a statement tags it with a query id and stops it on the server through `KILL QUERY`: queries, writes, metadata reads, table pages and the sequential batch. A batch is not atomic: a stop in a later statement reports the earlier statements as applied. A stopped `ALTER ... UPDATE` or `DELETE` can still be applied, because the driver waits for the mutation and a kill only ends the wait, so that outcome is reported as unknown. The calls that send no statement are listed with that fact in `every_controlled_call_of_the_contract_has_a_clickhouse_stop_case`, which fails when the contract gains a controlled call that is in neither list. |

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
cargo test --locked -p bookie-driver-mongodb --test integration a_lost_mongodb_server_is_reported_as_disconnected -- --ignored --exact --test-threads=1
cargo test --locked -p bookie-driver-mongodb --test integration a_cancelled_mongodb_read_leaves_the_client_usable -- --ignored --exact --test-threads=1
```

These suites are separate from the strict shared value-contract filter. A green value layer alone does not prove disconnect, cancellation, or reconnect behavior.
