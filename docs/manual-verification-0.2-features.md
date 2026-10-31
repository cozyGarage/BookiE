# Manual verification checklist

This checklist describes installed and native desktop scenarios that automated GTK tests cannot fully establish. Its checkboxes are steps to perform; run-specific outcomes belong in private verification records.

Test native Wayland separately from Xvfb; an Xvfb pass does not establish native Wayland behavior.

For each run, record the binary/package identity, GTK backend, OS and toolkit versions, scale, theme, outcomes and screenshots in a private verification record. Test normal and narrow window sizes, keyboard focus, clipboard and popover/dialog placement. Mark blocked and not-run items explicitly. This public checklist defines scenarios; run-specific records stay outside the public source tree.

Independent value checks compare database-native results with the app's display and supported write paths, using the database as an independent oracle where possible.

## Areas

| Area | Checklist |
|---|---|
| Connections and ssh | [manual-verification-connections.md](manual-verification-connections.md) |
| Sql editor and sessions | [manual-verification-editor.md](manual-verification-editor.md) |
| Grid, filters and data transfer | [manual-verification-grid.md](manual-verification-grid.md) |
| Catalog, sidebar and application shell | [manual-verification-app.md](manual-verification-app.md) |
