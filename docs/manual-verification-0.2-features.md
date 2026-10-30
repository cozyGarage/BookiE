# Manual verification checklist

This checklist describes installed and native desktop scenarios that automated GTK tests cannot fully establish. It is not milestone status; release scope is recorded in the [sprint](bookie-0.2-sprint.md), and subsequent work is tracked in the [backlog](backlog.md). The checkboxes summarize the recorded state at this snapshot. Run-specific environment details, outcomes and screenshots are kept in the private research archive.

Test native Wayland separately from Xvfb; an Xvfb pass does not establish native Wayland behavior.

For each run, record the binary/package identity, GTK backend, OS and toolkit versions, scale, theme, result and screenshot paths in the private evidence archive. Test normal and narrow window sizes, keyboard focus, clipboard and popover/dialog placement. Mark blocked and not-run items explicitly. Keep run results and screenshots out of this public checklist.

Independent value checks compare database-native results with the app's display and supported write paths. The [oracle review](oracle-review.md) explains the database-native comparison method.

## Areas

| Area | Checklist |
|---|---|
| Connections and ssh | [manual-verification-connections.md](manual-verification-connections.md) |
| Sql editor and sessions | [manual-verification-editor.md](manual-verification-editor.md) |
| Grid, filters and data transfer | [manual-verification-grid.md](manual-verification-grid.md) |
| Catalog, sidebar and application shell | [manual-verification-app.md](manual-verification-app.md) |
