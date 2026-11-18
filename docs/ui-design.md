# UI design

Read this before changing any widget, dialog, toolbar or colour in the GTK or GPUI app. It is short on purpose. Decision: [ADR 0054](decisions/0054-one-visual-language.md). Values: [DESIGN.md](../DESIGN.md). Review lists: [ui-review-checklist.md](ui-review-checklist.md).

## The look and the language

The look is dbx: near-monochrome neutrals, thin borders, 4 px control radius, 6 px surface radius, compact density. The language is TablePro and macOS: quiet, icon first, few words.

| Part | Rule |
| --- | --- |
| Toolbar | Icons with tooltips. A text label only for the one primary action (Run) or where an icon would be ambiguous. Secondary actions go in one overflow (⋮) menu. |
| Buttons | One filled button per surface (the primary action). Everything else is flat. Destructive actions are red text and sit last in a menu. |
| Mode switches | A segmented control (Data / Structure, Tree / Text), never two separate buttons. |
| Dropdowns | A popover menu: icon, short label, shortcut on the right, a check for the current choice, separators between groups, at most seven items per group (a BookiE rule; GNOME allows 3 to 12 per menu). |
| Colour | Semantic tokens only: background, surface, border, text, muted text, info, success, warning, destructive. The filled button is neutral, as in dbx; the system accent is for focus and selection only. No other blue, no raw hex in widget code. |
| Text | Sentence case, one to two words on a control, the explanation goes in the tooltip. An error says what happened and what to do next. |
| Badges | Small, one per fact (the environment, read-only). |
| Density | Two scales: 32 px form controls, 24 px compact toolbars, tabs and menus with 14 px icons. One scale per surface. Table rows compact by default. |
| Zoom | Result views zoom (Ctrl+scroll, Ctrl+plus, Ctrl+minus, Ctrl+0). Chrome does not. |

GTK uses libadwaita widgets restyled to these tokens in `apps/linux/data/resources/style.css`. GPUI takes the tokens from its theme and follows `gpui-component`'s design and coding guides.

## Building rules

1. **Write it twice, extract it the third time.** The first and second copy may be written in place. The third copy is the signal to write one shared part, in the same pull request that removes the copies. Do not build the part before the third use.
2. **Shared parts have one home.** Dialog shell, result view (text, list, table), list row, status line. Zoom, the output font and spacing live in the part, not in each caller.
3. **Spacing comes from the scale:** `ui::spacing` holds 2, 4, 8, 12, 16, 24 as named constants. A margin or spacing is written as a constant, never a number; a new size is a new constant with a reason in the pull request.
4. **State is passed in.** No new `thread_local!` or static in UI code. A component takes the few inputs it needs, not the application object.
5. **One owner per visible fact.** A fix to a banner, a font or a zoom lands once, in the part that owns it.
6. **Frontends render, services decide** ([ADR 0041](decisions/0041-frontends-render-services-decide.md)). GTK and GPUI share vocabulary and rules, not widget code.

`scripts/check-ui-patterns.py` holds rules 3 and 4, and hand-built dialog frames, at their current counts in `baselines/ui-pattern-baselines.txt`. Lower a count when you remove one; never raise it.
