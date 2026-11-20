# Work selection

Open work, status, lane and selected work order live in [Bookie Project #3](https://github.com/users/cozyGarage/projects/3). Each linked issue owns its acceptance criteria.

Release scope is set separately for each release.

## Finishing 0.2.8

The [0.2.8 milestone](https://github.com/cozyGarage/BookiE/milestone/5) owns the remaining release scope. An open issue is not wholly unimplemented: check its acceptance and merged changes before starting another implementation. In particular, grid choices/counts, session temporary tables and export maximum rows already have implemented slices; their limits are in [known limitations](known-limitations.md).

Finish and qualify the existing fixes and acceptance work before expanding the release. The remaining feature candidates are bounded multi-table export ([#3089](https://github.com/cozyGarage/BookiE/issues/3089)), data dictionary export ([#3097](https://github.com/cozyGarage/BookiE/issues/3097)) and the session temporary-table engine matrix ([#3604](https://github.com/cozyGarage/BookiE/issues/3604)). The current PostgreSQL/SQLite catalog behavior and MariaDB visibility are documented in [known limitations](known-limitations.md); #3098 tracks the current sidebar path and MariaDB open fix. Candidates remain until acceptance, lane and milestone are selected. Reuse the existing maximum-row control and PostgreSQL/SQLite session catalog paths. Database snapshots ([#547](https://github.com/cozyGarage/BookiE/issues/547)) remain separate feature work; a file copy alone does not satisfy the snapshot design.

Once the selected changes land, qualify one immutable candidate through the [validation playbook](validation-playbook.md#release-qualification) and [release process](release-process.md). New runtime results and benchmarks must name that source and its package; older runs do not qualify a changed candidate. Keep measurements and run records outside the public source tree.
