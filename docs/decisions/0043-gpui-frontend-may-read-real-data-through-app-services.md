# 0043: The GPUI frontend may read real data through app-services

- **Status**: Accepted (2026-11-09)
- **Date**: 2026-11-09

## Context

[ADR 0028](0028-isolated-frontend-experiments.md) keeps experimental frontends on synthetic data and says that connecting one to real data needs a separate decision covering typed values, policy, audit, accessibility and platform acceptance. [ADR 0036](0036-experimental-frontends-may-read-a-disposable-sqlite-fixture.md) allows one disposable SQLite file the experiment creates itself. The maintainer has chosen GPUI as the direction for the next frontend (2026-11-09), so the GTK application stays the shipped product ([ADR 0002](0002-rust-gtk4-libadwaita.md)) only until a GPUI client is accepted. The GPUI experiment now answers Find and filter through `bookie-results` ([ADR 0041](0041-frontends-render-services-decide.md)). It cannot be judged on real workloads while it reads only generated rows.

## Decision

1. The GPUI experiment may connect to a user-chosen database, and may use saved connections, once all of the following hold. Until then ADR 0028 and ADR 0036 apply unchanged.
2. Data reaches the frontend only through `bookie-app-services` and a `PolicyGuard`-wrapped connection ([AGENTS.md](../../AGENTS.md), security invariants). The experiment never depends on a driver, `bookie-storage` or `bookie-transport` directly; the check from ADR 0041 enforces this.
3. Values stay typed end to end ([ADR 0007](0007-type-and-value-preservation.md)). The frontend renders bounded display text and keeps exact values for copy and edit.
4. Credentials stay in Secret Service through `bookie-storage`. The frontend receives a connection handle, never a secret, and never logs SQL parameters or results.
5. Writes from the experiment stay disabled until a later decision. Reads, Find, filter, compare and export go through the same policy, approval and audit path as the GTK client, including terminal audit states for denied, failed and cancelled calls.
6. The experiment stays under `experiments/`, outside packages and the shipped workspace. Shipping it needs its own decision with accessibility and platform acceptance evidence.

## Rationale

Synthetic rows cannot show how typed values, slow servers, cancellation, policy denials and large catalogs behave in a GPUI client. Routing everything through app-services keeps one policy and audit path, so the experiment cannot weaken a security invariant.

## Alternatives considered

- Stay synthetic: no evidence on real workloads, and the frontend choice stays a guess.
- Let the experiment use drivers directly: bypasses `PolicyGuard`, which AGENTS.md forbids for any consumer path.
- Allow writes now: needs the edit, preview and conflict path from the GTK client moved into services first.
- Wait for accessibility proof before any real read: that proof needs a client that can open real data.

## Consequences

- app-services needs a connection and query facade that a non-GTK frontend can call; today some of that lives in `apps/linux/app/src/services`. Moving it follows ADR 0018 and ADR 0041.
- Accessibility with GPUI is unverified; acceptance must say how screen-reader and keyboard access are tested.
- Dependency review for the experiment grows with app-services' graph.
