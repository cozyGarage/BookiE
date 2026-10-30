# Large-query rendering investigation, October 21

Source base: `9f5dbd47e`. Owner: [#1272](https://github.com/cozyGarage/BookiE/issues/1272). This follows the archived [grid ownership investigation](../../archive/evidence/grid-lifetime-2026-10-21/README.md); it does not replace its pinned heap profiles.

## Reproduced cause and change

Initial streamed-result rendering and live page appending constructed a full grid for each retained page, even while hidden behind the pager. A GTK regression fails on the original implementation because a hidden page already contains a ColumnView. The changed path creates only a lightweight page holder until that page is first displayed. The existing grid builder then runs once. Switching away and back preserves grid identity and local state. The builder consumes captured inputs and never captures its owning widget.

Fetching, policy checks, cancellation, result values, page retention and export behavior are unchanged. Pages that have already been displayed can retain their grids until eviction or close. Selecting a page still performs grid construction on the GTK thread. Eviction can select and build a surviving page. This is reduced unnecessary work, not a CPU ceiling or aggregate byte budget.

## Verification and limits

The failing-before GTK log and passing-after runs are retained in the owning worktree under `.analysis/large-query/`. The new case covers a realized selected page, an unbuilt hidden page, first selection and grid reuse on remap. Existing grid-release and composed-stream ownership cases remain registered. Fresh preflight and all 95 isolated GTK cases passed, including the terminal-status lazy-loading case and existing ownership checks. App all-target Clippy and documentation/inventory checks passed.

No comparable end-to-end throughput or CPU percentage is claimed. Native Wayland, broad large-file responsiveness and the aggregate page byte budget remain separate acceptance. CPU and memory evidence must distinguish stored rows, built grid/model objects, native caches and active database work.
