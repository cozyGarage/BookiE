# Declared evidence discovery

Discovery portion of [#1824](https://github.com/cozyGarage/BookiE/issues/1824). Historical provenance recovery remains open.

The default scan previously inspected only `manifest.json`. Two failing-before regressions prove that a supported record under another filename was omitted and an invalid digest in `validation.json` did not fail the default scan. Discovery now inspects declared schema-version-one and source-digest records across JSON filenames. Named manifests remain included even when invalid; unrelated arrays are ignored.

All 48 manifest harness tests passed after the fix. The default repository scan passed while reporting 187 schema-version-one records, 19 legacy provenance packets and 136 historical/source-drift warnings. This larger count is discovery, not repaired provenance. No historical source digest or claim was modified. Full preflight remains pending.

Provenance review recovered six already-recorded base identifiers as full Git commit IDs. Original `base` fields and all source fingerprints remain unchanged. Each record explicitly states that a base and file hashes do not establish a clean complete run snapshot. Five validation packets have no recoverable recorded base and remain unresolved in [the review ledger](provenance-review.json); no tested snapshot is inferred from matching file bytes or a later merge. The wider scan also finds other historical records outside this initial validation-packet review. Issue #1824 remains open for that provenance work.
