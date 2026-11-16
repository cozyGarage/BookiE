## Change

Describe the user-visible behavior or the issue this change addresses. Link the owning issue.

## Changelog fragment

For a user-facing change, add `changelog.d/<slug>.<category>.md` in the same commit (`added`, `changed`, `deprecated`, `removed`, `fixed` or `security`). One line of user impact; no paths, type or function names. Changes with no user-visible effect add no file ([ADR 0053](../docs/decisions/0053-changelog-fragments.md)).

## Reproduction and regression proof

For a behavior change, include the issue's user-visible Given / When / Then scenario and the smallest trigger. Name the regression test that verifies each observable outcome, or describe the manual acceptance step when automation cannot cover it. State what happened before the fix.

## Validation

- Tested commit:
- Local commands and results:
- Unrun, blocked or pending checks:
- Required acceptance checks: pending / complete

For UI findings, include the Linux distribution, desktop and display protocol, BookiE commit and install method, and database/version when relevant. Redact secrets and private data from logs and screenshots.
