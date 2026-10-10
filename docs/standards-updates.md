# Updating the repository standard

The bookmark is [`.standards-version`](../.standards-version). It records the state this tree last aligned to. It is not a version to stay on. Latest is the only target.

## What is vendored

`scripts/self-verify.mjs` and the other `scripts/*.mjs` helpers are copies of the standard at that bookmark. `SPEC.md` is a verbatim copy. `standard.manifest.json` is that release's manifest with `profile` set to `core` and the recorded `exceptions`.

The full standard checkout is gitignored at `.repository-standards/` (degit of `repository-standards/core`). Use it to read method docs and to compute a delta. Do not commit it.

## How to take the next delta

1. Refresh the cache: `npx --yes degit repository-standards/core .repository-standards`.
2. Read the standard's `CHANGELOG.md` and `update-to-latest` notes between this bookmark and latest.
3. Copy changed `copy`-class files (guards, `SPEC.md`) so their hashes match the new manifest.
4. Merge the new `standard.manifest.json`, keep `profile: "core"` until A-8 says otherwise, and keep the existing `exceptions` unless an exception is no longer true.
5. Write the new version into `.standards-version`.
6. Run `node scripts/self-verify.mjs --warn --profile core` and update [`adoption-assessment.md`](adoption-assessment.md) / [`backlog.md`](backlog.md) if the drift set changed.
7. Do not re-scaffold PRODUCT, personas, AGENTS, linux ADRs, or Forgejo workflows as part of an update.

Do not enable a blocking GitHub or Forgejo job on drift 0 until alignment item A-9 is an explicit decision.

## Advisory check

```bash
bash scripts/standards-drift.sh
```

The GitHub workflow `.github/workflows/spec-guard.yml` runs the same warn-mode check. It must not fail the job. Forgejo merge tiers are unchanged.
