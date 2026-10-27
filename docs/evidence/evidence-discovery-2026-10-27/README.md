# Declared evidence discovery

Discovery portion of [#1824](https://github.com/cozyGarage/BookiE/issues/1824). Historical provenance recovery remains open.

The default scan previously inspected only `manifest.json`. Two failing-before regressions prove that a supported record under another filename was omitted and an invalid digest in `validation.json` did not fail the default scan. Discovery now inspects declared schema-version-one and source-digest records across JSON filenames. Named manifests remain included even when invalid; unrelated arrays are ignored.

All 48 manifest harness tests passed after the fix. The default repository scan passed while reporting 187 schema-version-one records, 19 legacy provenance packets and 136 historical/source-drift warnings. This larger count is discovery, not repaired provenance. No historical source digest or claim was modified. Full preflight remains pending.
