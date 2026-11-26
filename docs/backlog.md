# Work selection

Open work, status and selected work order live in [Bookie Project #3](https://github.com/users/cozyGarage/projects/3). Each linked issue owns its acceptance criteria.

Release scope is set separately for each release.

## Current release work

The [0.3.0 milestone](https://github.com/cozyGarage/BookiE/milestone/6) owns the scope of the next release. An open issue is not wholly unimplemented: check its acceptance and merged changes before starting another implementation. Implemented slices and their limits are in [known limitations](known-limitations.md).

New UI work targets the GPUI app in `apps/gpui`. The GTK app receives only security, data-integrity and release-blocking fixes; GTK-only issues use the `gtk-historical` label ([ADR 0058](decisions/0058-gpui-primary-gtk-historical.md)). Issues in shared crates do not get that label only because GTK also uses the code. GPUI becomes a supported release only after it passes the ADR 0058 gate.

The session temporary-table engine matrix ([#3604](https://github.com/cozyGarage/BookiE/issues/3604)) and database snapshots ([#547](https://github.com/cozyGarage/BookiE/issues/547)) remain backlog feature work. A file copy alone does not satisfy the snapshot design.

Once the selected changes land, qualify one immutable candidate through the [validation playbook](validation-playbook.md#release-qualification) and [release process](release-process.md). New runtime results and benchmarks must name that source and its package; older runs do not qualify a changed candidate. Keep measurements and run records outside the public source tree.
