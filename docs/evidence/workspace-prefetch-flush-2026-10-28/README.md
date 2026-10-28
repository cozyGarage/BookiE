# Workspace prefetch flush failure

Status: local regression reproduced and corrected. This change is stacked after #1826, #1827 and #1829. Hosted and Forgejo acceptance remain pending.

Prefetch previously discarded a failed workspace flush, then read an older snapshot from disk. A cached save issued before prefetch had the same mutation clock as the preload's starting clock, so the older snapshot replaced the newer cached draft.

The coordinator now returns the flush error before loading disk. A disconnected acknowledgement maps to writer unavailability. Saves remain queued for a later retry.

The legacy-metadata regression uses a regular file in place of the drafts directory. Before the fix, prefetch returns success and replaces the newer cached SQL. After the fix it returns `WriteFailed` and retains the cached SQL. A separate public regression blocks metadata publication, verifies the pending draft survives, repairs the path and verifies successful retry.

## Local checks

- `cargo test --locked -p tablepro-app-services --lib workspace_state::tests`: 43 passed.
- `cargo test --locked -p tablepro-app-services --test workspace_persistence`: 15 passed.
- Documentation links, issue ledger and whitespace checks passed.
- Full preflight was started and remains running at draft publication. This is not a claim of full acceptance.

## Related review

The existing #1826 → #1827 → #1829 stack remains authoritative. #1962 duplicates that stack and remains draft while unique tests are reviewed. Its nine public consistency probes were run unchanged against #1829 plus Linux `dd566302f` before this follow-up: five passed and four failed. The flush race is corrected here.

Three additional failures remain outside this focused change:

- An unchanged draft with mode `0644` remained `0644` after save instead of being tightened to `0600`.
- A failed metadata publication created a previously missing draft at the UUID referenced by the old snapshot.
- Repeated failed metadata publications retained four files where only the two published bodies were expected.

These observations are filesystem/public-API evidence, not installed desktop acceptance. Their tests remain preserved in #1962 until transferred to a focused follow-up. No Forgejo gate or merge was performed.
