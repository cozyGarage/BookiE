# Historical local validation

Archived from `aeac107a4` on 2026-10-03. Reports below retain their original
SHA/scope. Missing cache output cannot establish current acceptance.
Current commands and evidence rules live in [the playbook](validation-playbook.md).

## October 1–2 local checks

The full layer passed on clean commit `8e7766c`; its report is
[`20261001T235848303064Z-layers/report.json`](../target/quality/20261001T235848303064Z-layers/report.json).
The extended layers have retained individual reports from the same review
sequence; the Docker driver/value runs used the preceding production tree, and
`8e7766c` only adds XLSX cutoff assertions and review documentation.
The app-server/value report is
[`20261001T231537044614Z-layers/report.json`](../target/quality/20261001T231537044614Z-layers/report.json);
Docker drivers, TLS, PostgreSQL release, keyring and security-policy are in
[`20261001T232237703877Z-layers/report.json`](../target/quality/20261001T232237703877Z-layers/report.json).
GTK, audit, workflow/harness and packaging results are retained in the later
per-layer reports under `target/quality/`; each report records its selected
layers, source commit and dirty-tree state.
The October 2 base report is
[`20261001T232237703877Z-layers/report.json`](../target/quality/20261001T232237703877Z-layers/report.json).
The GTK widget and UI safety layers, both supply-chain checks, workflow lint,
test harness, change-contract and Debian packaging layers passed after
installing Xvfb, `cargo-audit`, `python-atspi`, and `dpkg`. The current
`linux/target` already reuses compiled artifacts; it was 82 GiB, including
45 GiB of incremental state, so preserve it for reuse and review cache cleanup
separately.

