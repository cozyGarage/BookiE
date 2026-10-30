# 0026: Rotate the audit journal into verified segments

- **Status**: Proposed (2026-10-30)
- **Date**: 2026-10-30

## Context

`audit.jsonl` is one append-only file of hash-chained records (`seq`, `prev_hash`, `hash`, `event`). It never rotates. Each start reads the whole file to find the tail and to recover pending write intents, `recent()` and `verify_chain` scan it from the start, and a legacy-format check reads it fully into memory. A busy agent or a long-lived profile makes start-up time, memory and the verification cost grow with the age of the install.

Audit records are evidence. They must not be dropped silently, and the chain must stay verifiable end to end. ADR 0010 and the guard rules require every write intent to reach a terminal outcome, and start-up recovery turns an intent without an outcome into an unknown outcome that blocks governed writes on the affected connection.

## Decision

Rotate the active file by size into immutable segments that stay chained.

1. When an append finds the active file at or above 64 MiB, and the append happens under the existing exclusive lock, the writer renames the active file to `audit.jsonl.<first-seq>` (zero-padded to 20 digits so names sort), sets mode 0600, syncs it and its directory, then starts a new `audit.jsonl`. The first record of the new file continues the chain: its `seq` follows the last record of the closed segment and its `prev_hash` is that record's `hash`. No new record kind is added.
2. Rotation is deferred while the journal has a pending write intent or an unresolved outcome, so the active file always holds every open intent. Start-up recovery therefore reads only the active file. A deferral is bounded because every intent reaches a terminal state; the file may exceed the limit until then.
3. Verification walks the segments in `seq` order and the active file last, checking each file's hash chain and that each segment starts where the previous one ended. A missing, reordered or edited segment fails verification with the segment name. `recent()` reads the active file first and older segments only until it has enough records.
4. Start-up stops reading the whole file. It reads the tail of the active file for the cursor and parses only the first record to detect the legacy format. A legacy journal is still verified in full once, then rotated aside as it is today.
5. Segments are never deleted by the application. Disk use is bounded by the user archiving or removing old segments after verifying them; removing the oldest segments is allowed and verification then starts at the earliest remaining segment, reporting that earlier history is absent. Retention settings are out of scope.
6. Files and the directory keep their current permissions (0600 files, 0700 directory). Rotation never opens a path through a symbolic link.

## Rationale

Keeping the chain across files means one verification rule and no checkpoint file to trust. Deferring rotation while intents are open keeps recovery simple and correct without carrying state between segments. Never deleting records matches the evidence purpose; size, not age, bounds the work done at start-up.

## Consequences

- Start-up and append cost depend on the active segment size, not the install age.
- `AuditJournal` readers that assume a single file (the Audit Log view, export, verification tools and tests) must learn about segments.
- A journal written by this version cannot be read by an older build that expects a chain starting at sequence 1. The journal format version is raised and an older build refuses a newer journal with a clear message.
- Tests must cover: rotation at the limit, deferral with a pending intent and rotation after its outcome, a crash between rename and the first record of the new file (the next start finds no active file or an empty one and continues from the last segment), tampering with and removal of a middle segment, concurrent writers across a rotation, `recent()` across a boundary, and start-up recovery after rotation.
- Installed acceptance must show the Audit Log view and `bookie-agentd` working across a rotated journal.

## Alternatives considered

- **Delete or truncate old records.** Rejected: audit evidence must not disappear without the user acting.
- **A checkpoint file holding the last hash.** Rejected: it adds a second file whose integrity must also be protected and recovered.
- **SQLite for the journal.** Rejected: ADR 0010's append-only, hash-chained text format is easy to inspect and verify outside the application.
- **Rotate by age.** Rejected: it does not bound work on a busy profile.
