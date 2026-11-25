# 0057: Interactive transactions are tab-owned sessions in both frontends

- **Status**: Accepted
- **Date**: 2026-11-24

## Context

GTK has an editor session mode (`apps/linux/app/src/ui/editor/session_mode.rs`) that holds a dedicated, audited session for an interactive transaction. GPUI has no interactive transaction; peers show Begin, Commit and Roll back in the SQL editor toolbar. [ADR 0008](0008-connection-and-session-ownership.md) already sets the ownership, refusal and uncertainty rules. [ADR 0056](0056-audited-reconciliation-of-unknown-write-outcomes.md) sets how an unknown outcome is resolved. Issue [#1750](https://github.com/cozyGarage/BookiE/issues/1750) records that a long-open transaction gets no warning.

This decision adds no new security model. It says how both frontends present interactive transactions on top of ADR 0008 and 0056.

## Decision

1. **One state machine.** The GTK-free part of the session mode moves to `crates/app-services` ([ADR 0018](0018-headless-app-services-crate.md)): the session handle, the commit, rollback-and-close and end ordering, and the callback ownership checks. GTK and GPUI both use it, and neither keeps its own copy.
2. **A transaction belongs to one query tab.**
   - Begin opens one dedicated audited session for that tab through `PolicyGuard`. Every run from the tab uses that session until Commit or Roll back.
   - Other tabs, table browsing and grid saves never join it.
   - Closing the tab, switching its connection or closing the window asks for Commit or Roll back in a dialog, with no default. Cancelling keeps the transaction open.
3. **Buttons and typed statements take the same guarded path.** Inside the tab's dedicated session, the guard already tracks typed `BEGIN`, `COMMIT` and `ROLLBACK` sent as separate statements (`crates/policy/src/guard/session.rs`). Begin, Commit and Roll back in the toolbar send exactly those through the same session, so both routes record the same audit. The guard keeps refusing:
   - transaction control hidden inside a larger statement batch (`hidden_transaction_control`);
   - a read-write begin on a read-only connection;
   - MySQL statements that would commit the transaction implicitly.

   On shared connections, outside a transaction tab, the ADR 0008 refusals of implicit transaction starters and unterminated batches stay as they are.
4. **Policy does not loosen inside a transaction.** Every statement goes through the normal guard: classification, environment rules, read-only connections, approval of writes on Prod, and agent scopes. **Committing a transaction on a Prod connection needs its own approval**, whether it comes from the Commit button or a typed `COMMIT`, and the approval shows the number of governed writes pending in the transaction. The check belongs in the guard session's commit path, so neither route can skip it.
5. **Outcomes:**

   | What happens | Recorded outcome |
   |---|---|
   | Commit succeeds | `committed` for each pending intent |
   | Roll back | `rolled_back` for each pending intent |
   | Connection lost or cancelled during COMMIT | Unknown; governed writes on the connection stay blocked until resolved under ADR 0056, and nothing is retried |
   | Session lost, or the server's idle-in-transaction timeout fires, before any COMMIT is sent | `rolled_back`, with the reason recorded (the server has discarded the transaction) |

   Cancelling a running statement cancels only that statement ([ADR 0005](0005-server-side-cancellation.md)). If the engine has now aborted the transaction (PostgreSQL after an error), only Roll back is offered.
6. **Visibility and idle warning.**
   - The editor toolbar shows Begin transaction, and while a transaction is open, Commit, Roll back and the number of pending governed writes.
   - The tab shows a transaction badge and the elapsed time.
   - After **5 minutes** without a statement (configurable), a message says the transaction is still open and may hold locks, with Commit and Roll back buttons. In GPUI it is an unread notification in the notification centre ([where content goes](../ui-design.md#where-content-goes-gpui)). It never ends the transaction by itself.
7. **Both frontends ship this together.** The Prod commit approval and the idle warning arrive in GTK and GPUI in the same release, through the shared module.
8. **Agents are unchanged.** Agents and MCP cannot open interactive transactions, and Prod agents stay read-only.

## Consequences

- GTK session mode is refactored without behaviour change before GPUI uses it; its existing tests move with the shared module.
- The Prod commit approval and the idle warning are new behaviour in GTK as well. Each needs a changelog fragment when it ships.
- The idle warning waits for the confirmation steps in #1750: the timeout error mapping (25P03 on PostgreSQL) and what the session shows after it.
- Required tests:
  - **Shared module:** rollback completes before close; a rollback failure still closes and reports; a late callback cannot end a newer session; a COMMIT on a lost session records Unknown and blocks writes.
  - **GPUI and GTK UI:** Roll back leaves rows unchanged; Commit changes them; a typed `COMMIT` in the transaction tab records the same audit as the button; `BEGIN` hidden in a batch is refused; closing a tab with an open transaction asks; cancelling keeps it open.
  - **PostgreSQL Docker layer:** an error leaves only Roll back; an idle timeout records `rolled_back`; Prod Commit asks for approval with the pending count.
  - **Audit:** exactly one terminal record per intent on every path, and the chain verifies.
