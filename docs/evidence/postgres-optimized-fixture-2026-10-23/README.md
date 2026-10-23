# Optimized PostgreSQL fixture profile, October 23, 2026

Related work: [#1315](https://github.com/cozyGarage/BookiE/issues/1315), [#1032](https://github.com/cozyGarage/BookiE/issues/1032). [Results and exact binary hash](results.json).

## Method

The optimized `ci` executable was taken from Forgejo run 916, GTK shard 1, built from `5b00b4ca6`. Harness checkout: `662ed2f76`. The probe runs an isolated Xvfb/Cairo display, D-Bus session, temporary configuration and keyring, and an owned SSH tunnel to the synthetic read-only PostgreSQL fixture. It stops its application and tunnel afterward. No desktop unlock or fixture mutation was required.

The probe originally labels the harness checkout as the application commit. The committed results correct that provenance: `harness_commit` identifies the imported harness; `application_commit` identifies the exact CI artifact build. The local raw output and console log remain under `target/quality/postgres-optimized-probe-20261023.*`.

RSS and peak are process observations, not live-allocation measurements. CPU seconds are process user/system ticks. Scenarios share one application process in the recorded order, so later RSS includes allocator retention and previous grid work. A single sample is not a leak diagnosis or a matched before/after benchmark. No code improvement is claimed from comparison with development-build observations.

## Results

| Scenario | Elapsed seconds | CPU seconds | RSS MiB |
| --- | ---: | ---: | ---: |
| Count | 0.724 | 0.410 | 180.08 |
| Exact numeric | 0.671 | 0.390 | 180.59 |
| Extension values | 0.688 | 0.390 | 182.45 |
| 100 rows with 257 source columns plus marker | 4.834 | 4.020 | 585.23 |
| 100 wide rows | 4.627 | 3.390 | 594.92 |
| Wide query exceeding page budget | 7.586 | 3.480 | 660.71 |
| Recovery query after refusal | 0.671 | 0.370 | 603.32 |

Baseline RSS was 172.49 MiB. All seven checks completed; the over-budget case explicitly reported an incomplete result. Exact numeric and extension values matched the synthetic fixture. Recovery succeeded after the refusal.

## Next work

The many-column case adds roughly 413 MiB of RSS even in the optimized build. Attribute live allocations and retained widget owners separately before changing grid construction. Use independent fresh-process scenarios for allocation baselines and a repeated run/close sequence for owner lifetime. Keep exact copy/export values and keyboard/accessibility behavior intact. A byte-safe PostgreSQL cursor continuation repair requires driver ownership review; lowering a fixed page row count alone is not acceptance.
