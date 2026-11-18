# Design tokens

The values both apps use. Rules and process: [docs/ui-design.md](docs/ui-design.md). Review lists: [docs/ui-review-checklist.md](docs/ui-review-checklist.md). Decision: [ADR 0054](docs/decisions/0054-one-visual-language.md).

Token values follow the [dbx](https://github.com/t8y2/dbx) desktop styles (`apps/desktop/src/styles/tokens.css`, Apache-2.0). The control, toolbar and menu language follows TablePro (AGPL-3.0-or-later) and Apple's and GNOME's guidelines, restated in our own words.

## Decisions

- **The filled button is neutral**, near-black in light and light grey in dark, as in dbx. The system accent is for focus, text selection and the connection-environment dots only. Blue is not decoration.
- **Two control scales, as dbx has:** form controls and dialog buttons are 32 px; toolbars, tab bars, tree rows and menus are compact at 24 px with 14 px icons. A surface uses one scale.
- **Toolbar icons are muted foreground**, never tinted per feature.
- **GPUI expresses sizes as theme tokens** (size tiers), not literal pixels; its radius and font defaults are overridden to 4 and 6.

## Colour

| Role | Light | Dark |
| --- | --- | --- |
| Background | `#ffffff` | `#131416` |
| Surface (card, dialog) | `#ffffff` | `#1b1b1e` |
| Popover | `#ffffff` | `#1e1e20` |
| Sidebar | `#fafafa` | `#19191c` |
| Bars (toolbar, tab bar) | `#f5f5f5` | `#1b1b1e` |
| Border | `#e5e5e5` | `#6e6e72` at 28% |
| Text | `#0a0a0a` | `#d7d7db` |
| Muted text | `#737373` | `#97989d` |
| Muted fill | `#f5f5f5` | `#2a2a2d` |
| Filled button, its text | `#171717`, `#fafafa` | `#d0d0d6`, `#131416` |
| Hover and selected fill | `#f5f5f5` | `#2e2f33` |
| Success | `#16a34a` | `#4ade80` |
| Warning | `#d97706` | `#fbbf24` |
| Info | `#2563eb` | `#60a5fa` |
| Destructive | `#e7000b` | `#f3625f` |
| Focus ring | `#a1a1a1` | `#85868b` |

Status tints are the colour mixed into the background at 12% (light) or 16% (dark). The connection-environment palette is the only literal colour set in widget code: blue `#3584e4`, teal `#2190a4`, green `#3a944a`, yellow `#c88800`, orange `#ed5b00`, red `#e62d42`, pink `#d56199`, purple `#9141ac`, slate `#6f8396`.

## Shape and size

| Token | Value |
| --- | --- |
| Radius, control | 4 px |
| Radius, surface | 6 px |
| Radius, badge | full |
| Control height, form | 32 px (28 small, 24 extra small) |
| Control height, compact | 24 px, icon 14 px |
| Toolbar bar | 36 px, 4 px gap, 12 px side padding |
| Tab, tree row | 28 px (24 px compact) |
| Grid row | 26 px |
| Segmented control | 20 px segments in a 2 px padded 24 px group |
| Badge | 20 px high, 12 px icon, 8 px side padding |
| Spacing scale | 2 optical, 4 inside a control, 8 related controls, 12 one group, 16 groups, 24 sections, 32 regions |
| Type | UI 14 px, grid 12.5 px with tabular numbers, labels and shortcuts 12 px, status bar 0.9 em |
| Motion | 0, 120, 180, 280 ms; honour reduced motion |

## States

| State | Treatment |
| --- | --- |
| Hover | muted fill; flat controls get no border |
| Pressed or menu open | accent-neutral fill; a menu button stays pressed while its menu is open |
| Focus | 1 px ring colour plus a 3 px ring at 50%; never removed |
| Selected | the item's own fill and stronger text; no side bar |
| Disabled | 50% opacity, no hover |
| Tooltip | inverted colours, 12 px, carries the name and the shortcut |
| Elevation | none, except popover, menu and dialog (1 px ring, soft shadow) |
