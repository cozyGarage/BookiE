# Changelog fragments

A user-facing change adds one file here, in the same commit: `<slug>.<category>.md`, where the category is `added`, `changed`, `deprecated`, `removed`, `fixed` or `security`. The file holds one line of user impact (no paths, type or function names). A change with no user impact adds none. A change that only touches `apps/gpui` adds none until GPUI ships ([ADR 0052](../docs/decisions/0052-gpui-is-the-cross-platform-frontend.md)).

Example: `grid-column-widths.changed.md` containing `The grid starts each column at a width that suits its type.`

`python3 scripts/release-notes.py fragments` validates the files. `python3 scripts/release-notes.py release X.Y.Z YYYY-MM-DD` moves them into `CHANGELOG.md` and deletes them ([ADR 0053](../docs/decisions/0053-changelog-fragments.md)).
