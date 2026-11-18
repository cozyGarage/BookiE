# UI review checklist

Use it on every pull request that touches a widget, in GTK or GPUI. A reviewer ticks what applies; an unchecked item needs a reason in the pull request. Tokens: [DESIGN.md](../DESIGN.md). Rules: [ui-design.md](ui-design.md).

Sources are BookiE (`AGENTS.md`, `docs/state-management.md`), gpui-component, dbx, the GNOME HIG, the libadwaita docs and Apple's HIG, restated in our words.

## Every view

Layout
- [ ] Colours are tokens; no hex in widget code except the environment palette and data colours.
- [ ] Radii and spacing come from the tokens and the scale.
- [ ] Related controls sit closer than groups, groups closer than sections. Siblings share one leading inset.
- [ ] No card inside a card. Hairlines and fills separate regions; only popovers, menus and dialogs cast a shadow.
- [ ] Each region has a minimum, a default and a rule for surplus space. A restored split size is clamped.
- [ ] Density is set for the whole surface, not one control. Text, icon, padding and hit target move together.

State and data
- [ ] Background work uses the one task spawner. A view holds the session, never a connection.
- [ ] One request per kind is current per tab; a reply to a replaced request is dropped; closing the tab cancels.
- [ ] A refresh never clears what it refreshes. Spinner only until the first load, then stale content with a banner and Retry.
- [ ] Saving goes through the one save route; Cancel on the review dialog shows nothing.

Interaction
- [ ] Every control has rest, hover, pressed, open, focus, selected, disabled, loading and error states.
- [ ] Every command is reachable by keyboard; focus returns after an overlay closes.
- [ ] An icon-only control has an accessible name and a tooltip with the shortcut.
- [ ] Status is never colour alone. Disabled and read-only look different.
- [ ] High contrast and reduced motion are honoured.
- [ ] A destructive action is undoable with a toast, or confirmed by a dialog that names the object.

Copy
- [ ] Sentence case, no final period; `…` only where the control opens a dialog or needs more input.
- [ ] The button names the result (`Delete`, `Discard changes`); `Cancel` leaves; no "Are you sure".
- [ ] An error says what happened and what to do next.
- [ ] Strings go through translation; no concatenated fragments.

## Anti-patterns to flag

- [ ] A filled button because it is the only action. Filled means one commitment per surface.
- [ ] Tinted toolbar icons per feature, or a row of multicolour badges.
- [ ] A strip of hover-only buttons instead of a menu; or everything hidden in menus so the main action disappears.
- [ ] A text-only, linked, suggested or destructive button in a GTK header bar.
- [ ] Icon plus label on a button outside a header bar or toolbar.
- [ ] A segmented control mixing icons and text, or mixing selection with actions.
- [ ] A menu over twelve items, a nested submenu, or actions mixed with check items in one group.
- [ ] Zoom by font size alone; a fixed-height button with a larger label.
- [ ] A custom height without moving the size tier.
- [ ] `thread_local!` or static view state; blocking work on the UI thread; a strong clone of a view in a callback the view owns.
- [ ] `choose_future` on an alert dialog; use the response helper.
- [ ] Theme colour on chrome or on a container of selectable rows.
- [ ] A hand-drawn widget where libadwaita has one.
- [ ] A box-shadow on base control chrome; it erases the focus ring.
- [ ] A disabled control that still reacts to hover.
- [ ] GPUI app code with a literal `px(..)` or `rounded_*()`; use the theme tiers.
