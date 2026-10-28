# Workspace staging follow-up

Status: three public-API failures reproduced against #1829 plus Linux `dd566302f`. This follow-up is stacked after #1964 and retains the existing #1826 → #1827 → #1829 implementation. Focused validation passed. Full preflight and hosted acceptance are pending.

## Negative control

The unique public regressions from #1962 were run unchanged. Five of its nine consistency tests passed. The flush failure was transferred to #1964. Three remaining tests failed:

- An unchanged existing draft retained mode `0644` rather than `0600`.
- A metadata destination that was a symlink caused publication failure, but a previously missing draft referenced by the published snapshot was created.
- Repeated symlink-destination publication failures left four draft files instead of the two published bodies.

The tests remain preserved as three integration regressions. A separate staging test checks that a missing published UUID is never reused even before publication, independently of destination prevalidation.

## Correction and durability boundary

The writer validates the metadata destination before staging draft bodies. A symlink or nonregular destination is refused before files are created. Existing reused drafts and their directories receive the same `0600` and `0700` protection as newly written files. A missing body already referenced by published metadata uses a fresh version UUID.

Once metadata publication begins, prepared bodies remain available if publication reports an error. No cleanup relies on rereading metadata while another writer could change it. An injected acknowledgement error after successful metadata publication checks that the referenced body remains readable. This pins the existing conservative publication behavior; it is not a power-loss or fsync test.

The existing retry test now expects one published file rather than an extra staged file when a symlink destination is rejected before preparation. Its old-body, unchanged-metadata and successful-retry assertions remain. This changes the earlier rejection point, not the ambiguous-publication retention rule. Errors after publication begins can still leave unreferenced prepared files; this change does not claim universal orphan reclamation.

## Scope

The fixed 8 KiB comparison buffer and symlink refusal are preserved. No framework, driver, value, policy or persistence format change is introduced. Forgejo, hosted and installed desktop acceptance remain pending. #1962 stays draft until its unique coverage has been accounted for.

## Local verification

- The three public regressions failed against the preceding stack and pass with this change.
- Disabling only the missing-published-version guard makes the new staging regression fail by recreating the referenced missing path. Restoring the guard passes.
- 409 app-services unit tests, 14 workspace model tests, 15 public persistence tests and the three transferred staging integration tests passed.
- The postcommit acknowledgement-error test pins existing conservative retention. It is not presented as a previously failing defect.
- Function-size, documentation links, issue ledger and whitespace checks passed. Full preflight is running at draft publication; package, hosted and Forgejo acceptance are not inferred from focused checks.
