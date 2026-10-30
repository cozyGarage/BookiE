# PostgreSQL session and pool behavior, October 21

## Pooled connection reset

Issue [#1288](https://github.com/cozyGarage/BookiE/issues/1288) tracks cleanup of session state before a pooled PostgreSQL connection is reused. The reset runs on the same backend that was used by the operation. A reused connection must not retain a changed `search_path` or temporary tables.

## Retired editor session

Issue [#1294](https://github.com/cozyGarage/BookiE/issues/1294) tracks an SSH-recovered editor session whose database session is no longer usable. The app refuses session-setting reads before SQL dispatch and presents the refusal without a misleading query-completion state. Reopening the session allows a new query; the retired session must not persist writes.

## PgBouncer prepared statements

Issue [#1295](https://github.com/cozyGarage/BookiE/issues/1295) tracks a test that assumed every workload fails without prepared-statement tracking. The test accepts either a verified successful workload or an actionable prepared-statement conflict. It stops after the first failed create or insert and checks values only for stages that succeeded. This does not qualify transaction pooling for session-dependent editor features.
