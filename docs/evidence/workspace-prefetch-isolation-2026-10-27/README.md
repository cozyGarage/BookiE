# Workspace prefetch isolation

Issue [#1822](https://github.com/cozyGarage/BookiE/issues/1822), lane UX. Base: `2174a6099`.

The public API regression failed before the fix: requesting a healthy connection returned `WriteFailed` because a different connection had an invalid UTF-8 draft. Metadata and the damaged draft remained unchanged.

Prefetch now parses global metadata once and reads draft bodies only for requested connections. A requested unreadable draft is excluded from cache publication; other requested workspaces are cached and the call returns an error. The existing GUI caller reports a restore-failure toast and continues loading connections. No write or deletion is performed by prefetch. Global malformed JSON and invalid connection IDs still refuse the complete preload. Missing drafts keep the existing empty-editor recovery behavior.

Focused validation passed: 44 workspace unit tests, then 13 persistence integration tests including invalid UTF-8, an unreadable draft path, mixed healthy/unreadable requests, missing-draft recovery, metadata refusal and preservation of existing files. Full preflight and installed acceptance remain pending. The unreadable-path test uses a directory at the draft path for a deterministic read error, not a permission-bits simulation. No installed permission-denied result is claimed.

This fix does not address atomic multi-draft publication (#1821) or the deletion/preload race (#1730). Historical audit evidence remains tied to its original source and is not rewritten to describe the fixed behavior.
