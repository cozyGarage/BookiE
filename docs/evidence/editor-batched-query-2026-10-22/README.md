# Batched editor query publication, 2026-10-22

Tracks [#1344](https://github.com/cozyGarage/BookiE/issues/1344).

The failing-first isolated GTK regression selected 100 Unicode SQL lines and invoked Toggle Line Comment. The original implementation emitted 100 complete query-change outputs, each representing an intermediate script. The handler copied the full buffer for every changed signal despite the operation being one GTK user action.

The candidate tracks user-action nesting. Buffer changes still invalidate the generation immediately. Full query publication, fold clearing and the changed-handler completion refresh occur once when the outer action ends. Changes outside a user action publish immediately. A no-op action publishes nothing. Cursor-position completion refresh is unchanged and may still perform repeated advisory analysis.

The initial 100-line regression passed after the change: one final output, exact commented Unicode text, and one Undo restoring the original. The nested-action, no-op and ordinary-edit regression also passed: publication waits for the outer action, an empty action publishes nothing, and an ordinary insert publishes immediately. Full preflight and all 713 application unit tests passed. The GTK widget tier and final all-target Clippy remain pending. This is a verified reduction in full-query publications, not a measured whole-app CPU improvement or an explanation of the native freeze.

Raw before/after logs are retained under `~/bookie-test/editor-batch-*`. All widget interaction uses isolated Xvfb displays, with no native desktop input.
