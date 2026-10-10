# Alignment backlog

Counted remaining work to full alignment with repository-standards 1.0.19 at profile `core`. Product work stays in the [ledger](../linux/docs/known-issues.md). The Gate 5 scope block that must add up lives in [`docs/backlog.md`](backlog.md).

Owner roles are `product`, `architect`, `dev` or `agent`.

| # | ID | Item | Owner | Notes |
| --- | --- | --- | --- | --- |
| 1 | A-1 | Review inferred `PRODUCT.md` | product | Drafted from README and the sprint. |
| 2 | A-2 | Review inferred `docs/personas.md` | product | Drafted from UI, MCP and packaging surfaces. |
| 3 | A-3 | Review extracted PolicyGuard and SSH specs | architect | Built from code citations, not a product rewrite. |
| 4 | A-4 | Spec browse and grid | architect | Later wave. Ledger stays the work tracker. |
| 5 | A-5 | Spec storage and keyring | architect | Later wave. |
| 6 | A-6 | Spec packaging | architect | Later wave. |
| 7 | A-7 | Declare restatements in `docs/facts.json` | architect | After README / ARCHITECTURE trim settles. |
| 8 | A-8 | Record `core` versus `scale` as a BDR or ADR | architect | Public audience exists; do not flip the profile in passing. |
| 9 | A-9 | Promote advisory self-verify to a merge gate | architect | Only after an explicit choice. Never change Forgejo to do this quietly. |
| 10 | A-10 | Optional move of ADRs under `docs/decision-records/` | architect | Separate change after indexes are stable. |
| 11 | A-11 | Security baseline ADR for the R19 axes | architect | AGENTS already states the in-code invariants. |
| 12 | A-12 | Data retention and GDPR BDR | product | Undecided. Do not invent a retention period. |
| 13 | A-13 | Accessibility / WCAG baseline for the GTK UI | product | Undecided. Adwaita is not a recorded AA claim. |
| 14 | A-14 | Positioning and messaging one-liner BDR | product | Free / no-account already lives in PRODUCT; market phrasing does not. |
| 15 | A-15 | Port lifecycle skills to Cursor, or keep the skills exception | agent | `.claude/skills` is excepted on purpose. |
| 16 | A-16 | Record integration method (rebase versus squash) | architect | R23 fork. Current practice is not written as a decision. |
| 17 | A-17 | Optional gitleaks GitHub job | dev | `cargo deny` already reviews Rust advisories. |
| 18 | A-18 | Name Flathub and distro review as an external release gate | architect | PKG-4 / packaging already track the work; the fork is unrecorded. |
