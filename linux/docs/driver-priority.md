# Adding drivers (order and maturity)

Drivers stay statically linked ([ADR 0001](decisions/0001-no-plugin-system.md)).
Capability status for each engine lives in [driver-maturity.md](driver-maturity.md).

Persona order used for the Stage 5 engines:

1. ClickHouse (Stable)
2. Redis (Experimental)
3. DuckDB (Experimental, `--features duckdb`)
4. MongoDB (Experimental)

Use [the driver implementation checklist](adding-drivers.md#implementation-checklist)
for crate registration, capabilities, mapping and native tests. This page owns
historical priority only; it does not maintain another implementation checklist.

SSH jump-host chains belong in `crates/ssh` (multi-hop via sequential tunnels), not in drivers.
