# 0052: GPUI is the cross-platform frontend; GTK is maintained in parallel

- **Status**: Accepted (2026-11-11)
- **Date**: 2026-11-11
- **Supersedes**: 0028, 0036, 0043 and 0044

## Context

Eight ADRs each carry part of the GPUI story. 0028 and 0036 still describe an isolated synthetic experiment under `experiments/`. 0043 and 0044 let GPUI read real data and moved it to `apps/gpui`, keeping 0028 "as history". 0041 and 0042 were never accepted although a CI check enforces 0041. The direction appears only in Context paragraphs. The maintainer decided on 2026-11-11 what "official" means.

## Decision

1. **Direction.** GPUI is the chosen toolkit for BookiE's cross-platform frontend, instead of Tauri. The Tauri comparison prototype is removed from the repository. Cross-platform is the aim; today the GPUI app runs and is tested on Linux only.
2. **GTK stays.** The GTK4 and libadwaita application ([ADR 0002](0002-rust-gtk4-libadwaita.md)) is maintained in parallel as the Linux-native client. Neither frontend replaces the other until a later ADR says so. Both reach data only through the shared UI-free crates.
3. **Where it lives.** `apps/gpui`, package `bookie-gpui`, own lockfile until a later decision joins it to the workspace. `experiments/` holds only fixtures and scripts for it. The dependency allow-list and the check in `scripts/check-crate-boundaries.py` from [ADR 0041](0041-frontends-render-services-decide.md) apply.
4. **Reads today.** GPUI may connect to a user-chosen database and use saved connections. Data reaches it only through `bookie-app-services` and a `PolicyGuard`-wrapped connection ([ADR 0049](0049-command-event-bridge.md)). Values stay typed ([ADR 0007](0007-type-and-value-preservation.md)); credentials stay in Secret Service and never reach the frontend; denied, failed and cancelled calls reach the audit journal as they do for GTK.
5. **Writes.** A write is any statement that changes data or schema: a cell edit, insert, delete, DDL or a script. GPUI stays read-only until app-services offers the same preview, approval, overwrite check ([ADR 0024](0024-keyed-grid-writes-check-what-they-overwrite.md)), commit resolution ([ADR 0051](0051-clickhouse-write-commit-token.md)) and audit path that GTK uses, behind a command in the bridge of ADR 0049. Each write capability is enabled by its own pull request with tests that a denied statement does not run.
6. **Shipping.** No package or release includes GPUI until all of these hold: keyboard and screen-reader access is tested and documented; packaging exists for each platform it ships on; the security invariants in `AGENTS.md` are covered by tests through app-services; and the release notes policy of [ADR 0053](0053-changelog-fragments.md) names it. Until then GPUI changes carry no changelog fragment.

## Rationale

One toolkit for every platform means one renderer to maintain. Keeping GTK in parallel keeps the shipped Linux client stable while GPUI reaches parity. The refactors that moved logic into app-services and results make a second frontend cheap and keep policy in one place.

## Alternatives considered

- Tauri: rejected by the maintainer in favour of GPUI; its prototype was a comparison only.
- Replace GTK now: commits to a rewrite before accessibility and packaging evidence exists.
- Keep GPUI as a labelled experiment: no longer describes the code or the plan.

## Consequences

- ADR 0028, 0036, 0043 and 0044 are superseded and keep their text. 0039 (source pin) and 0049 (bridge) stay accepted. 0041 and 0042 are accepted by this decision.
- Two frontends cost double the UI maintenance until one is retired by a later decision.
- GPUI accessibility is unverified; the shipping condition makes that explicit.
