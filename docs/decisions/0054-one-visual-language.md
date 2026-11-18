# 0054: One visual language for both apps

- **Status**: Proposed (2026-11-18)
- **Date**: 2026-11-18
- **Supersedes**: part of 0042 (point 3, that the GTK application keeps libadwaita's look)

## Context

[ADR 0042](0042-dbx-visual-reference-for-gpui.md) made dbx the reference for GPUI and kept libadwaita's look for GTK. The maintainer wants both apps to look like dbx and speak like TablePro and macOS: simple, elegant, symbols before words, designed dropdowns, buttons and colours. The GTK app does not match that yet: its editor toolbar carries six text buttons, spacing is hard-coded in 355 places, and 21 dialogs build their own shell.

## Decision

1. Both apps follow [UI design](../ui-design.md): dbx's neutral tokens and radii, TablePro's icon-first, low-text language.
2. GTK keeps libadwaita as its widget toolkit and restyles it with the tokens; it does not draw widgets by hand.
3. The building rules in the guide apply to new and changed UI code: extract a shared part at the third copy, spacing from the scale, state passed in.
4. A shared part replaces its copies in one pull request ([AGENTS.md](../../AGENTS.md), reuse before building).

## Alternatives considered

- Keep libadwaita's look in GTK: lowest effort, but the two apps then never share a language.
- Draw custom widgets in GTK: closest to dbx, but loses accessibility, theming and input handling from the toolkit.
- Extract shared parts now, before three callers agree: rejected, a premature abstraction is wrong more often than a third copy is.

## Consequences

- ADR 0042 point 3 no longer holds once this is accepted; its other points stand.
- Existing screens move to the new language one pull request at a time, starting with the editor toolbar, dialog shell and result view.
- A baseline check for literal spacing and `thread_local!` in UI code follows, in the repository's usual ratchet style.
