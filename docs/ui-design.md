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
2. **Shared parts have one home.** Dialog shell, result view (text, list, table), list row, status line, notification centre. Zoom, the output font and spacing live in the part, not in each caller.
3. **Spacing comes from the scale:** `ui::spacing` holds 2, 4, 8, 12, 16, 24 as named constants. A margin or spacing is written as a constant, never a number; a new size is a new constant with a reason in the pull request.
4. **State is passed in.** No new `thread_local!` or static in UI code. A component takes the few inputs it needs, not the application object.
5. **One owner per visible fact.** A fix to a message, a font or a zoom lands once, in the part that owns it.
6. **Frontends render, services decide** ([ADR 0041](decisions/0041-frontends-render-services-decide.md)). GTK and GPUI share vocabulary and rules, not widget code.

## Where content goes (GPUI)

This section is the one source for where a GPUI surface opens. Other documents link here instead of restating it. A pull request that adds a surface names its row in this table.

| Content | Surface |
| --- | --- |
| Work: tables, queries, Home, Settings, history, audit, diagram, server views | A tab in the editor area. |
| Cell and row details, foreign-key peek | The right detail sidebar. |
| Commands | The command palette, anchored to the top of the window. |
| New connection, edit connection | A centred dialog. Test results show inside it. |
| Human approval | A centred dialog that nothing else can be typed behind. |
| One-shot steps: export to a file, import, SQL review before saving, structure changes, Set cells, the row form | A centred dialog. Escape and Cancel close it and change nothing. |
| Destructive actions: drop or empty a table, delete rows, drop a column | A centred dialog that names the object, shows the SQL and has no default button. A Prod connection also asks for approval. |
| Pending edits | A slim marker in the grid strip with the count, Save and Discard. It describes state, so it is not a dialog. |
| Messages: saved, imported, failed, unknown write outcome, transaction left open | The notification centre: an indicator with a count at the bottom right of the status bar that opens the list. |
| A warning before a write runs, such as "will change N rows" | Inside the dialog that reviews the write. |
| The result of a run: rows affected, lines changed, time taken | The status line of that result, as today. |

The notification centre:

- A success shows in the status bar for about 5 seconds, then stays in the list as read.
- An error, an unknown write outcome and an idle open transaction stay unread, and the count turns destructive, until the user resolves or dismisses them.
- A notification carries its own action: Retry, Resolve outcome, Commit and Roll back. Resolving opens the matching dialog.
- The list is reachable from the keyboard and the palette, and focus returns to where it was when it closes.

Nothing pushes the grid down to show a message: no banner rows, no toasts. A bar that stays above the grid (Find, filter, WHERE, compare) holds input, never a message.

## GPUI parts

In `apps/gpui`, these jobs have exactly one constructor in `surfaces.rs`. A view never builds its own, and never calls the library `Button::new` for them.

| Job | Part | Rules |
| --- | --- | --- |
| Icon-only button | `icon_button(id, icon, tooltip)` | The tooltip is required and is also the accessible name. Its size follows the bar it sits in. |
| Text button | `button(kind, id, label)` | Primary, Ghost or Destructive. One line, ending in an ellipsis. Busy and disabled states show the reason as a tooltip. Paired buttons share a minimum width. |
| One-of-N choice | `segmented(options, selected)` | Selection is a fill, never a ring, never blue. |
| Dialog and overlay | `scrim` and `dialog_surface(width)` | 6 px radius, a shadow, a scrim from the theme. Pages are flat and cast no shadow. Used for every centred dialog in the table above. |
| Task content | `task_bar(title)` | Fields, then the primary action, then Cancel, always in that order. One-shot steps put it inside a dialog. |
| Section caption | `section_header(caption)` | Sentence case, muted, small. |
| Empty, loading, failed | `list_state(...)` | One sentence, at most one action. Errors use the destructive colour; warnings use warning. |

- **One primary per bar or dialog**, and it comes before Cancel.
- **One height per bar.** Every control in a bar has the bar's control height. A bar is its control height plus padding from the scale.
- **Truncation.** A fixed-width label is one line with an ellipsis and a tooltip holding the full text. Secondary text (host, type) gives way before the primary name.
- **Colour roles only.** No `rgb(0x…)`, `rgba(0x…)` or `hsla(` outside `theme.rs`. Blue (`info`) marks a link and nothing else.
- **Contrast.** Text roles reach 4.5:1 and indicators 3:1 on every surface, in both themes (tested in `theme.rs`).
- **Screenshots.** A pull request that changes GPUI layout attaches light and dark screenshots at 1500 and 760 px.

`scripts/check-ui-patterns.py` holds rules 3 and 4, and hand-built dialog frames, at their current counts in `baselines/ui-pattern-baselines.txt`. For GPUI it also counts literal px sizes, raw colours, `text_xs`/`text_sm`, buttons with an empty label, `theme.info` and shadows outside `surfaces.rs`. Lower a count when you remove one; never raise it.
