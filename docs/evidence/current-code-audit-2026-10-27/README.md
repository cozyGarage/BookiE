# Current code regression evidence, October 27, 2026

Source: `644c74dd34afb35f01c62254b458c12304890573`. This packet retains temporary diagnostic probes and their outputs for [#1820](https://github.com/cozyGarage/BookiE/issues/1820), [#1821](https://github.com/cozyGarage/BookiE/issues/1821), [#1822](https://github.com/cozyGarage/BookiE/issues/1822), [#1730](https://github.com/cozyGarage/BookiE/issues/1730), and [#1824](https://github.com/cozyGarage/BookiE/issues/1824). Issue pages own current status and acceptance.

## Regression probes

`integration-probes.rs` was temporarily installed as `crates/app-services/tests/current_audit_probe.rs` and run with:

```sh
cargo test -p tablepro-app-services --test current_audit_probe -- --test-threads=1
```

Exit 101: three desired-behavior assertions failed. `integration-probes.log` records the SQLite stale edit (#1820), partial workspace publication (#1821), and unrelated corrupt draft blocking a requested restore (#1822).

`preload-race-probe.rs` was temporarily appended to `workspace_state/tests.rs` and run with:

```sh
cargo test -p tablepro-app-services --lib audit_prefetch_does_not_restore_a_deleted_cache_entry -- --exact workspace_state::tests::audit_prefetch_does_not_restore_a_deleted_cache_entry --test-threads=1
```

Exit 101: the deleted workspace was recached in the deterministic interleaving recorded in `preload-race-probe.log` (#1730). Both probe sources were removed from active test targets after execution.

## Evidence discovery

`evidence-default.log` records the default evidence scan. `evidence-explicit.log` records strict validation of the legacy source-provenance records (#1824). These outputs describe the pinned source and validation invocation; they do not establish current issue status.

## Baseline scope

`manifest.json` records source hashes, diagnostic outcomes, checks and the baseline preflight counts. The non-GTK preflight passed at the recorded source. GTK, full driver integrations, TLS, installed package, native Wayland and Forgejo were not run. The probes are defect reproductions, not passing acceptance evidence.
