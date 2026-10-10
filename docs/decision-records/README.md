# Decision records

Living ADRs stay in [`linux/docs/decisions/`](../../linux/docs/decisions/README.md). This directory is an index, not a second log. Do not move those files until a later change (alignment item A-10) says to.

BookiE records use a short ADR shape (Status, Date, Context, Decision, Rationale, Consequences, Alternatives). The standard's paved form is MADR. New records keep the existing BookiE shape so the linux index and `check-doc-links.py` keep working.

There is no BDR stream yet. Product forks that need one are listed on the [checklist](checklist.md) and in the [alignment backlog](../alignment-backlog.md).

## Technical records (ADR)

| Record | Title | Status | Home |
|---|---|---|---|
| linux ADR 0001 | Database drivers are statically linked | Accepted | [linux](../../linux/docs/decisions/0001-no-plugin-system.md) |
| linux ADR 0002 | Rust, GTK4, and libadwaita | Accepted | [linux](../../linux/docs/decisions/0002-rust-gtk4-libadwaita.md) |
| linux ADR 0003 | Relm4 for application architecture | Accepted | [linux](../../linux/docs/decisions/0003-relm4-architecture.md) |
| linux ADR 0004 | Secret Service through oo7 | Accepted | [linux](../../linux/docs/decisions/0004-libsecret-secret-storage.md) |
| linux ADR 0005 | Cancellation must reach the database | Accepted | [linux](../../linux/docs/decisions/0005-server-side-cancellation.md) |
| linux ADR 0006 | A driver panic is contained, not trusted away | Accepted | [linux](../../linux/docs/decisions/0006-driver-panic-containment.md) |
| linux ADR 0007 | Native type and value preservation | Accepted | [linux](../../linux/docs/decisions/0007-type-and-value-preservation.md) |
| linux ADR 0008 | Connection/session ownership and trust | Accepted | [linux](../../linux/docs/decisions/0008-connection-and-session-ownership.md) |
| linux ADR 0009 | Durable identity and recoverable state | Accepted | [linux](../../linux/docs/decisions/0009-persistence-and-identity-compatibility.md) |
| linux ADR 0010 | Audit confirmed bundle administration | Accepted | [linux](../../linux/docs/decisions/0010-administrative-action-audit.md) |
| linux ADR 0011 | Page large editor results through a cursor | Accepted | [linux](../../linux/docs/decisions/0011-paged-query-results.md) |
| linux ADR 0012 | Show database objects in a collapsible sidebar tree | Accepted | [linux](../../linux/docs/decisions/0012-sidebar-object-tree.md) |
| linux ADR 0013 | Preserve every result set from a query | Accepted | [linux](../../linux/docs/decisions/0013-multiple-query-result-sets.md) |
| linux ADR 0014 | Full table exports use one read snapshot | Accepted | [linux](../../linux/docs/decisions/0014-full-table-export-snapshot.md) |
| linux ADR 0015 | Database snapshots and restore | Proposed | [linux](../../linux/docs/decisions/0015-database-snapshots.md) |

## Business records (BDR)

None yet. Free / no-account is stated in [PRODUCT.md](../../PRODUCT.md) until a BDR is written (A-1, A-14).
