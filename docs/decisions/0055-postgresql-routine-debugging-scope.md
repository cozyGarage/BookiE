# 0055: Scope PostgreSQL routine debugging before implementation

- **Status**: Proposed
- **Date**: 2026-11-18

## Context

Issue [#3099](https://github.com/cozyGarage/BookiE/issues/3099) requests breakpoints, stepping and local-variable inspection for database routines. The [pgAdmin debugger documentation](https://www.pgadmin.org/docs/pgadmin4/latest/debugger.html) describes its server-extension and privilege requirements; the [extension installation instructions](https://github.com/EnterpriseDB/pldebugger) require preload configuration and a server restart. Invoking the selected routine may modify data, and debugger commands can resume that execution. Locals may contain values hidden by ordinary result masking.

## Decision

1. The first implementation slice, if accepted, covers direct debugging of one selected PostgreSQL PL/pgSQL routine on an explicitly opted-in, administrator-provisioned development connection. The client does not install the extension, change server configuration or elevate privileges.
2. Debugging remains unavailable to read-only connections and agent principals. Routine invocation, step and continue are governed operations because any may execute writes. They use PolicyGuard approval, durable audit intent and terminal outcomes; debugger commands are not treated as reads merely because they control a debugger.
3. The initial slice supports source display, line breakpoints, step, continue, stop, a bounded stack and bounded local inspection. It excludes changing local values, arbitrary expressions, global breakpoints, attaching to unrelated sessions and other engines.
4. Driver capability and protocol details stay in the PostgreSQL driver. app-services owns the lifecycle and frontends render its typed state. No frontend receives a raw connection.
5. Locals are exposed only when the policy can establish that masking and authorization are safe. Otherwise inspection is denied. Stack depth, local count, value bytes, response bytes, queued events, command time and total session time have explicit bounds and incomplete-state reporting before a frontend is built.
6. Cancellation, close, disconnect, replacement and transport loss account for both debugger and target sessions. A dropped future or debugger stop is not proof that the routine stopped. Unknown outcomes follow [ADR 0005](0005-server-side-cancellation.md); reconnect does not replay invocation or debugger commands. Stopping does not promise rollback of external side effects.

## Rationale

This scope keeps the first slice tied to one server protocol and one routine invocation while preserving the existing policy, audit and cancellation contracts. Explicit server prerequisites and operation limits prevent the UI from implying that debugging is available or safe on every PostgreSQL connection.

## Alternatives considered

- Treat debugger operations as ordinary reads: rejected because routine execution and later stepping may modify data.
- Install the extension or edit server configuration automatically: rejected because these are administrator-controlled server changes.
- Start with global breakpoints, arbitrary session attachment or other engines: deferred because they broaden session ownership and authorization beyond one selected invocation.
- Build a client-side PL/pgSQL interpreter: rejected because it would create a second, incomplete execution engine.
- Keep source lookup only: remains current behavior until this proposal is accepted and the implementation acceptance passes.

## Consequences

- Before implementation, qualify a pinned extension and supported PostgreSQL versions in an isolated fixture, including the actual protocol, privileges and session requirements. Missing extension and insufficient privileges must produce distinct unavailable states without automatic setup.
- The debugger target may write. Its invocation and each control operation require explicit policy and audit semantics, and lost acknowledgements preserve unknown outcomes without replay.
- Masking, numeric bounds and terminal states must be part of the shared contract and regression tests before UI work.
- The proposal does not authorize debugger execution or change an accepted decision. Implementation requires the relevant Policy and Data lane handoffs and maintainer acceptance of this ADR.
