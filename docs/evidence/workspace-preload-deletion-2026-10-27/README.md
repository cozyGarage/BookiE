# Deleted workspace preload race

Remaining race from [#1730](https://github.com/cozyGarage/BookiE/issues/1730). This follow-up is based on [#1826](https://github.com/cozyGarage/BookiE/pull/1826), which must land first.

A deterministic regression loads the real disk snapshot, deletes one connection through `forget_connection`, then publishes the older snapshot through the preload coordinator. Before the fix, the deleted workspace was absent on disk but readable again from memory. No timing sleeps or native UI race are used to establish this result.

Deletion now advances the existing mutation clock and records the deleted connection's generation. A preload started before deletion skips that connection. Unrelated connections still restore. Existing session-token and reimport handling remain unchanged.

All 45 workspace unit tests passed, including the failing-before regression, concurrent-save/preload tests, deletion and reimport coverage. Full preflight, persistence integration and installed acceptance are pending. This focused race fix alone does not certify the broader native workspace-freeze issue.
