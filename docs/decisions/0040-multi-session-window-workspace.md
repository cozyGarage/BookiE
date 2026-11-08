# 0040: Multi-session windows keep a headless workspace model

- **Status**: Proposed
- **Date**: 2026-11-08

## Context

BookiE is effectively one live connection per window. Opening a second saved connection means another window. Users who keep several databases open at once want their tabs side by side under one shell. Issue [#2499](https://github.com/cozyGarage/BookiE/issues/2499).

[ADR 0018](0018-headless-app-services-crate.md) already places the tab model and workspace persistence in `bookie-app-services`. [ADR 0008](0008-connection-and-session-ownership.md) keeps saved UUID, live connection, editor session and selected namespace distinct. [ADR 0016](0016-governed-consumer-seam.md) requires every consumer handle to be a `PolicyGuard`. [ADR 0003](0003-relm4-architecture.md) keeps Relm4 as the Linux UI architecture. UX chrome must not invent a second session model before those contracts are recorded.

## Decision

A window may hold several open connection sessions. The headless multi-session model lives in `bookie-app-services`. Relm4 and GTK remain the Linux shell only: tab view, sidebar chrome, dialogs, toasts and bindings.

### Ownership

- Each open session keeps its own workspace state and its own tabs. A tab is born under one connection and never moves to another.
- The window owns an ordered map of open sessions and a front session. The front session is the connection of the selected tab. Header actions, Safe Mode and policy chrome follow the front session.
- Sessions are not shared across windows. Opening a connection that is already open elsewhere brings that window and session forward.
- MCP-opened sessions stay outside the user's working session list for that window. They keep today's isolation posture and still hold only `PolicyGuard`-wrapped connections.

### Policy and identity

- Every live connection the window exposes is a distinct `PolicyGuard` (ADR 0016). There is no shared guard across connections and no raw driver handle for UI or MCP consumers.
- Uncertainty, audit generation and dedicated editor sessions remain per live connection (ADR 0008). Switching the front session never clears another connection's uncertainty or audit obligations.

### Restore and close

- Open-sessions restore records the saved connection keys grouped by window. Older single-key records remain readable and restore as one session in that window.
- Per-connection workspace tab records stay keyed by saved connection id in the existing workspace store. This decision does not rewrite that format.
- Window close asks Save Changes once across every open session. Cancel keeps all sessions. Disconnect of one session prompts and closes only that session's tabs.
- A failed connect while other sessions are open stays in the same window (toast with details). It does not open a new window or replace the shell.

### Shell boundary

- `bookie-app-services` owns: session membership, tab↔connection maps, front-session selection, close/save aggregation counters, and the open-sessions restore shape.
- `apps/linux` owns: Relm4 components, `AdwTabView` page order, sidebar tree widgets, connect progress UI, and the Save Changes dialog. It does not hold a second source of truth for which tab belongs to which connection.

## Rationale

Putting the maps and restore shape in app-services keeps multi-session behaviour testable without a display and keeps experimental or headless consumers on the same model ([ADR 0018](0018-headless-app-services-crate.md)). Leaving Relm4 as shell-only preserves [ADR 0003](0003-relm4-architecture.md) and avoids encoding GTK widgets into the session contract. One guard per connection preserves the governed seam when several databases share a tab bar.

## Consequences

- UX may implement multi-connection chrome only after this record is accepted.
- Acceptance work must cover front-session switching, cross-session tab close, restore of multi-key and older single-key records, and MCP isolation from user working sessions.
- Tab labels name their connection while more than one session is open in the window.
- Implementation stays inside the Cursor/UX documentation-then-app sequence: no product code lands ahead of acceptance of this decision unless a tiny stub is required to keep formats readable.

## Alternatives considered

- **One window per connection (status quo).** Rejected for the product ask in #2499: tabs of two connections cannot sit side by side.
- **One tab bar per session inside one window.** Rejected: switching connection hides the other connection's tabs.
- **Swap the window's single session in place.** Rejected by the same ownership concerns as ADR 0008: every consumer that read the session would need a reset, and a late reply could land in the next session.
- **Put the multi-session maps only in Relm4 component state.** Rejected: duplicates ADR 0018's headless boundary and blocks display-free tests.
- **One PolicyGuard or shared live identity for every session in the window.** Rejected: crosses connection ownership and audit generation (ADR 0008, ADR 0016).
- **Drop Relm4 for a different shell.** Out of scope; BookiE keeps ADR 0003.
