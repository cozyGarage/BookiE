# Evidence discovery and provenance review, October 27, 2026

Fix for [#1824](https://github.com/cozyGarage/BookiE/issues/1824), Linux base `d7f9273be1002766f621f876630369231a432e50`.

The default validator previously discovered only `manifest.json`. Eight focused discovery tests run against the previous validator produce eleven assertion/subtest failures ([negative control](negative-control.txt)); malformed legacy packets, invalid or incomplete execution reports, and valid reports under other names were omitted. The fixed discovery scans JSON records for supported source/report declarations, preserves explicit-path behavior, skips unrelated data and unsupported schemas, and still reports malformed conventional evidence names.

The focused evidence suite passes 54 tests. Baseline non-strict validation discovers 185 schema-v1 records and 19 legacy source-provenance packets on this source; warnings remain for historical limitations. Discovery includes records, not acceptance of every claim inside them. Strict mode remains available for complete provenance checks.

## Historical packet disposition

The omitted recent `validation.json` packets are unversioned legacy source records, correcting their original schema-v1 classification in the audit. Eleven lack an exact source/base commit for the executed run. Their 37 recorded file hashes were matched to Git blobs reachable through BookiE GitHub remote refs. These mappings are appended as `source_commits`; original hashes, counts, results and gate claims are unchanged.

[The disposition](provenance-review.json) lists every packet. Each now explicitly states that its complete run snapshot was not recorded. A per-file mapping does not prove all those files were executed together. Missing run identity continues to be reported and to fail strict validation; the provenance note is not a waiver. No replacement run identity was invented.

Validation and hosted-gate state are recorded on the PR. This change does not rerun the historical product/GTK claims or certify a release.
