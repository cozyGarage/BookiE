# Adoption intake - BookiE

What Step 0 measured and asked before this brownfield alignment proceeded. Updated in place on re-entry. Never recreated.

## State measured

- **self-verify drift at intake:** 4 against the built-in 5-check skeleton (20% of that skeleton). No `.standards-version` and no `standard.manifest.json`, so the number is not the distance from the full standard. Headline failures: missing version pin, `specs/`, `docs/decision-records/`, and a root/docs backlog the skeleton recognizes. `AGENTS.md` was already present.
- **Lifecycle and policy signals found:** living agent rules in `AGENTS.md` and `CLAUDE.md`; no archived/frozen repo marker. Delivery branch is `linux`. Rename boundary (`tablepro` / `com.tablepro.linux` identifiers stay) is recorded in AGENTS, README and the sprint.
- **Existing-knowledge sources offered:** in-repo only, all reachable. `AGENTS.md`, `linux/ARCHITECTURE.md`, `linux/docs/decisions/` (0001–0015), `linux/docs/known-issues.md`, `linux/docs/bookie-0.2-sprint.md`, `linux/docs/README.md`, `linux/docs/validation-playbook.md`, `linux/CHANGELOG.md`. No wiki. GitHub and Forgejo hold execution state for pull requests and merge tiers.

## Question round

- **Intent:** bring the existing BookiE tree to the standard gradually. Adopt where it helps. No big-bang switch. Bridge files point at the living homes; they do not replace them in early waves.
- **Technology:** Rust 1.98, GTK4, libadwaita, Relm4, static database drivers. Layer 2 Node stack declined forever unless a web surface is added later.
- **Appetite:** a programme of small waves (assessment, bridge scaffold, decision checklist, first specs, advisory verify). One concern per wave. Do not mix leftover README/ARCHITECTURE trim into the first standards change unless chosen later.
- **Profile:** `core`. BookiE has a public release audience, so a later wave may flip to `scale` or keep `core` with recorded exceptions. That flip is a BDR/ADR, not this intake.
- **Tracked-work location:** both, bridged. Product work stays in `linux/docs/known-issues.md` plus the sprint. Alignment work is counted in `docs/backlog.md` / `docs/alignment-backlog.md`. GitHub and Forgejo remain the PR and merge trackers.
- **Plan-only or execute:** execute the locked gradual plan through Wave 4. Do not commit unless asked.
- **Evidence:** keep the run in this repository. Do not send an anonymised transcript upstream unless the owner asks later.

## Re-entry log

- 2026-10-10: first intake. Gradual brownfield, profile `core`, Layer 2 declined, AGENTS / linux ADRs / Forgejo stay the living authority.
- 2026-10-11: owner asked to implement waves 0–4 of the locked plan without editing the plan file.
